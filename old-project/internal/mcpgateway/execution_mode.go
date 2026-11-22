package mcpgateway

import (
	"context"
	"errors"
	"strings"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/gateway"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func (s *Service) callExecutionMode(ctx context.Context, userID, session string, args map[string]any) (*mcp.CallToolResult, error) {
	if s == nil || s.Store == nil || s.Workspaces == nil {
		return errorResult(errors.New("execution mode service unavailable")), nil
	}
	key, _ := args["workspaceKey"].(string)
	key = strings.TrimSpace(key)
	if key == "" {
		key = strings.TrimSpace(s.route(userID, session))
	}
	if key == "" {
		key = strings.TrimSpace(s.defaultWorkspaceKey(ctx, userID))
	}
	if key == "" {
		return gatewayFailureResult(errors.New("no workspace selected"), "", "", "", 0, false), nil
	}
	catalog, err := s.Workspaces.Catalog(ctx, userID)
	if err != nil {
		return errorResult(err), nil
	}
	var workspace *gateway.WorkspaceView
	for index := range catalog {
		if catalog[index].Key == key {
			workspace = &catalog[index]
			break
		}
	}
	if workspace == nil {
		return gatewayFailureResult(errors.New("workspace not found"), "", "", key, 0, false), nil
	}
	layer, err := s.Store.RuntimeSettingsLayer(ctx, userID, cloud.RuntimeScopeWorkspace, workspace.DeviceID, workspace.WorkspaceID)
	if err != nil {
		return errorResult(err), nil
	}
	requested, _ := args["executionMode"].(string)
	requested = strings.TrimSpace(requested)
	runtimeApplied := false
	if requested != "" {
		mode := cloud.RuntimeExecutionMode(requested)
		if !cloud.ValidRuntimeExecutionMode(mode) {
			return errorResult(errors.New("executionMode must be one of: safe, live")), nil
		}
		layer.ExecutionMode = mode
		layer.ExecutionModeConfigured = true
		if err := s.Store.PutRuntimeSettingsLayer(ctx, userID, layer, workspace.DeviceID, workspace.WorkspaceID); err != nil {
			return errorResult(err), nil
		}
		s.Store.Audit(cloud.AuditEvent{UserID: userID, Event: "runtime.execution_mode_updated", DeviceID: workspace.DeviceID, WorkspaceID: workspace.WorkspaceID, Detail: map[string]any{"mode": mode, "source": "mcp"}})
		if workspace.RuntimeOnline && s.Hub != nil {
			if active, activateErr := s.Workspaces.Activate(ctx, userID, key); activateErr == nil {
				requestID := cloud.RandomHex(16)
				routed, callErr := s.Hub.Call(ctx, userID, active.Key, session, "execution_mode", map[string]any{"mode": string(mode)}, false, requestID)
				runtimeApplied = callErr == nil && routed.OK
			}
		}
	}
	mode := cloud.NormalizeRuntimeExecutionMode(layer.ExecutionMode)
	return textResult(map[string]any{
		"mode": mode, "configured": layer.ExecutionModeConfigured, "scope": "workspace",
		"workspaceKey": key, "deviceId": workspace.DeviceID, "workspaceId": workspace.WorkspaceID,
		"runtimeApplied": runtimeApplied,
		"choices": []map[string]any{
			{"mode": "safe", "label": "Safe Workspace", "description": "Work in an isolated checkout. Recommended/default."},
			{"mode": "live", "label": "Live Project", "description": "Edit the active project checkout directly."},
		},
	}, false), nil
}

func (s *Service) executionModeSelectionRequired(ctx context.Context, userID, workspaceKey string) (*mcp.CallToolResult, bool) {
	if s == nil || s.Store == nil || s.Workspaces == nil || strings.TrimSpace(workspaceKey) == "" {
		return nil, false
	}
	catalog, err := s.Workspaces.Catalog(ctx, userID)
	if err != nil {
		return nil, false
	}
	for _, workspace := range catalog {
		if workspace.Key != workspaceKey {
			continue
		}
		layer, err := s.Store.RuntimeSettingsLayer(ctx, userID, cloud.RuntimeScopeWorkspace, workspace.DeviceID, workspace.WorkspaceID)
		if err != nil || layer.ExecutionModeConfigured {
			return nil, false
		}
		return textResult(map[string]any{
			"code":             "CODELOCAL_EXECUTION_MODE_SELECTION_REQUIRED",
			"status":           "selection_required",
			"executionStarted": false,
			"workspaceKey":     workspace.Key,
			"workspaceName":    workspace.WorkspaceName,
			"defaultMode":      "safe",
			"message":          "Ask the user once to choose Safe Workspace or Live Project before the first coding mutation in this workspace.",
			"choices": []map[string]any{
				{"mode": "safe", "label": "Safe Workspace", "description": "Work in an isolated checkout. Recommended/default."},
				{"mode": "live", "label": "Live Project", "description": "Edit the active project checkout directly."},
			},
			"nextAction": "Call workspace(action=execution, executionMode=safe|live, workspaceKey=...) after the user chooses.",
		}, true), true
	}
	return nil, false
}
