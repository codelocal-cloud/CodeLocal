package mcpgateway

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/cloudmcp"
)

const testPlaneProjectUUID = "2ec86630-f40c-4c39-be78-c22c88780517"
const testPlaneOwnerUUID = "b697cd13-8d70-4dd7-bf68-6690ff2379ca"

func TestPlaneAutoPolicyPermitsOnlyRoutineOperations(t *testing.T) {
	project := testPlaneProjectUUID
	owner := testPlaneOwnerUUID
	cases := []struct {
		name    string
		tool    string
		args    map[string]any
		allowed bool
	}{
		{"list projects", "project", map[string]any{"action": "list"}, true},
		{"retrieve project", "project", map[string]any{"action": "retrieve", "project_id": project}, true},
		{"delete project", "project", map[string]any{"action": "delete", "project_id": project}, false},
		{"create project", "project", map[string]any{"action": "create", "name": "Unauthorized"}, false},
		{"list sprint", "cycle", map[string]any{"action": "list", "project_id": project}, true},
		{"create sprint", "cycle", map[string]any{"action": "create", "project_id": project, "owned_by": owner, "name": "Mobile Sprint"}, true},
		{"create unscoped sprint", "cycle", map[string]any{"action": "create", "name": "Mobile Sprint", "owned_by": owner}, false},
		{"attach cycle workitems", "cycle", map[string]any{"action": "manage_workitems", "project_id": project, "cycle_id": project, "add_ids": owner}, true},
		{"detach cycle workitems", "cycle", map[string]any{"action": "manage_workitems", "project_id": project, "cycle_id": project, "add_ids": owner, "remove_ids": project}, false},
		{"invalid cycle ids", "cycle", map[string]any{"action": "manage_workitems", "project_id": project, "cycle_id": project, "add_ids": "foo"}, false},
		{"remove cycle", "cycle", map[string]any{"action": "delete", "project_id": project}, false},
		{"list workitem", "workitem", map[string]any{"action": "list", "project_id": project}, true},
		{"create task", "workitem", map[string]any{"action": "create", "project_id": project, "name": "Fix mobile notification"}, true},
		{"create without project", "workitem", map[string]any{"action": "create", "name": "Task"}, false},
		{"update task content", "workitem", map[string]any{"action": "update", "project_id": project, "workitem_id": owner, "priority": "high"}, true},
		{"update task assignments", "workitem", map[string]any{"action": "update", "project_id": project, "workitem_id": owner, "assignees": []string{owner}}, false},
		{"update parent", "workitem", map[string]any{"action": "update", "project_id": project, "workitem_id": owner, "parent": project}, false},
		{"delete task", "workitem", map[string]any{"action": "delete", "project_id": project, "workitem_id": owner}, false},
		{"archive task", "workitem", map[string]any{"action": "archive", "project_id": project, "workitem_id": owner}, false},
		{"manage member", "member", map[string]any{"action": "update", "member_id": owner}, false},
		{"list member", "member", map[string]any{"action": "list"}, true},
		{"read labels", "label", map[string]any{"action": "list", "project_id": project}, true},
		{"create labels", "label", map[string]any{"action": "create", "project_id": project}, false},
		{"unspecified action", "workitem", map[string]any{"project_id": project}, false},
		{"unknown tool", "custom_powerful", map[string]any{"action": "list"}, false},
		{"malformed args", "workitem", nil, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := planeAutoActionAllowed(tc.tool, tc.args); got != tc.allowed {
				t.Fatalf("Plane %s %v was allowed=%v, expected %v", tc.tool, tc.args, got, tc.allowed)
			}
		})
	}
}

func TestPlaneAutoPolicySupportsValidatedSprintBatchWithoutExtraApproval(t *testing.T) {
	arguments := map[string]any{
		"project_id": testPlaneProjectUUID, "cycle_name": "Sprint 1 — SOCO Mobile First", "owned_by": testPlaneOwnerUUID,
		"items": []any{
			map[string]any{"key": "S1-MOB-01", "name": "[S1-MOB-01] Test iOS PWA login", "description": "Acceptance: OTP works on iOS Safari and PWA.", "priority": "high"},
		},
	}
	if !planeAutoActionAllowed(cloudmcp.PlaneSprintBatchToolName, arguments) {
		t.Fatal("Validated owner-scoped batch should be callable without extra browser approval")
	}
	arguments["items"] = []any{
		map[string]any{"key": "../bad", "name": "task", "description": "nope", "priority": "urgent"},
	}
	if planeAutoActionAllowed(cloudmcp.PlaneSprintBatchToolName, arguments) {
		t.Fatal("Invalid batch must not be auto-authorized")
	}
}

func TestPlaneAutoActionFailsClosedWhenCloudNotConfigured(t *testing.T) {
	s := &Service{}
	r, err := s.callAutoPlaneCloudMCP(context.Background(), "user", cloud.PluginConnection{}, cloudmcp.Config{}, nil, map[string]any{
		"tool": "project", "arguments": map[string]any{"action": "list"},
	})
	if err != nil || r == nil || !r.IsError {
		t.Fatalf("Missing tools or Cloud store should fail closed: %v", err)
	}
	raw, _ := json.Marshal(r.Content)
	if strings.Contains(string(raw), "approvalUrl") {
		t.Fatal("Direct read may not create approval URLs")
	}
}
