package cloudmcp

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/plugins"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

// This is a deliberately narrow one-off cleanup tool for the CodeLocal-made
// SocCo mobile backlog only. It cannot accept arbitrary project IDs, cycle
// IDs, work item IDs, or provenance tags from an MCP caller.
const SocCoMobileCleanupToolName = "cleanup_socco_mobile_backlog"
const cleanupProjectID = "2ec86630-f40c-4c39-be78-c22c88780517"
const cleanupCycleID = "b58eddd8-767f-4256-a28a-37da0eb76aa2"

var SocCoMobileCleanupTool = ToolSummary{
	Name:        SocCoMobileCleanupToolName,
	Description: "One-off cleanup only: delete up to 8 work items per call from SocCo Sprint 1 that have the exact external_source=codelocal-chatgpt-mobile-sprint and an external_id S1-MOB-01..S1-MOB-22. Does not touch the Sprint cycle or any other tasks. Repeat to finish; verifies after each batch.",
	InputSchema: json.RawMessage(`{"type":"object","properties":{},"additionalProperties":false}`),
}

func isGeneratedMobileTask(row map[string]any) bool {
	external, _ := row["external_source"].(string)
	key, _ := row["external_id"].(string)
	title, _ := row["name"].(string)
	if external != planeSprintExternalSource {
		return false
	}
	if !strings.HasPrefix(title, "["+key+"] ") {
		return false
	}
	for i := 1; i <= 22; i++ {
		if key == fmt.Sprintf("S1-MOB-%02d", i) {
			return planeMCPID(row) != ""
		}
	}
	return false
}

func (m *Manager) CleanupSocCoMobileBacklog(ctx context.Context, user string, cfg Config) (map[string]any, error) {
	if !plugins.IsPlaneMCPPATEndpoint(cfg.Endpoint) || cfg.WorkspaceSlug != "socco" {
		return nil, errors.New("cleanup restricted to a ready SocCo Plane connection")
	}
	ctx, cancel := context.WithTimeout(ctx, 145*time.Second)
	defer cancel()
	sess, release, err := m.open(ctx, user, cfg)
	if err != nil {
		return nil, err
	}
	defer release()
	tools, err := discover(ctx, sess)
	if err != nil {
		return nil, err
	}
	schemas := map[string]json.RawMessage{}
	for _, tool := range tools {
		if tool.Name == "cycle" || tool.Name == "workitem" {
			schemas[tool.Name] = tool.InputSchema
		}
	}
	if len(schemas) != 2 {
		return nil, errors.New("Plane workitem/cycle tools unavailable")
	}
	call := func(name string, args map[string]any) (map[string]any, error) {
		if err := validateArguments(schemas[name], args); err != nil {
			return nil, err
		}
		res, err := sess.CallTool(ctx, &mcp.CallToolParams{Name: name, Arguments: args})
		if err != nil {
			return nil, fmt.Errorf("Plane %s call result unknown: %w", name, err)
		}
		return planeMCPObject(res)
	}
	result := map[string]any{"project_id": cleanupProjectID, "cycle_id": cleanupCycleID, "deleted": []map[string]string{}, "status": "partial", "verified": false}
	stop := func(reason string) (map[string]any, error) {
		result["status"] = "partial_or_unknown"
		result["isError"] = true
		result["message"] = reason
		return result, nil
	}
	// Read all project items before selecting any deletion target.
	inventory := make([]map[string]any, 0)
	cursor := ""
	for page := 0; page < 10; page++ {
		args := map[string]any{"action": "list", "project_id": cleanupProjectID, "per_page": 100}
		if cursor != "" {
			args["cursor"] = cursor
		}
		res, err := call("workitem", args)
		if err != nil {
			return stop("Project inventory unavailable; no deletion started: " + err.Error())
		}
		rows, err := planeMCPItems(res)
		if err != nil {
			return stop(err.Error())
		}
		inventory = append(inventory, rows...)
		next, _ := res["next_cursor"].(string)
		if next == "" || next == cursor || res["next_page_results"] == false {
			break
		}
		if page == 9 {
			return stop("Project inventory truncated; no deletion started")
		}
		cursor = next
	}
	matches := make([]map[string]any, 0)
	seen := map[string]bool{}
	for _, row := range inventory {
		if !isGeneratedMobileTask(row) {
			continue
		}
		key := row["external_id"].(string)
		if seen[key] {
			return stop("Duplicate generated task key; refusing to delete")
		}
		seen[key] = true
		matches = append(matches, row)
	}
	// Independent cycle membership check prevents deleting otherwise similar
	// tasks that a person moved into a different cycle after creation.
	members := map[string]bool{}
	cursor = ""
	for page := 0; page < 10; page++ {
		args := map[string]any{"action": "list_workitems", "project_id": cleanupProjectID, "cycle_id": cleanupCycleID, "per_page": 100}
		if cursor != "" {
			args["cursor"] = cursor
		}
		res, err := call("cycle", args)
		if err != nil {
			return stop("Sprint inventory unavailable; no deletion started: " + err.Error())
		}
		rows, err := planeMCPItems(res)
		if err != nil {
			return stop(err.Error())
		}
		for _, row := range rows {
			if id := planeMCPID(row); id != "" {
				members[id] = true
			}
		}
		next, _ := res["next_cursor"].(string)
		if next == "" || next == cursor || res["next_page_results"] == false {
			break
		}
		if page == 9 {
			return stop("Sprint membership inventory truncated; no deletion started")
		}
		cursor = next
	}
	for _, row := range matches {
		if !members[planeMCPID(row)] {
			return stop("Generated task moved out of target cycle; no deletion started to prevent wrong-scope changes")
		}
	}
	result["before"] = len(matches)
	// Small bounded batches protect the MCP request deadline. On an unknown
	// result, stop immediately. Read fresh provider state before retrying.
	maxBatch := 8
	if len(matches) < maxBatch {
		maxBatch = len(matches)
	}
	for i := 0; i < maxBatch; i++ {
		row := matches[i]
		id := planeMCPID(row)
		_, err := call("workitem", map[string]any{"action": "delete", "project_id": cleanupProjectID, "workitem_id": id})
		if err != nil {
			return stop("Delete outcome for " + row["external_id"].(string) + " is unknown; refresh Plane before trying again")
		}
		result["deleted"] = append(result["deleted"].([]map[string]string), map[string]string{"key": row["external_id"].(string), "id": id})
	}
	// Verify remaining task count after deletion, not just an MCP 200.
	check, err := call("workitem", map[string]any{"action": "list", "project_id": cleanupProjectID, "per_page": 100})
	if err != nil {
		return stop("Delete ran but verification failed; check Plane")
	}
	list, err := planeMCPItems(check)
	if err != nil {
		return stop("Delete verification returned malformed data")
	}
	if check["next_page_results"] == true {
		return stop("Delete verification inventory paginated")
	}
	left := 0
	for _, row := range list {
		if isGeneratedMobileTask(row) {
			left++
		}
	}
	result["remaining"] = left
	result["deleted_count"] = len(result["deleted"].([]map[string]string))
	result["verified"] = true
	if left == 0 {
		result["status"] = "done"
	} else {
		result["status"] = "more_remaining"
	}
	return result, nil
}
