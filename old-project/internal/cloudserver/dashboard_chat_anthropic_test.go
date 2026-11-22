package cloudserver

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestCallAnthropicMessagesWithToolsUsesMessagesProtocol(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/messages" {
			t.Fatalf("path=%q want /messages", r.URL.Path)
		}
		if got := r.Header.Get("x-api-key"); got != "secret-key" {
			t.Fatalf("x-api-key=%q", got)
		}
		if got := r.Header.Get("anthropic-version"); got != dashboardAnthropicVersion {
			t.Fatalf("anthropic-version=%q", got)
		}
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatal(err)
		}
		if body["model"] != "claude-test" {
			t.Fatalf("model=%#v", body["model"])
		}
		if _, ok := body["messages"].([]any); !ok {
			t.Fatalf("messages missing: %#v", body)
		}
		if _, ok := body["tools"].([]any); !ok {
			t.Fatalf("tools missing: %#v", body)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"content":[{"type":"text","text":"done"},{"type":"tool_use","id":"tool_1","name":"read","input":{"path":"README.md"}}]}`))
	}))
	defer server.Close()

	messages := []map[string]any{
		{"role": "system", "content": "Be concise."},
		{"role": "user", "content": "Inspect the project."},
	}
	tools := []map[string]any{{
		"type": "function",
		"function": map[string]any{
			"name":        "read",
			"description": "Read a file",
			"parameters":  map[string]any{"type": "object", "properties": map[string]any{"path": map[string]any{"type": "string"}}},
		},
	}}

	calls, content, err := callAnthropicMessagesWithTools(server.URL, "secret-key", "claude-test", messages, tools)
	if err != nil {
		t.Fatal(err)
	}
	if content != "done" {
		t.Fatalf("content=%q", content)
	}
	if len(calls) != 1 || calls[0].ID != "tool_1" || calls[0].Name != "read" || calls[0].Arguments != `{"path":"README.md"}` {
		t.Fatalf("calls=%#v", calls)
	}
}
