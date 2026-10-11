package cloudserver

import (
	"encoding/json"
	"strings"

	"github.com/0xmarkhydra/codelocal/internal/gateway"
)

// Preserve JSON validity on large tool responses. Slicing a serialized JSON
// object and appending a field can produce invalid JSON and mislead the model.
func dashboardCompactToolJSON(encoded []byte) string {
	const maxToolResultBytes = 5000
	if len(encoded) <= maxToolResultBytes {
		return string(encoded)
	}
	preview := strings.ToValidUTF8(string(encoded[:maxToolResultBytes/2]), "")
	for {
		result, _ := json.Marshal(map[string]any{
			"truncated":     true,
			"originalBytes": len(encoded),
			"preview":       preview,
			"message":       "The result was truncated. Request a narrower query for complete data.",
		})
		if len(result) <= maxToolResultBytes || preview == "" {
			return string(result)
		}
		preview = strings.ToValidUTF8(preview[:len(preview)*3/4], "")
	}
}

// A tool reading workspace detail must respect the thread's already-bound
// project. Unlike free-form discovery, project identity cannot use fuzzy match.
func dashboardChatMatchesBoundWorkspace(workspace gateway.WorkspaceView, requested string) bool {
	requested = strings.TrimSpace(requested)
	if requested == "" {
		return true
	}
	return strings.EqualFold(requested, strings.TrimSpace(workspace.WorkspaceID)) ||
		strings.EqualFold(requested, strings.TrimSpace(workspace.WorkspaceName)) ||
		strings.EqualFold(requested, strings.TrimSpace(workspace.DeviceID)+"::"+strings.TrimSpace(workspace.WorkspaceID))
}
