package mcpgateway

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

const publicCatalogUserID = "__codelocal_public_catalog__"

type publicDiscoveryResponseWriter struct {
	http.ResponseWriter
}

func (w *publicDiscoveryResponseWriter) applyNoStore() {
	w.Header().Set("Cache-Control", "no-store, no-cache, no-transform, max-age=0")
	w.Header().Set("Pragma", "no-cache")
}

func (w *publicDiscoveryResponseWriter) WriteHeader(statusCode int) {
	w.applyNoStore()
	w.ResponseWriter.WriteHeader(statusCode)
}

func (w *publicDiscoveryResponseWriter) Write(body []byte) (int, error) {
	w.applyNoStore()
	return w.ResponseWriter.Write(body)
}

// Unwrap keeps http.ResponseController support used by the MCP SDK while we
// enforce discovery cache headers at the final response-write boundary.
func (w *publicDiscoveryResponseWriter) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}

var publicDiscoveryMethods = map[string]struct{}{
	"initialize":                {},
	"server/discover":           {},
	"notifications/initialized": {},
	"tools/list":                {},
	"ping":                      {},
}

func publicDiscoveryEnvelope(value any) bool {
	switch typed := value.(type) {
	case []any:
		if len(typed) == 0 {
			return false
		}
		for _, item := range typed {
			if !publicDiscoveryEnvelope(item) {
				return false
			}
		}
		return true
	case map[string]any:
		method, _ := typed["method"].(string)
		_, ok := publicDiscoveryMethods[method]
		return ok
	default:
		return false
	}
}

func readPublicDiscoveryEnvelope(r *http.Request) ([]byte, any, bool) {
	if r == nil || r.URL.Path != "/mcp" || r.Method != http.MethodPost || r.Body == nil {
		return nil, nil, false
	}
	raw, err := readBoundedMCPRequestBody(r)
	if err != nil {
		return nil, nil, false
	}

	var envelope any
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	if decoder.Decode(&envelope) != nil || !publicDiscoveryEnvelope(envelope) {
		return raw, envelope, false
	}
	return raw, envelope, true
}

func publicDiscoveryRequest(r *http.Request) bool {
	_, _, ok := readPublicDiscoveryEnvelope(r)
	return ok
}

func normalizePublicDiscoveryMessage(message map[string]any, header http.Header) {
	method, _ := message["method"].(string)
	if method == "" {
		return
	}
	if header.Get("Mcp-Method") == "" {
		header.Set("Mcp-Method", method)
	}

	params, _ := message["params"].(map[string]any)
	if params == nil {
		params = map[string]any{}
		message["params"] = params
	}
	meta, _ := params["_meta"].(map[string]any)
	if meta == nil {
		meta = map[string]any{}
		params["_meta"] = meta
	}

	version := header.Get("Mcp-Protocol-Version")
	if version == "" {
		version, _ = meta[mcp.MetaKeyProtocolVersion].(string)
	}
	if version == "" && method == "server/discover" {
		version = modernMCPProtocolVersion
	}
	if !isModernMCPProtocolVersion(version) {
		return
	}
	if header.Get("Mcp-Protocol-Version") == "" {
		header.Set("Mcp-Protocol-Version", version)
	}
	if _, ok := meta[mcp.MetaKeyProtocolVersion].(string); !ok {
		meta[mcp.MetaKeyProtocolVersion] = version
	}
	if _, ok := meta[mcp.MetaKeyClientInfo]; !ok {
		meta[mcp.MetaKeyClientInfo] = map[string]any{"name": "openai-discovery-compat", "version": "1"}
	}
	if _, ok := meta[mcp.MetaKeyClientCapabilities]; !ok {
		meta[mcp.MetaKeyClientCapabilities] = map[string]any{}
	}
}

func normalizePublicDiscoveryRequest(r *http.Request) bool {
	_, envelope, ok := readPublicDiscoveryEnvelope(r)
	if !ok {
		return false
	}

	switch typed := envelope.(type) {
	case map[string]any:
		normalizePublicDiscoveryMessage(typed, r.Header)
	case []any:
		for _, item := range typed {
			if message, ok := item.(map[string]any); ok {
				normalizePublicDiscoveryMessage(message, r.Header)
			}
		}
	}

	raw, err := json.Marshal(envelope)
	if err != nil {
		return false
	}
	r.Body = io.NopCloser(bytes.NewReader(raw))
	r.ContentLength = int64(len(raw))
	return true
}

// PublicDiscoveryHandler exposes only the static MCP catalog handshake. It is
// intentionally stateless and normalizes OpenAI scanner requests before they
// reach the strict Go MCP SDK 2026 transport validator. No real CodeLocal user,
// workspace or runtime route exists on this handler.
func (s *Service) PublicDiscoveryHandler() http.Handler {
	stream := mcp.NewStreamableHTTPHandler(func(*http.Request) *mcp.Server {
		return s.serverFor(publicCatalogUserID)
	}, &mcp.StreamableHTTPOptions{
		Stateless:                    true,
		JSONResponse:                 true,
		MaxRequestBodyBytes:          4 << 20,
		PropagateRequestCancellation: true,
	})
	surface := PublicToolSurface()
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !normalizePublicDiscoveryRequest(r) {
			http.Error(w, "authentication required", http.StatusUnauthorized)
			return
		}
		// Tool schemas are versioned and may change between deploys. Prevent HTTP
		// clients/proxies from reusing a stale tools/list response after a surface
		// generation bump (for example when workspace(action=execution) was added).
		response := &publicDiscoveryResponseWriter{ResponseWriter: w}
		response.applyNoStore()
		response.Header().Set("X-CodeLocal-MCP-Transport", "stateless-discovery")
		response.Header().Set("X-CodeLocal-Tool-Surface-Version", fmt.Sprint(surface.Version))
		response.Header().Set("X-CodeLocal-Tool-Surface-Hash", surface.Hash)
		stream.ServeHTTP(response, r)
	})
}

// PublicDiscoveryOrProtected always serves static discovery through the public
// compatibility handler. An unauthenticated tools/call receives the MCP-level
// OAuth challenge required by ChatGPT/OpenAI docs; authenticated execution and
// all other non-discovery requests remain behind the strict OAuth middleware.
func PublicDiscoveryOrProtected(public, authChallenge, protected http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r != nil && publicDiscoveryRequest(r) {
			public.ServeHTTP(w, r)
			return
		}
		if r != nil && r.Header.Get("Authorization") == "" && unauthenticatedToolCallRequest(r) {
			authChallenge.ServeHTTP(w, r)
			return
		}
		protected.ServeHTTP(w, r)
	})
}
