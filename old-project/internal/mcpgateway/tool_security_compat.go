package mcpgateway

import (
	"bytes"
	"encoding/json"
	"errors"
	"net/http"
)

func chatGPTOAuthSecuritySchemes() []any {
	return []any{map[string]any{"type": "oauth2", "scopes": []any{"mcp:tools"}}}
}

func containsToolsListRequest(value any) bool {
	switch typed := value.(type) {
	case []any:
		for _, item := range typed {
			if containsToolsListRequest(item) {
				return true
			}
		}
	case map[string]any:
		method, _ := typed["method"].(string)
		return method == "tools/list"
	}
	return false
}

func injectToolSecuritySchemes(value any) bool {
	changed := false
	switch typed := value.(type) {
	case []any:
		for _, item := range typed {
			if injectToolSecuritySchemes(item) {
				changed = true
			}
		}
	case map[string]any:
		result, _ := typed["result"].(map[string]any)
		tools, _ := result["tools"].([]any)
		for _, item := range tools {
			tool, _ := item.(map[string]any)
			if tool == nil {
				continue
			}
			tool["securitySchemes"] = chatGPTOAuthSecuritySchemes()
			meta, _ := tool["_meta"].(map[string]any)
			if meta == nil {
				meta = map[string]any{}
				tool["_meta"] = meta
			}
			meta["securitySchemes"] = chatGPTOAuthSecuritySchemes()
			changed = true
		}
	}
	return changed
}

func rewriteToolSecuritySchemes(raw []byte) ([]byte, bool) {
	var envelope any
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	if decoder.Decode(&envelope) != nil || envelope == nil {
		return raw, false
	}
	if !injectToolSecuritySchemes(envelope) {
		return raw, false
	}
	rewritten, err := json.Marshal(envelope)
	if err != nil {
		return raw, false
	}
	return rewritten, true
}

// ToolSecurityCompatibility adds the OpenAI tool-level OAuth descriptor fields
// to tools/list responses. The upstream Go MCP SDK currently serializes _meta
// but does not expose the top-level securitySchemes field that ChatGPT uses to
// bind discovered actions to the account that just completed OAuth.
func ToolSecurityCompatibility(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/mcp" || r.Method != http.MethodPost || r.Body == nil {
			next.ServeHTTP(w, r)
			return
		}
		raw, err := readBoundedMCPRequestBody(r)
		if err != nil {
			if errors.Is(err, errMCPRequestBodyTooLarge) {
				http.Error(w, "MCP request body too large", http.StatusRequestEntityTooLarge)
				return
			}
			next.ServeHTTP(w, r)
			return
		}
		if !containsToolsListRequest(func() any {
			var value any
			decoder := json.NewDecoder(bytes.NewReader(raw))
			decoder.UseNumber()
			_ = decoder.Decode(&value)
			return value
		}()) {
			next.ServeHTTP(w, r)
			return
		}

		buffered := newLegacyCompatibilityResponseWriter()
		next.ServeHTTP(buffered, r)
		for key, values := range buffered.header {
			for _, value := range values {
				w.Header().Add(key, value)
			}
		}
		w.Header().Del("Content-Length")
		body := buffered.body.Bytes()
		if rewritten, changed := rewriteToolSecuritySchemes(body); changed {
			body = rewritten
		}
		status := buffered.status
		if status == 0 {
			status = http.StatusOK
		}
		w.WriteHeader(status)
		_, _ = w.Write(body)
	})
}
