package cloudserver

import "testing"

func TestDashboardChatThreadWorkspaceConflict(t *testing.T) {
	if dashboardChatThreadWorkspaceConflict("device-a::project-a", "device-a::project-a") {
		t.Fatal("same project must remain allowed")
	}
	if !dashboardChatThreadWorkspaceConflict("device-a::project-a", "device-a::project-b") {
		t.Fatal("bound thread must reject a different project")
	}
	if dashboardChatThreadWorkspaceConflict("", "device-a::project-b") {
		t.Fatal("unbound legacy thread may bind once")
	}
	if dashboardChatThreadWorkspaceConflict("device-a::project-a", "") {
		t.Fatal("omitted request workspace should reuse the bound thread project")
	}
}

func TestDashboardChatWorkspaceFromKey(t *testing.T) {
	workspace := dashboardChatWorkspaceFromKey("device-a::project-a")
	if workspace == nil || workspace.DeviceID != "device-a" || workspace.WorkspaceID != "project-a" {
		t.Fatalf("unexpected workspace: %#v", workspace)
	}
	if dashboardChatWorkspaceFromKey("invalid") != nil {
		t.Fatal("malformed workspace key must be rejected")
	}
}
