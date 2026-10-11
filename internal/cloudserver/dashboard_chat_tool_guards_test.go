package cloudserver

import (
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/gateway"
)

func TestDashboardCompactToolJSONKeepsValidUTF8AndJSON(t *testing.T) {
	original := []byte(`{"devices":` + strings.Repeat(`"thiết bị máy tính",`, 900) + `"end"}`)
	result := dashboardCompactToolJSON(original)
	if !json.Valid([]byte(result)) {
		t.Fatal("tool result must remain valid JSON after truncation")
	}
	var parsed struct {
		Truncated     bool   `json:"truncated"`
		Preview       string `json:"preview"`
		OriginalBytes int    `json:"originalBytes"`
	}
	if err := json.Unmarshal([]byte(result), &parsed); err != nil || !parsed.Truncated || parsed.OriginalBytes != len(original) || len(result) > 5000 {
		t.Fatalf("bad compacted result: %+v, %v", parsed, err)
	}
	if strings.Contains(parsed.Preview, "\ufffd") {
		t.Fatal("preview must not split a UTF-8 rune")
	}
	if got := dashboardCompactToolJSON([]byte(`{"ok":true}`)); got != `{"ok":true}` {
		t.Fatalf("small JSON must remain unchanged: %s", got)
	}
}

func TestDashboardWorkspaceDetailCannotSwitchBoundTask(t *testing.T) {
	bound := &gateway.WorkspaceView{DeviceID: "mac-1", WorkspaceID: "project-1", WorkspaceName: "Finance"}
	request := dashboardWithExecutionState(httptest.NewRequest("POST", "/chat", nil), "user-1", "req-1", bound)
	for _, candidate := range []string{"", "project-1", "Finance", "mac-1::project-1"} {
		raw := execDashboardTool(request, &Server{}, "user-1", "get_workspace_detail", map[string]any{"workspace": candidate})
		if !json.Valid([]byte(raw)) || !strings.Contains(raw, `"ready":true`) {
			t.Fatalf("same workspace %q must return bound project: %s", candidate, raw)
		}
	}
	for _, candidate := range []string{"project-2", "Finance Other", "mac-2::project-1"} {
		raw := execDashboardTool(request, &Server{}, "user-1", "get_workspace_detail", map[string]any{"workspace": candidate})
		if !strings.Contains(raw, "workspace_locked_to_task") {
			t.Fatalf("cross-project detail %q not rejected: %s", candidate, raw)
		}
	}
	if state := dashboardExecutionStateFromRequest(request); state.workspace != bound {
		t.Fatal("workspace must stay unchanged after rejected tool calls")
	}
}

func TestDashboardBrainSearchDoesNotReturnFakeHits(t *testing.T) {
	request := httptest.NewRequest("POST", "/chat", nil)
	raw := execDashboardTool(request, &Server{}, "user-1", "search_project_brain", map[string]any{"query": "finance"})
	var result struct {
		Error string `json:"error"`
		Hits  []any  `json:"hits"`
	}
	if err := json.Unmarshal([]byte(raw), &result); err != nil || result.Error != "project_brain_search_unavailable" || len(result.Hits) != 0 {
		t.Fatalf("unavailable search must be honest: %s, %v", raw, err)
	}
}
