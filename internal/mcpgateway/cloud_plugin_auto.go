package mcpgateway

import (
	"context"
	"errors"
	"regexp"
	"strings"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/cloudmcp"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

// Plane Cloud's already-connected PAT authorizes ordinary project operations
// made by the same authenticated CodeLocal account. Do NOT infer tool safety
// from untrusted MCP annotation hints or free-form descriptions. Use this
// explicit per-tool, per-action allowlist only. Destructive/access/security
// operations continue through the existing owner approval flow.
var planeAutoUUIDRE = regexp.MustCompile(`(?i)^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$`)
var planeAutoIDList = regexp.MustCompile(`(?i)^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}(,[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})*$`)

func planeAutoActionAllowed(tool string, arguments map[string]any) bool {
	if arguments == nil {
		return false
	}
	if tool == cloudmcp.SocCoMobileCleanupToolName {
		return len(arguments) == 0
	}
	if tool == cloudmcp.PlaneSprintBatchToolName {
		// Batch is explicitly scoped to project/cycle and uses unique
		// external IDs, provider read-before-create and no mutation retries.
		_, err := cloudmcp.ValidatePlaneSprintBatch(arguments)
		return err == nil
	}
	action, ok := arguments["action"].(string)
	if !ok || action == "" {
		return false
	}
	// Access to another project/workspace is still rejected by Plane PAT.
	switch tool {
	case "project":
		return action == "list" || action == "retrieve"
	case "cycle":
		switch action {
		case "list", "retrieve", "list_workitems":
			return true
		case "create":
			return validUUIDArg(arguments, "project_id") && nonEmptyStringArg(arguments, "name") && validUUIDArg(arguments, "owned_by")
		case "manage_workitems":
			added, _ := arguments["add_ids"].(string)
			removed, _ := arguments["remove_ids"].(string)
			return validUUIDArg(arguments, "project_id") && validUUIDArg(arguments, "cycle_id") &&
				removed == "" && len(added) <= 4096 && planeAutoIDList.MatchString(added)
		}
	case "workitem":
		switch action {
		case "list", "list_archived", "retrieve", "retrieve_by_identifier", "search", "count":
			return true
		case "create":
			return validUUIDArg(arguments, "project_id") && nonEmptyStringArg(arguments, "name")
		case "update":
			return validUUIDArg(arguments, "project_id") && validUUIDArg(arguments, "workitem_id") &&
				safePlaneWorkitemUpdate(arguments)
		}
	case "member":
		return action == "list" || action == "retrieve"
	case "label", "state", "module":
		return action == "list" || action == "retrieve" || (tool == "module" && action == "list_workitems")
	}
	return false
}

func validUUIDArg(arguments map[string]any, key string) bool {
	id, _ := arguments[key].(string)
	return planeAutoUUIDRE.MatchString(id)
}

func nonEmptyStringArg(arguments map[string]any, key string) bool {
	value, _ := arguments[key].(string)
	return strings.TrimSpace(value) != "" && len(value) <= 180
}

func safePlaneWorkitemUpdate(arguments map[string]any) bool {
	// Only task-content/progress edits, not ownership, assignment, security,
	// external provenance, parent rewiring, arbitrary lifecycle operations.
	allowed := map[string]bool{
		"action": true, "project_id": true, "workitem_id": true,
		"name": true, "description_html": true, "description_stripped": true,
		"priority": true, "state": true, "target_date": true,
		"start_date": true, "point": true, "estimate_point": true,
	}
	changed := false
	for key := range arguments {
		if !allowed[key] {
			return false
		}
		if key != "action" && key != "project_id" && key != "workitem_id" {
			changed = true
		}
	}
	return changed
}

func (s *Service) callAutoPlaneCloudMCP(ctx context.Context, user string, c cloud.PluginConnection, cfg cloudmcp.Config, tools []cloudmcp.ToolSummary, args map[string]any) (*mcp.CallToolResult, error) {
	name, _ := args["tool"].(string)
	arguments, ok := args["arguments"].(map[string]any)
	if !ok || !planeAutoActionAllowed(name, arguments) {
		return errorResult(errors.New("Plane operation requires explicit approval")), nil
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
	if s.Store == nil || s.CloudMCP == nil {
		return errorResult(errors.New("Plane Cloud is unavailable")), nil
	}

	var output map[string]any
	var callErr error
	err := s.Store.WithPluginCloudConnection(ctx, user, "plane", c.UpdatedAt, func(locked cloud.PluginConnection) error {
		if locked.UserID != user || locked.PluginID != "plane" ||
			locked.DeviceID != cloud.PluginCloudDeviceID ||
			locked.State != cloud.PluginConnectionReady ||
			locked.Endpoint != cfg.Endpoint {
			return errors.New("Plane connection has changed")
		}
		fresh, err := s.planeCloudMCPConfig(ctx, user, locked)
		if err != nil {
			return err
		}
		if name == cloudmcp.PlaneSprintBatchToolName {
			input := make(map[string]any, len(arguments))
			for k, v := range arguments {
				if k != "action" {
					input[k] = v
				}
			}
			output, callErr = s.CloudMCP.SetupPlaneSprint(ctx, user, fresh, input)
		} else if name == cloudmcp.SocCoMobileCleanupToolName {
			output, callErr = s.CloudMCP.CleanupSocCoMobileBacklog(ctx, user, fresh)
		} else {
			output, callErr = s.CloudMCP.CallApproved(ctx, user, fresh, name, arguments)
		}
		return callErr
	})
	status := "done"
	if err != nil || output["isError"] == true {
		status = "error"
	}
	s.Store.Audit(cloud.AuditEvent{UserID: user, Event: "plugin.mcp_auto_tool_call", Detail: map[string]any{
		"pluginId": "plane", "tool": name, "action": arguments["action"], "target": "cloud", "status": status,
	}})
	if err != nil {
		return textResult(map[string]any{"status": "unknown", "executionStarted": true,
			"message": "Plane operation failed or may have completed. Inspect Plane before retrying.", "server": planeCloudMCPAlias,
		}, true), nil
	}
	return textResult(map[string]any{"status": status, "executionStarted": true,
		"server": planeCloudMCPAlias, "result": output}, status != "done"), nil
}
