package cloudmcp

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func validPlaneSprintBatch() map[string]any {
	return map[string]any{
		"project_id": "2ec86630-f40c-4c39-be78-c22c88780517",
		"cycle_name": "Sprint 1 — SOCO Mobile First",
		"owned_by":   "b697cd13-8d70-4dd7-bf68-6690ff2379ca",
		"items": []any{
			map[string]any{"key": "S1-MOB-01", "name": "[S1-MOB-01] Restore mobile Git baseline", "description": "Acceptance: protect dirty files and verify main baseline.", "priority": "high"},
			map[string]any{"key": "S1-MOB-02", "name": "[S1-MOB-02] Test mobile layouts", "description": "Acceptance: verify iOS and Android mobile routes.", "priority": "medium"},
		},
	}
}

func TestPlaneSprintBatchValidation(t *testing.T) {
	arguments := validPlaneSprintBatch()
	req, err := ValidatePlaneSprintBatch(arguments)
	if err != nil || len(req.Items) != 2 {
		t.Fatalf("valid batch rejected: %+v, %v", req, err)
	}
	copyArguments := func() map[string]any {
		raw, _ := json.Marshal(validPlaneSprintBatch())
		var value map[string]any
		_ = json.Unmarshal(raw, &value)
		return value
	}
	for _, scenario := range []struct {
		name  string
		alter func(map[string]any)
	}{
		{"cross-project-credential", func(m map[string]any) { m["project_id"] = "other" }},
		{"malformed-owner", func(m map[string]any) { m["owned_by"] = "not-a-uuid" }},
		{"unknown-field", func(m map[string]any) { m["admin_pat"] = "leak" }},
		{"duplicate-id", func(m map[string]any) { m["items"].([]any)[1].(map[string]any)["key"] = "S1-MOB-01" }},
		{"bad-key", func(m map[string]any) { m["items"].([]any)[0].(map[string]any)["key"] = "../../x" }},
		{"unapproved-priority", func(m map[string]any) { m["items"].([]any)[0].(map[string]any)["priority"] = "critical" }},
		{"empty-items", func(m map[string]any) { m["items"] = []any{} }},
		{"oversized-task", func(m map[string]any) {
			m["items"].([]any)[0].(map[string]any)["description"] = strings.Repeat("x", 2200)
		}},
	} {
		t.Run(scenario.name, func(t *testing.T) {
			input := copyArguments()
			scenario.alter(input)
			if _, err := ValidatePlaneSprintBatch(input); err == nil {
				t.Fatal("invalid or potentially unsafe batch accepted")
			}
		})
	}
}

func TestPlaneSprintBatchResponseParsing(t *testing.T) {
	res := &mcp.CallToolResult{StructuredContent: map[string]any{
		"result": map[string]any{"results": []any{map[string]any{"id": "2ec86630-f40c-4c39-be78-c22c88780517", "name": "SocCo"}}},
	}}
	obj, err := planeMCPObject(res)
	if err != nil {
		t.Fatal(err)
	}
	rows, err := planeMCPItems(obj)
	if err != nil || len(rows) != 1 {
		t.Fatal("MCP structuredContent nested result was not parsed", err)
	}
	if planeMCPID(rows[0]) != "2ec86630-f40c-4c39-be78-c22c88780517" {
		t.Fatal("identifier extraction failed")
	}
	if planeMCPID(map[string]any{"id": "not-a-uuid"}) != "" {
		t.Fatal("invalid provider ID accepted")
	}
	if _, err = planeMCPItems(map[string]any{}); err == nil {
		t.Fatal("unknown response shape treated as empty results and could duplicate tasks")
	}
	if _, err = planeMCPObject(&mcp.CallToolResult{IsError: true}); err == nil {
		t.Fatal("provider failure must abort batch")
	}
}

func TestPlaneSprintBatchRejectsUntrustedEndpoints(t *testing.T) {
	m := NewManager()
	_, err := m.SetupPlaneSprint(t.Context(), "user", Config{Endpoint: "https://evil.example/mcp", WorkspaceSlug: "socco"}, validPlaneSprintBatch())
	if err == nil || !strings.Contains(err.Error(), "trusted Plane") {
		t.Fatalf("untrusted origin accepted: %v", err)
	}
}
