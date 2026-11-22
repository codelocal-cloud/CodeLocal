package mcpgateway

import (
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/gateway"
)

func defaultWorkspaceFixture(key, workspaceID, status string, lastSeenAt int64) gateway.WorkspaceView {
	return gateway.WorkspaceView{Key: key, WorkspaceID: workspaceID, WorkspaceName: workspaceID, Status: status, Authorized: true, LastSeenAt: lastSeenAt}
}

func TestSelectDefaultWorkspaceKeyHonorsConfiguredIDOrKey(t *testing.T) {
	catalog := []gateway.WorkspaceView{
		defaultWorkspaceFixture("u::d::alpha", "alpha", "active", 100),
		defaultWorkspaceFixture("u::d::beta", "beta", "active", 500),
	}
	if got := selectDefaultWorkspaceKey(catalog, "alpha"); got != "u::d::alpha" {
		t.Fatalf("workspace-id default resolved to %q", got)
	}
	if got := selectDefaultWorkspaceKey(catalog, "u::d::beta"); got != "u::d::beta" {
		t.Fatalf("full-key default resolved to %q", got)
	}
}

func TestSelectDefaultWorkspaceKeyDoesNotGuessBetweenMultipleActiveWorkspaces(t *testing.T) {
	catalog := []gateway.WorkspaceView{
		defaultWorkspaceFixture("u::d::alpha", "alpha", "active", 100),
		defaultWorkspaceFixture("u::d::beta", "beta", "active", 900),
	}
	if got := selectDefaultWorkspaceKey(catalog, ""); got != "" {
		t.Fatalf("key=%q; multiple active projects must require user selection", got)
	}
}

func TestSelectDefaultWorkspaceKeyAutoUsesSingleActiveWorkspace(t *testing.T) {
	catalog := []gateway.WorkspaceView{
		defaultWorkspaceFixture("u::d::awake", "awake", "active", 100),
		defaultWorkspaceFixture("u::d::old", "old", "sleeping", 900),
	}
	if got := selectDefaultWorkspaceKey(catalog, ""); got != "u::d::awake" {
		t.Fatalf("key=%q want sole active workspace", got)
	}
}

func TestSelectDefaultWorkspaceKeyAutoWakesSingleSleepingWorkspace(t *testing.T) {
	catalog := []gateway.WorkspaceView{defaultWorkspaceFixture("u::d::sleepy", "sleepy", "sleeping", 100)}
	if got := selectDefaultWorkspaceKey(catalog, ""); got != "u::d::sleepy" {
		t.Fatalf("key=%q want sole sleeping workspace", got)
	}
}

func TestSelectDefaultWorkspaceKeyDoesNotGuessBetweenSleepingWorkspaces(t *testing.T) {
	catalog := []gateway.WorkspaceView{
		defaultWorkspaceFixture("u::d::old", "old", "sleeping", 100),
		defaultWorkspaceFixture("u::d::recent", "recent", "sleeping", 900),
	}
	if got := selectDefaultWorkspaceKey(catalog, ""); got != "" {
		t.Fatalf("key=%q; LastSeenAt must not choose between projects", got)
	}
}

func TestSelectDefaultWorkspaceKeyIgnoresUnauthorizedOrOfflineDefault(t *testing.T) {
	unauthorized := defaultWorkspaceFixture("u::d::gone", "gone", "sleeping", 9999)
	unauthorized.Authorized = false
	catalog := []gateway.WorkspaceView{unauthorized}
	if got := selectDefaultWorkspaceKey(catalog, "gone"); got != "" {
		t.Fatalf("unauthorized default resolved to %q", got)
	}
	catalog = []gateway.WorkspaceView{defaultWorkspaceFixture("u::d::gone", "gone", "device_offline", 9999)}
	if got := selectDefaultWorkspaceKey(catalog, "gone"); got != "" {
		t.Fatalf("offline default resolved to %q", got)
	}
}

func TestSelectDefaultWorkspaceKeySkipsManagedSystemProjectsImplicitly(t *testing.T) {
	system := defaultWorkspaceFixture("u::d::system-openmontage", "system-openmontage", "active", 5000)
	system.System = true
	catalog := []gateway.WorkspaceView{system, defaultWorkspaceFixture("u::d::codex", "codex", "active", 100)}
	if got := selectDefaultWorkspaceKey(catalog, ""); got != "u::d::codex" {
		t.Fatalf("key=%q want operator project", got)
	}
	if got := selectDefaultWorkspaceKey(catalog, "system-openmontage"); got != "u::d::system-openmontage" {
		t.Fatalf("explicit configured system workspace resolved to %q", got)
	}
}

func TestWorkspaceSelectionRequiredResultCarriesCandidates(t *testing.T) {
	catalog := []gateway.WorkspaceView{
		defaultWorkspaceFixture("u::d::alpha", "alpha", "active", 100),
		defaultWorkspaceFixture("u::d::beta", "beta", "active", 200),
	}
	result := workspaceSelectionRequiredResult(catalog)
	structured, ok := result.StructuredContent.(map[string]any)
	if !ok || structured["selectionRequired"] != true || structured["canSetDefault"] != true {
		t.Fatalf("unexpected selection result: %#v", result.StructuredContent)
	}
	workspaces, ok := structured["workspaces"].([]any)
	if !ok || len(workspaces) != 2 {
		t.Fatalf("selection candidates=%#v", structured["workspaces"])
	}
	for _, raw := range workspaces {
		if _, ok := raw.(map[string]any); !ok {
			t.Fatalf("selection candidate has unexpected shape: %#v", raw)
		}
	}
}
