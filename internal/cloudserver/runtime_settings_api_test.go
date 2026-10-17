package cloudserver

import (
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/taskexecution"
)

func TestParseWorktreeLimitMutation(t *testing.T) {
	tests := []struct {
		name      string
		scope     cloud.RuntimeScope
		deviceID  string
		workspace string
		key       string
		action    string
		value     string
		want      int
		wantError string
	}{
		{name: "unrelated key", key: "FFMPEG_PATH", want: 3},
		{name: "workspace required", scope: cloud.RuntimeScopeGlobal, key: taskexecution.RuntimeSettingMaxWorktrees, value: "3", want: 3, wantError: "workspace_scope_required"},
		{name: "minimum", scope: cloud.RuntimeScopeWorkspace, deviceID: "device", workspace: "workspace", key: taskexecution.RuntimeSettingMaxWorktrees, value: "1", want: 1},
		{name: "maximum", scope: cloud.RuntimeScopeWorkspace, deviceID: "device", workspace: "workspace", key: taskexecution.RuntimeSettingMaxWorktrees, value: "20", want: 20},
		{name: "out of range", scope: cloud.RuntimeScopeWorkspace, deviceID: "device", workspace: "workspace", key: taskexecution.RuntimeSettingMaxWorktrees, value: "21", want: 3, wantError: "invalid_worktree_limit"},
		{name: "delete restores default", scope: cloud.RuntimeScopeWorkspace, deviceID: "device", workspace: "workspace", key: taskexecution.RuntimeSettingMaxWorktrees, action: "delete", want: 3},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, gotError := parseWorktreeLimitMutation(test.scope, test.deviceID, test.workspace, test.key, test.action, test.value)
			if got != test.want || gotError != test.wantError {
				t.Fatalf("got limit=%d error=%q want limit=%d error=%q", got, gotError, test.want, test.wantError)
			}
		})
	}
}
