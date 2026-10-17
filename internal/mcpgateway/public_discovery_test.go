package mcpgateway

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func discoveryRequest(method string) *http.Request {
	return httptest.NewRequest(http.MethodPost, "/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"`+method+`","params":{}}`))
}

func TestPublicDiscoveryRequestHandlesUnknownContentLength(t *testing.T) {
	req := discoveryRequest("tools/list")
	req.ContentLength = -1
	if !publicDiscoveryRequest(req) {
		t.Fatal("tools/list with unknown Content-Length should remain discoverable")
	}
}

func TestPublicDiscoveryRequestAllowsOnlyCatalogHandshake(t *testing.T) {
	for _, method := range []string{"initialize", "server/discover", "notifications/initialized", "tools/list", "ping"} {
		req := discoveryRequest(method)
		if !publicDiscoveryRequest(req) {
			t.Fatalf("%s should be public discovery", method)
		}
	}
	for _, method := range []string{"tools/call", "resources/list", "prompts/list"} {
		req := discoveryRequest(method)
		if publicDiscoveryRequest(req) {
			t.Fatalf("%s must remain OAuth-protected", method)
		}
	}

	batch := httptest.NewRequest(http.MethodPost, "/mcp", strings.NewReader(`[
		{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{}},
		{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
	]`))
	if !publicDiscoveryRequest(batch) {
		t.Fatal("all-discovery batch should be public")
	}
	mixed := httptest.NewRequest(http.MethodPost, "/mcp", strings.NewReader(`[
		{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}},
		{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"workspace","arguments":{"action":"list"}}}
	]`))
	if publicDiscoveryRequest(mixed) {
		t.Fatal("batch containing tools/call must remain OAuth-protected")
	}
}

func TestPublicDiscoveryOrProtectedRoutesDocsCompliantAuthChallenge(t *testing.T) {
	var publicCalls atomic.Int32
	var challengeCalls atomic.Int32
	var protectedCalls atomic.Int32
	public := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		publicCalls.Add(1)
		w.WriteHeader(http.StatusOK)
	})
	challenge := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		challengeCalls.Add(1)
		w.WriteHeader(http.StatusOK)
	})
	protected := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		protectedCalls.Add(1)
		w.WriteHeader(http.StatusUnauthorized)
	})
	handler := PublicDiscoveryOrProtected(public, challenge, protected)

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, discoveryRequest("tools/list"))
	if rec.Code != http.StatusOK || publicCalls.Load() != 1 || challengeCalls.Load() != 0 || protectedCalls.Load() != 0 {
		t.Fatalf("tools/list routing status=%d public=%d challenge=%d protected=%d", rec.Code, publicCalls.Load(), challengeCalls.Load(), protectedCalls.Load())
	}

	rec = httptest.NewRecorder()
	handler.ServeHTTP(rec, discoveryRequest("tools/call"))
	if rec.Code != http.StatusOK || challengeCalls.Load() != 1 || protectedCalls.Load() != 0 {
		t.Fatalf("unauthenticated tools/call must return MCP auth challenge: status=%d challenge=%d protected=%d", rec.Code, challengeCalls.Load(), protectedCalls.Load())
	}

	req := discoveryRequest("tools/call")
	req.Header.Set("Authorization", "Bearer token")
	rec = httptest.NewRecorder()
	handler.ServeHTTP(rec, req)
	if rec.Code != http.StatusUnauthorized || protectedCalls.Load() != 1 {
		t.Fatalf("authenticated tools/call must stay protected: status=%d protected=%d", rec.Code, protectedCalls.Load())
	}
}

func publicDiscoveryFixture(t *testing.T) *httptest.Server {
	t.Helper()
	s := &Service{
		servers:      map[string]*mcp.Server{},
		routes:       map[string]map[string]string{},
		shownUpdates: map[string]map[string]struct{}{},
	}
	return httptest.NewServer(s.PublicDiscoveryHandler())
}

func TestPublicDiscoveryHandlerListsCompactToolsWithoutOAuthClaims(t *testing.T) {
	httpServer := publicDiscoveryFixture(t)
	defer httpServer.Close()

	client := mcp.NewClient(&mcp.Implementation{Name: "codelocal-public-discovery-test", Version: "1"}, nil)
	session, err := client.Connect(context.Background(), &mcp.StreamableClientTransport{Endpoint: httpServer.URL + "/mcp"}, nil)
	if err != nil {
		t.Fatalf("connect public discovery MCP client: %v", err)
	}
	defer session.Close()

	result, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatalf("public tools/list failed: %v", err)
	}
	if len(result.Tools) != len(compactToolDefinitions()) || len(result.Tools) != 14 {
		t.Fatalf("public discovery returned %d tools, want 14", len(result.Tools))
	}

	var workspace *mcp.Tool
	for _, tool := range result.Tools {
		if tool.Name == "workspace" {
			workspace = tool
			break
		}
	}
	if workspace == nil {
		t.Fatal("public discovery must advertise workspace tool")
	}
	rawSchema, err := json.Marshal(workspace.InputSchema)
	if err != nil {
		t.Fatalf("marshal workspace input schema: %v", err)
	}
	var schema struct {
		Properties map[string]struct {
			Enum []string `json:"enum"`
		} `json:"properties"`
	}
	if err := json.Unmarshal(rawSchema, &schema); err != nil {
		t.Fatalf("decode workspace input schema: %v", err)
	}
	contains := func(values []string, want string) bool {
		for _, value := range values {
			if value == want {
				return true
			}
		}
		return false
	}
	if !contains(schema.Properties["action"].Enum, "execution") {
		t.Fatalf("public workspace actions=%v; execution mode selection would deadlock stale clients", schema.Properties["action"].Enum)
	}
	if got := schema.Properties["executionMode"].Enum; !reflect.DeepEqual(got, []string{"safe", "live"}) {
		t.Fatalf("public executionMode choices=%v want [safe live]", got)
	}
}

func TestPublicDiscoveryNormalizesIncompleteOpenAIModernToolsList(t *testing.T) {
	httpServer := publicDiscoveryFixture(t)
	defer httpServer.Close()

	req, err := http.NewRequest(http.MethodPost, httpServer.URL+"/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}`))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	req.Header.Set("Mcp-Protocol-Version", modernMCPProtocolVersion)
	// Deliberately omit Mcp-Method and all 2026 request _meta fields. The
	// compatibility layer must repair this scanner request before SDK validation.
	response, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		t.Fatalf("normalized tools/list status=%d want 200", response.StatusCode)
	}
	if got := response.Header.Get("X-CodeLocal-MCP-Transport"); got != "stateless-discovery" {
		t.Fatalf("transport=%q want stateless-discovery", got)
	}
	if got := response.Header.Get("Cache-Control"); !strings.Contains(got, "no-store") {
		t.Fatalf("public discovery cache control=%q want no-store", got)
	}
}
