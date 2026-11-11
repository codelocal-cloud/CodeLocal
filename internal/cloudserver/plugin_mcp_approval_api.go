package cloudserver

import (
	"net/http"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/webutil"
)

// MCP approvals originate from ChatGPT OAuth, but the approval itself must be
// performed by the same human account in an authenticated, CSRF-protected
// CodeLocal browser session. A caller cannot approve using an MCP token.
func (s *Server) pluginMCPApprovalDetailsAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.authenticatedAPIIdentity(w, r)
	if !ok {
		return
	}
	a, err := s.Store.PluginMCPApproval(r.Context(), identity.User.ID, r.PathValue("approvalID"))
	if err != nil {
		webutil.JSON(w, http.StatusNotFound, map[string]string{"error": "plugin_approval_unavailable"})
		return
	}
	connection, found, err := s.Store.PluginConnectionByDevice(r.Context(), identity.User.ID, "plane", cloud.PluginCloudDeviceID)
	if err != nil || !found || connection.UpdatedAt != a.Version || connection.State != cloud.PluginConnectionReady {
		webutil.JSON(w, http.StatusConflict, map[string]string{"error": "plugin_connection_changed"})
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	webutil.JSON(w, http.StatusOK, map[string]any{
		"id": a.ID, "plugin": a.PluginID, "tool": a.Tool,
		"arguments": a.Arguments, "expiresAt": a.ExpiresAt, "endpoint": connection.Endpoint,
	})
}

func (s *Server) pluginMCPApprovalConfirmAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.pluginMutationIdentity(w, r, true)
	if !ok {
		return
	}
	var input struct {
		Approve bool `json:"approve"`
	}
	if webutil.DecodeJSON(r, 1024, &input) != nil {
		webutil.JSON(w, http.StatusBadRequest, map[string]string{"error": "invalid_approval"})
		return
	}
	// Never accept the originating session, plugin name, tool or argument
	// from the browser; read the encrypted owner-bound pending record.
	a, err := s.Store.PluginMCPApproval(r.Context(), identity.User.ID, r.PathValue("approvalID"))
	if err != nil {
		webutil.JSON(w, http.StatusConflict, map[string]string{"error": "plugin_approval_unavailable"})
		return
	}
	approved, err := s.Store.TakePluginApproval(r.Context(), identity.User.ID, a.SessionID, a.ID, !input.Approve)
	if err != nil {
		webutil.JSON(w, http.StatusConflict, map[string]string{"error": "plugin_approval_unavailable"})
		return
	}
	if !input.Approve {
		s.Store.Audit(cloud.AuditEvent{UserID: identity.User.ID, Event: "plugin.mcp_tool_denied", Detail: map[string]any{"pluginId": approved.PluginID, "tool": approved.Tool}})
		webutil.JSON(w, http.StatusOK, map[string]string{"status": "denied"})
		return
	}
	result, callErr := s.executeApprovedPlugin(r.Context(), identity, approved)
	status := "done"
	if callErr != nil || result["isError"] == true {
		status = "error"
	}
	if err := s.Store.CompletePluginApproval(r.Context(), identity.User.ID, approved.ID, status); err != nil {
		webutil.JSON(w, http.StatusInternalServerError, map[string]any{"status": "unknown", "message": "Operation outcome unknown; inspect Plane before retrying"})
		return
	}
	s.Store.Audit(cloud.AuditEvent{UserID: identity.User.ID, Event: "plugin.mcp_tool_call", Detail: map[string]any{
		"pluginId": approved.PluginID, "tool": approved.Tool, "target": "cloud", "status": status,
	}})
	w.Header().Set("Cache-Control", "no-store")
	if callErr != nil {
		webutil.JSON(w, http.StatusOK, map[string]any{"status": "unknown", "message": "Plane call failed or timed out. Check the project before retrying."})
		return
	}
	webutil.JSON(w, http.StatusOK, map[string]any{"status": status, "result": result})
}
