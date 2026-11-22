package cloudserver

import (
	"context"
	"net/http"
	"strconv"
	"strings"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/gateway"
	"github.com/0xmarkhydra/codelocal/internal/taskexecution"
	"github.com/0xmarkhydra/codelocal/internal/webutil"
)

func parseWorktreeLimitMutation(scope cloud.RuntimeScope, deviceID, workspaceID, key, action, value string) (int, string) {
	limit := taskexecution.DefaultMaxWorktrees
	if key != taskexecution.RuntimeSettingMaxWorktrees {
		return limit, ""
	}
	if scope != cloud.RuntimeScopeWorkspace || deviceID == "" || workspaceID == "" {
		return limit, "workspace_scope_required"
	}
	if action == "delete" {
		return limit, ""
	}
	parsed, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil || parsed < taskexecution.MinMaxWorktrees || parsed > taskexecution.MaxMaxWorktrees {
		return limit, "invalid_worktree_limit"
	}
	return parsed, ""
}

func (s *Server) applyRuntimeWorkspaceOperation(ctx context.Context, userID, deviceID, workspaceID, operation string, args map[string]any) bool {
	if s.Workspaces == nil || s.Hub == nil {
		return false
	}
	workspaceKey := gateway.ClientKey(userID, deviceID, workspaceID)
	catalog, err := s.Workspaces.Catalog(ctx, userID)
	if err != nil {
		return false
	}
	for _, workspace := range catalog {
		if workspace.Key != workspaceKey || workspace.Status != "active" {
			continue
		}
		result, callErr := s.Hub.Call(ctx, userID, workspaceKey, "dashboard-settings", operation, args, false, cloud.RandomHex(16))
		return callErr == nil && result.OK
	}
	return false
}

func runtimeScopeFromRequest(r *http.Request) cloud.RuntimeScope {
	scope := strings.TrimSpace(r.URL.Query().Get("scope"))
	if scope == "" {
		scope = strings.TrimSpace(r.FormValue("scope"))
	}
	if scope == "" {
		scope = string(cloud.RuntimeScopeGlobal)
	}
	return cloud.RuntimeScope(scope)
}

func runtimeTargetFromRequest(r *http.Request) (cloud.RuntimeScope, string, string) {
	return runtimeScopeFromRequest(r), strings.TrimSpace(first(r.URL.Query().Get("deviceId"), r.FormValue("deviceId"))), strings.TrimSpace(first(r.URL.Query().Get("workspaceId"), r.FormValue("workspaceId")))
}

func (s *Server) runtimeSettingsResourceAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.identity(r)
	if !ok {
		webutil.JSON(w, http.StatusUnauthorized, map[string]any{"error": "unauthorized"})
		return
	}
	scope, deviceID, workspaceID := runtimeTargetFromRequest(r)
	layer, err := s.Store.RuntimeSettingsLayer(r.Context(), identity.User.ID, scope, deviceID, workspaceID)
	if err != nil {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "invalid_runtime_scope"})
		return
	}
	effective, err := s.Store.ResolveRuntimeTarget(r.Context(), identity.User.ID, scope, deviceID, workspaceID)
	if err != nil {
		webutil.JSON(w, http.StatusInternalServerError, map[string]any{"error": "runtime_settings_unavailable"})
		return
	}
	webutil.JSON(w, http.StatusOK, map[string]any{"scope": scope, "deviceId": deviceID, "workspaceId": workspaceID, "layer": layer, "effective": effective})
}

func (s *Server) runtimeConfigMutationAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.identity(r)
	if !ok {
		webutil.JSON(w, http.StatusUnauthorized, map[string]any{"error": "unauthorized"})
		return
	}
	if !s.WebAuth.VerifySessionCSRF(r, identity) {
		webutil.JSON(w, http.StatusForbidden, map[string]any{"error": "invalid_csrf"})
		return
	}
	scope, deviceID, workspaceID := runtimeTargetFromRequest(r)
	layer, err := s.Store.RuntimeSettingsLayer(r.Context(), identity.User.ID, scope, deviceID, workspaceID)
	if err != nil {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "invalid_runtime_scope"})
		return
	}
	if layer.Values == nil {
		layer.Values = map[string]string{}
	}
	key := strings.TrimSpace(r.FormValue("key"))
	if !cloud.ValidRuntimeEnvKey(key) {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "invalid_runtime_key"})
		return
	}
	action := r.FormValue("action")
	worktreeLimit, limitError := parseWorktreeLimitMutation(scope, deviceID, workspaceID, key, action, r.FormValue("value"))
	if limitError != "" {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": limitError})
		return
	}
	if action == "delete" {
		delete(layer.Values, key)
	} else {
		layer.Values[key] = r.FormValue("value")
	}
	if err := s.Store.PutRuntimeSettingsLayer(r.Context(), identity.User.ID, layer, deviceID, workspaceID); err != nil {
		webutil.JSON(w, http.StatusInternalServerError, map[string]any{"error": "runtime_config_update_failed"})
		return
	}
	s.Store.Audit(cloud.AuditEvent{UserID: identity.User.ID, Event: "runtime.config_updated", DeviceID: deviceID, WorkspaceID: workspaceID, Detail: map[string]any{"scope": scope, "key": key, "action": first(r.FormValue("action"), "set")}})
	runtimeApplied := false
	if key == taskexecution.RuntimeSettingMaxWorktrees {
		runtimeApplied = s.applyRuntimeWorkspaceOperation(r.Context(), identity.User.ID, deviceID, workspaceID, "worktree_limit", map[string]any{"limit": worktreeLimit})
	}
	webutil.JSON(w, http.StatusOK, map[string]any{"ok": true, "runtimeApplied": runtimeApplied})
}

func (s *Server) runtimeExecutionModeMutationAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.identity(r)
	if !ok {
		webutil.JSON(w, http.StatusUnauthorized, map[string]any{"error": "unauthorized"})
		return
	}
	if !s.WebAuth.VerifySessionCSRF(r, identity) {
		webutil.JSON(w, http.StatusForbidden, map[string]any{"error": "invalid_csrf"})
		return
	}
	scope, deviceID, workspaceID := runtimeTargetFromRequest(r)
	if scope != cloud.RuntimeScopeWorkspace || deviceID == "" || workspaceID == "" {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "workspace_scope_required"})
		return
	}
	mode := cloud.RuntimeExecutionMode(strings.TrimSpace(r.FormValue("mode")))
	if !cloud.ValidRuntimeExecutionMode(mode) {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "invalid_execution_mode"})
		return
	}
	layer, err := s.Store.RuntimeSettingsLayer(r.Context(), identity.User.ID, scope, deviceID, workspaceID)
	if err != nil {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "invalid_runtime_scope"})
		return
	}
	layer.ExecutionMode = mode
	layer.ExecutionModeConfigured = true
	if err := s.Store.PutRuntimeSettingsLayer(r.Context(), identity.User.ID, layer, deviceID, workspaceID); err != nil {
		webutil.JSON(w, http.StatusInternalServerError, map[string]any{"error": "execution_mode_update_failed"})
		return
	}
	s.Store.Audit(cloud.AuditEvent{UserID: identity.User.ID, Event: "runtime.execution_mode_updated", DeviceID: deviceID, WorkspaceID: workspaceID, Detail: map[string]any{"mode": mode, "source": "dashboard"}})
	runtimeApplied := s.applyRuntimeWorkspaceOperation(r.Context(), identity.User.ID, deviceID, workspaceID, "execution_mode", map[string]any{"mode": string(mode)})
	webutil.JSON(w, http.StatusOK, map[string]any{"ok": true, "mode": mode, "runtimeApplied": runtimeApplied})
}

func (s *Server) runtimeSecretMutationAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.identity(r)
	if !ok {
		webutil.JSON(w, http.StatusUnauthorized, map[string]any{"error": "unauthorized"})
		return
	}
	if !s.WebAuth.VerifySessionCSRF(r, identity) {
		webutil.JSON(w, http.StatusForbidden, map[string]any{"error": "invalid_csrf"})
		return
	}
	scope, deviceID, workspaceID := runtimeTargetFromRequest(r)
	name := strings.TrimSpace(r.FormValue("name"))
	if !cloud.ValidRuntimeEnvKey(name) {
		webutil.JSON(w, http.StatusBadRequest, map[string]any{"error": "invalid_runtime_key"})
		return
	}
	var err error
	if r.FormValue("action") == "delete" {
		err = s.Store.DeleteRuntimeSecret(r.Context(), identity.User.ID, scope, deviceID, workspaceID, name)
	} else {
		err = s.Store.PutRuntimeSecret(r.Context(), identity.User.ID, scope, deviceID, workspaceID, name, r.FormValue("value"))
	}
	if err != nil {
		webutil.JSON(w, http.StatusInternalServerError, map[string]any{"error": "runtime_secret_update_failed"})
		return
	}
	s.Store.Audit(cloud.AuditEvent{UserID: identity.User.ID, Event: "runtime.secret_updated", DeviceID: deviceID, WorkspaceID: workspaceID, Detail: map[string]any{"scope": scope, "key": name, "action": first(r.FormValue("action"), "set")}})
	webutil.JSON(w, http.StatusOK, map[string]any{"ok": true})
}
