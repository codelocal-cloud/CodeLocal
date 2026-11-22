package mcpgateway

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestCompactToolsAdvertiseOAuthSecuritySchemes(t *testing.T) {
	for _, def := range compactToolDefinitions() {
		raw, ok := def.Meta["securitySchemes"]
		if !ok {
			t.Fatalf("%s missing _meta.securitySchemes", def.Name)
		}
		encoded, err := json.Marshal(raw)
		if err != nil {
			t.Fatalf("%s security schemes marshal: %v", def.Name, err)
		}
		if string(encoded) != `[{"scopes":["mcp:tools"],"type":"oauth2"}]` {
			t.Fatalf("%s security schemes=%s", def.Name, encoded)
		}
	}
}

func TestToolSecurityCompatibilityHandlesUnknownContentLength(t *testing.T) {
	next := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"workspace","inputSchema":{"type":"object"},"_meta":{}}]}}`))
	})
	handler := ToolSecurityCompatibility(next)
	req := httptest.NewRequest(http.MethodPost, "/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}`))
	req.ContentLength = -1
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status=%d body=%s", rec.Code, rec.Body.String())
	}
	var envelope map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &envelope); err != nil {
		t.Fatal(err)
	}
	result := envelope["result"].(map[string]any)
	tool := result["tools"].([]any)[0].(map[string]any)
	if tool["securitySchemes"] == nil {
		t.Fatalf("top-level securitySchemes missing for unknown Content-Length: %#v", tool)
	}
}

func TestRewriteToolSecuritySchemesAddsTopLevelAndMeta(t *testing.T) {
	raw := []byte(`{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"workspace","inputSchema":{"type":"object"},"_meta":{"existing":true}}]}}`)
	rewritten, changed := rewriteToolSecuritySchemes(raw)
	if !changed {
		t.Fatal("tools/list response was not rewritten")
	}
	var envelope map[string]any
	if err := json.Unmarshal(rewritten, &envelope); err != nil {
		t.Fatal(err)
	}
	result := envelope["result"].(map[string]any)
	tool := result["tools"].([]any)[0].(map[string]any)
	if _, ok := tool["securitySchemes"]; !ok {
		t.Fatalf("top-level securitySchemes missing: %#v", tool)
	}
	meta := tool["_meta"].(map[string]any)
	if meta["existing"] != true || meta["securitySchemes"] == nil {
		t.Fatalf("_meta compatibility fields missing: %#v", meta)
	}
}
