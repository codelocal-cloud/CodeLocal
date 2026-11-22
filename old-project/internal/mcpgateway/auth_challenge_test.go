package mcpgateway

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestMCPAuthChallengeHandlerReturnsOpenAIMetaChallenge(t *testing.T) {
	handler := MCPAuthChallengeHandler("https://codelocal.cloud/.well-known/oauth-protected-resource")
	rec := httptest.NewRecorder()
	req := discoveryRequest("tools/call")
	req.ContentLength = -1
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status=%d want 200; body=%s", rec.Code, rec.Body.String())
	}
	var envelope map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &envelope); err != nil {
		t.Fatal(err)
	}
	result, ok := envelope["result"].(map[string]any)
	if !ok || result["isError"] != true {
		t.Fatalf("expected MCP tool error result: %#v", envelope)
	}
	meta, ok := result["_meta"].(map[string]any)
	if !ok {
		t.Fatalf("missing result _meta: %#v", result)
	}
	values, ok := meta["mcp/www_authenticate"].([]any)
	if !ok || len(values) != 1 {
		t.Fatalf("missing mcp/www_authenticate: %#v", meta)
	}
	challenge, _ := values[0].(string)
	for _, required := range []string{
		`resource_metadata="https://codelocal.cloud/.well-known/oauth-protected-resource"`,
		`scope="mcp:tools"`,
		`error="insufficient_scope"`,
		`error_description="Connect your CodeLocal account to use this tool"`,
	} {
		if !strings.Contains(challenge, required) {
			t.Fatalf("challenge %q missing %q", challenge, required)
		}
	}
}

func TestMCPAuthChallengeHandlerRejectsNonToolCall(t *testing.T) {
	handler := MCPAuthChallengeHandler("https://codelocal.cloud/.well-known/oauth-protected-resource")
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, discoveryRequest("resources/list"))
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("status=%d want 401", rec.Code)
	}
}
