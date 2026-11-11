package mcpgateway

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"strings"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/cloudmcp"
	"github.com/0xmarkhydra/codelocal/internal/plugins"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

const planeCloudMCPAlias = "cloud:plane"
const maxCloudPluginApprovalPreview = 6000

var planeCloudWorkspaceRE = regexp.MustCompile(`^[a-z0-9][a-z0-9_-]{0,63}$`)

// Cloud plugins are account-scoped, never a local device MCP configuration.
// No PAT, refresh token or credential reference is returned to the AI client.
func (s *Service) planeCloudMCPConnection(ctx context.Context, user string) (cloud.PluginConnection, error) {
	if s.Store == nil || s.CloudMCP == nil || user == "" {
		return cloud.PluginConnection{}, errors.New("Cloud MCP is unavailable")
	}
	c, found, err := s.Store.PluginConnectionByDevice(ctx, user, "plane", cloud.PluginCloudDeviceID)
	if err != nil {
		return cloud.PluginConnection{}, errors.New("Plane connection could not be loaded")
	}
	if !found || c.State != cloud.PluginConnectionReady {
		return cloud.PluginConnection{}, errors.New("Connect Plane in CodeLocal Plugins first")
	}
	if !plugins.IsPlaneMCPPATEndpoint(c.Endpoint) {
		return cloud.PluginConnection{}, errors.New("Plane MCP endpoint is not trusted")
	}
	return c, nil
}

func (s *Service) planeCloudMCPConfig(ctx context.Context, user string, c cloud.PluginConnection) (cloudmcp.Config, error) {
	if c.UserID != user || c.PluginID != "plane" || c.DeviceID != cloud.PluginCloudDeviceID ||
		c.State != cloud.PluginConnectionReady || !plugins.IsPlaneMCPPATEndpoint(c.Endpoint) {
		return cloudmcp.Config{}, errors.New("Plane connection is not authorized")
	}
	cred, err := s.Store.PluginCloudCredential(ctx, user, "plane")
	if err != nil {
		return cloudmcp.Config{}, errors.New("Plane credentials are unavailable; reconnect in CodeLocal Plugins")
	}
	if cred.Kind != "bearer" || cred.Endpoint != c.Endpoint || cred.Token == "" ||
		!planeCloudWorkspaceRE.MatchString(cred.WorkspaceSlug) {
		return cloudmcp.Config{}, errors.New("Plane credentials or workspace are invalid; reconnect in CodeLocal Plugins")
	}
	return cloudmcp.Config{Endpoint: c.Endpoint, Bearer: cred.Token, WorkspaceSlug: cred.WorkspaceSlug}, nil
}

// Lists only the user's already-connected Cloud Plane entry. The device's
// locally installed MCP extension list remains unchanged.
func (s *Service) planeCloudMCPServers(ctx context.Context, user string) []map[string]any {
	c, err := s.planeCloudMCPConnection(ctx, user)
	if err != nil {
		return nil
	}
	return []map[string]any{{
		"name": planeCloudMCPAlias, "source": "cloud-plugin", "connected": true,
		"state": c.State, "toolsCached": c.ToolCount, "endpoint": c.Endpoint,
		"note": "Use mcp(action=search,server=cloud:plane). Connected owners can read and manage routine Plane tasks directly; destructive and privileged operations still require confirmation.",
	}}
}

func cloudPluginMCPToolMatch(tool cloudmcp.ToolSummary, query string) bool {
	query = strings.ToLower(strings.TrimSpace(query))
	if query == "" {
		return true
	}
	return strings.Contains(strings.ToLower(tool.Name+" "+tool.Description), query)
}

func (s *Service) callPlaneCloudMCP(ctx context.Context, user string, args map[string]any, req *mcp.CallToolRequest) (*mcp.CallToolResult, error) {
	if server, _ := args["server"].(string); server != planeCloudMCPAlias {
		return errorResult(errors.New("unknown Cloud MCP server")), nil
	}
	action, _ := args["action"].(string)
	if action != "search" && action != "info" && action != "call" {
		return errorResult(errors.New("Cloud Plane MCP supports search, info and call")), nil
	}
	c, err := s.planeCloudMCPConnection(ctx, user)
	if err != nil {
		return errorResult(err), nil
	}
	cfg, err := s.planeCloudMCPConfig(ctx, user, c)
	if err != nil {
		return errorResult(err), nil
	}
	tools, err := s.CloudMCP.Discover(ctx, user, cfg)
	if err != nil {
		return errorResult(err), nil
	}
	// This is a bounded CodeLocal workflow, not an extra native Plane tool.
	tools = append(tools, cloudmcp.PlaneSprintBatchTool)
	switch action {
	case "search":
		query, _ := args["query"].(string)
		limit := 10
		if n, ok := args["limit"].(float64); ok && n >= 1 && n <= 50 {
			limit = int(n)
		}
		results := make([]map[string]any, 0, limit)
		for _, tool := range tools {
			if cloudPluginMCPToolMatch(tool, query) {
				results = append(results, map[string]any{"server": planeCloudMCPAlias, "tool": tool.Name, "description": tool.Description, "inputSchema": json.RawMessage(tool.InputSchema), "readOnlyHint": tool.ReadOnly})
				if len(results) == limit {
					break
				}
			}
		}
		return textResult(map[string]any{"server": planeCloudMCPAlias, "connection": "ready", "nativeTools": len(tools) - 1, "bridgeWorkflows": 1, "results": results, "endpoint": c.Endpoint}, false), nil
	case "info":
		name, _ := args["tool"].(string)
		for _, tool := range tools {
			if tool.Name == name {
				return textResult(map[string]any{"server": planeCloudMCPAlias, "tool": tool, "endpoint": c.Endpoint}, false), nil
			}
		}
		return errorResult(errors.New("Plane tool was not found")), nil
	default:
		name, _ := args["tool"].(string)
		arguments, _ := args["arguments"].(map[string]any)
		if planeAutoActionAllowed(name, arguments) {
			return s.callAutoPlaneCloudMCP(ctx, user, c, cfg, tools, args)
		}
		return s.callApprovedPlaneCloudMCP(ctx, user, c, cfg, tools, args, req)
	}
}

// Privileged Plane Cloud operations never inherit authorization from MCP
// annotations, a local device access mode, or an AI-authored token. The safe
// account-scoped allowlist is handled separately; other actions still need
// a real owner confirmation for the exact tool and arguments.
func (s *Service) callApprovedPlaneCloudMCP(ctx context.Context, user string, c cloud.PluginConnection, cfg cloudmcp.Config, tools []cloudmcp.ToolSummary, args map[string]any, req *mcp.CallToolRequest) (*mcp.CallToolResult, error) {
	name, _ := args["tool"].(string)
	if name == "" || len(name) > 128 {
		return errorResult(errors.New("Plane tool name is required")), nil
	}
	found := false
	for _, tool := range tools {
		if tool.Name == name {
			found = true
			break
		}
	}
	if !found {
		return errorResult(errors.New("Plane tool is not available")), nil
	}
	arguments, ok := args["arguments"].(map[string]any)
	if !ok {
		return errorResult(errors.New("Plane tool arguments must be an object")), nil
	}
	payload, err := json.Marshal(arguments)
	maxPreview := maxCloudPluginApprovalPreview
	if name == cloudmcp.PlaneSprintBatchToolName {
		if _, validateErr := cloudmcp.ValidatePlaneSprintBatch(arguments); validateErr != nil {
			return errorResult(validateErr), nil
		}
		maxPreview = 60 << 10
	}
	if err != nil || len(payload) > maxPreview {
		return errorResult(fmt.Errorf("Plane tool arguments must fit within %d bytes for explicit review", maxPreview)), nil
	}
	confirmed := false
	if name != cloudmcp.PlaneSprintBatchToolName && req != nil && req.Session != nil {
		response, elicitErr := req.Session.Elicit(ctx, &mcp.ElicitParams{
			Mode:    "form",
			Message: "Approve one Plane MCP operation on your CodeLocal account?\nTool: " + name + "\nEndpoint: " + c.Endpoint + "\nArguments: " + string(payload),
			RequestedSchema: map[string]any{
				"type": "object", "properties": map[string]any{
					"approve": map[string]any{"type": "boolean", "title": "I authorize this exact operation"},
				}, "required": []string{"approve"}, "additionalProperties": false,
			},
		})
		if elicitErr == nil && response != nil {
			if response.Action != "accept" || response.Content["approve"] != true {
				return textResult(map[string]any{"status": "not_approved", "executionStarted": false}, true), nil
			}
			confirmed = true
		}
	}
	if s.Store == nil {
		return textResult(map[string]any{"status": "blocked", "executionStarted": false, "message": "Cloud approval store unavailable"}, true), nil
	}
	approval, err := s.Store.CreatePluginApproval(ctx, cloud.PluginApproval{
		UserID: user, SessionID: "mcp:" + sessionID(req), PluginID: "plane",
		DeviceID: cloud.PluginCloudDeviceID, Version: c.UpdatedAt, Tool: name, Arguments: arguments,
	})
	if err != nil {
		return errorResult(errors.New("Plane approval store is unavailable")), nil
	}
	if !confirmed {
		return textResult(map[string]any{
			"status": "approval_required", "executionStarted": false, "tool": name,
			"approvalId": approval.ID, "expiresAt": approval.ExpiresAt,
			"approvalUrl": "https://codelocal.cloud/dashboard/plugins/approve/" + approval.ID,
			"message":     "Open the CodeLocal approval link, inspect the exact tool arguments, and approve or deny. No Plane operation has been executed. Check the result before retrying.",
		}, false), nil
	}
	approved, err := s.Store.TakePluginApproval(ctx, user, approval.SessionID, approval.ID, false)
	if err != nil {
		return errorResult(errors.New("Plane approval expired or was already used")), nil
	}
	var output map[string]any
	err = s.Store.WithPluginCloudConnection(ctx, user, "plane", approved.Version, func(locked cloud.PluginConnection) error {
		if locked.Endpoint != cfg.Endpoint {
			return errors.New("Plane endpoint changed during approval")
		}
		fresh, err := s.planeCloudMCPConfig(ctx, user, locked)
		if err != nil {
			return err
		}
		if approved.Tool == cloudmcp.PlaneSprintBatchToolName {
			output, err = s.CloudMCP.SetupPlaneSprint(ctx, user, fresh, approved.Arguments)
		} else {
			output, err = s.CloudMCP.CallApproved(ctx, user, fresh, approved.Tool, approved.Arguments)
		}
		return err
	})
	status := "done"
	if err != nil || output["isError"] == true {
		status = "error"
	}
	if completed := s.Store.CompletePluginApproval(ctx, user, approved.ID, status); completed != nil {
		return textResult(map[string]any{"status": "unknown", "message": "Plane approval tracking failed; check Plane before retrying.", "executionStarted": true}, true), nil
	}
	s.Store.Audit(cloud.AuditEvent{UserID: user, Event: "plugin.mcp_tool_call", Detail: map[string]any{
		"pluginId": "plane", "tool": name, "target": "cloud", "status": status,
	}})
	if err != nil {
		return textResult(map[string]any{"status": "unknown", "message": "Plane operation failed or was interrupted; check Plane before retrying.", "executionStarted": true}, true), nil
	}
	return textResult(map[string]any{"status": status, "server": planeCloudMCPAlias, "result": output}, status != "done"), nil
}
