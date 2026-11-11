package cloudmcp

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"strings"
	"time"

	plugindomain "github.com/0xmarkhydra/codelocal/internal/plugins"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

const PlaneSprintBatchToolName = "batch_create_sprint"
const planeSprintExternalSource = "codelocal-chatgpt-mobile-sprint"

var planeBatchUUIDRE = regexp.MustCompile(`(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)
var planeBatchKeyRE = regexp.MustCompile(`^[A-Z][A-Z0-9-]{2,31}$`)

type planeSprintBatchItem struct {
	Key         string `json:"key"`
	Name        string `json:"name"`
	Description string `json:"description"`
	Priority    string `json:"priority"`
}
type planeSprintBatchInput struct {
	ProjectID string                 `json:"project_id"`
	CycleName string                 `json:"cycle_name"`
	OwnedBy   string                 `json:"owned_by"`
	Items     []planeSprintBatchItem `json:"items"`
}

var PlaneSprintBatchTool = ToolSummary{
	Name:        PlaneSprintBatchToolName,
	Description: "CodeLocal owner-scoped workflow: find/create a Plane sprint cycle, create up to 25 uniquely keyed work items, attach them to that cycle and read back results. Uses existing connected Plane credentials and read-before-create checks. No automatic retries after uncertain outcomes.",
	InputSchema: json.RawMessage(`{
		"type":"object","additionalProperties":false,
		"required":["project_id","cycle_name","owned_by","items"],
		"properties":{
			"project_id":{"type":"string"},
			"cycle_name":{"type":"string","minLength":5,"maxLength":120},
			"owned_by":{"type":"string"},
			"items":{"type":"array","minItems":1,"maxItems":25,"items":{
				"type":"object","additionalProperties":false,
				"required":["key","name","description","priority"],
				"properties":{
					"key":{"type":"string","minLength":3,"maxLength":32},
					"name":{"type":"string","minLength":8,"maxLength":180},
					"description":{"type":"string","minLength":8,"maxLength":2000},
					"priority":{"type":"string","enum":["urgent","high","medium","low","none"]}
				}
			}}
		}
	}`),
}

func ValidatePlaneSprintBatch(arguments map[string]any) (planeSprintBatchInput, error) {
	var req planeSprintBatchInput
	if arguments == nil {
		return req, errors.New("Plane sprint batch arguments are missing")
	}
	if err := validateArguments(PlaneSprintBatchTool.InputSchema, arguments); err != nil {
		return req, fmt.Errorf("invalid sprint batch: %w", err)
	}
	data, err := json.Marshal(arguments)
	if err != nil || len(data) > 60<<10 {
		return req, errors.New("Plane sprint batch exceeds limit")
	}
	if err = json.Unmarshal(data, &req); err != nil {
		return req, errors.New("Plane sprint batch cannot be decoded")
	}
	if !planeBatchUUIDRE.MatchString(req.ProjectID) || !planeBatchUUIDRE.MatchString(req.OwnedBy) {
		return req, errors.New("Plane project and owner must be UUIDs")
	}
	req.CycleName = strings.TrimSpace(req.CycleName)
	if len(req.CycleName) < 5 || len(req.CycleName) > 120 {
		return req, errors.New("Plane sprint name is invalid")
	}
	used := make(map[string]bool)
	for _, item := range req.Items {
		if !planeBatchKeyRE.MatchString(item.Key) || used[item.Key] {
			return req, errors.New("Plane sprint item keys must be distinct safe identifiers")
		}
		used[item.Key] = true
		if strings.TrimSpace(item.Name) != item.Name || strings.TrimSpace(item.Description) != item.Description {
			return req, errors.New("Plane sprint item names and descriptions must be normalized")
		}
	}
	return req, nil
}

func planeMCPObject(res *mcp.CallToolResult) (map[string]any, error) {
	if res == nil || res.IsError {
		return nil, errors.New("Plane rejected the operation; check Plane before retrying")
	}
	if data, err := json.Marshal(res.StructuredContent); err == nil && string(data) != "null" {
		var obj map[string]any
		if json.Unmarshal(data, &obj) == nil {
			if nested, ok := obj["result"].(map[string]any); ok {
				return nested, nil
			}
			return obj, nil
		}
	}
	for _, content := range res.Content {
		t, ok := content.(*mcp.TextContent)
		if !ok {
			continue
		}
		var obj map[string]any
		if json.Unmarshal([]byte(t.Text), &obj) == nil {
			if nested, ok := obj["result"].(map[string]any); ok {
				return nested, nil
			}
			return obj, nil
		}
	}
	return map[string]any{}, nil
}

func planeMCPItems(result map[string]any) ([]map[string]any, error) {
	items, ok := result["results"].([]any)
	if !ok {
		return nil, errors.New("Plane response has no recognizable results list; aborting to avoid duplicates")
	}
	out := make([]map[string]any, 0, len(items))
	for _, item := range items {
		v, ok := item.(map[string]any)
		if !ok {
			return nil, errors.New("Plane returned an invalid row")
		}
		out = append(out, v)
	}
	return out, nil
}

func planeMCPID(result map[string]any) string {
	if id, ok := result["id"].(string); ok && planeBatchUUIDRE.MatchString(id) {
		return id
	}
	for _, field := range []string{"data", "cycle", "work_item", "workitem", "issue"} {
		if nested, ok := result[field].(map[string]any); ok {
			if id := planeMCPID(nested); id != "" {
				return id
			}
		}
	}
	return ""
}

// SetupPlaneSprint is called ONLY after a backend-owned, owner-confirmed,
// single-use approval was consumed. A stable external ID is checked before
// creating each item. Unknown outcomes never trigger automatic replays.
func (m *Manager) SetupPlaneSprint(ctx context.Context, user string, cfg Config, args map[string]any) (map[string]any, error) {
	if !plugindomain.IsPlaneMCPPATEndpoint(cfg.Endpoint) || cfg.WorkspaceSlug == "" {
		return nil, errors.New("sprint batch requires a trusted Plane PAT endpoint and workspace")
	}
	req, err := ValidatePlaneSprintBatch(args)
	if err != nil {
		return nil, err
	}
	ctx, cancel := context.WithTimeout(ctx, 160*time.Second)
	defer cancel()
	session, release, err := m.open(ctx, user, cfg)
	if err != nil {
		return nil, err
	}
	defer release()
	tools, err := discover(ctx, session)
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
		return nil, errors.New("Plane cycle and workitem tools are required")
	}
	call := func(name string, arguments map[string]any) (map[string]any, error) {
		if err := validateArguments(schemas[name], arguments); err != nil {
			return nil, err
		}
		result, err := session.CallTool(ctx, &mcp.CallToolParams{Name: name, Arguments: arguments})
		if err != nil {
			return nil, errors.New("Plane call outcome unknown; inspect provider state before retrying")
		}
		return planeMCPObject(result)
	}
	out := map[string]any{
		"project_id": req.ProjectID, "cycle_name": req.CycleName, "requested": len(req.Items),
		"external_source": planeSprintExternalSource, "created": []map[string]string{},
		"existing": []map[string]string{}, "attached": false, "verified": false,
	}
	stop := func(message string) (map[string]any, error) {
		out["status"] = "partial_or_unknown"
		out["isError"] = true
		out["message"] = message
		return out, nil
	}
	// Locate an existing cycle before creation. Fail closed on ambiguity.
	cycleID := ""
	cursor := ""
	for page := 0; page < 10; page++ {
		reqArgs := map[string]any{"action": "list", "project_id": req.ProjectID, "per_page": 100}
		if cursor != "" {
			reqArgs["cursor"] = cursor
		}
		result, err := call("cycle", reqArgs)
		if err != nil {
			return stop("Could not safely list cycles: " + err.Error())
		}
		rows, err := planeMCPItems(result)
		if err != nil {
			return stop(err.Error())
		}
		for _, row := range rows {
			if name, _ := row["name"].(string); name == req.CycleName {
				id := planeMCPID(row)
				if id == "" || (cycleID != "" && cycleID != id) {
					return stop("Ambiguous or invalid existing sprint")
				}
				cycleID = id
			}
		}
		next, _ := result["next_cursor"].(string)
		if next == "" || next == cursor || result["next_page_results"] == false {
			break
		}
		if page == 9 {
			return stop("Cycle list was truncated, avoiding duplicate sprint creation")
		}
		cursor = next
	}
	if cycleID == "" {
		result, err := call("cycle", map[string]any{
			"action": "create", "project_id": req.ProjectID, "name": req.CycleName,
			"owned_by": req.OwnedBy, "description": "Mobile-first SOCO Sprint 1: auth, chat, groups, media, notifications, mobile PWA QA. Marketplace, Point/Coin and desktop are excluded.",
		})
		if err != nil {
			return stop("Sprint creation outcome unknown: " + err.Error())
		}
		cycleID = planeMCPID(result)
		if cycleID == "" {
			return stop("Sprint may have been created but no valid UUID was returned; inspect Plane")
		}
		out["cycle_created"] = true
	} else {
		out["cycle_created"] = false
	}
	out["cycle_id"] = cycleID

	// Plane's workitem list rejects filtering by external_id on some
	// self-hosted editions. Read the project inventory without filters,
	// then match exact external keys and titles locally before any mutation.
	existingRows := make([]map[string]any, 0)
	cursor = ""
	for page := 0; page < 10; page++ {
		listArgs := map[string]any{"action": "list", "project_id": req.ProjectID, "per_page": 100}
		if cursor != "" {
			listArgs["cursor"] = cursor
		}
		result, listErr := call("workitem", listArgs)
		if listErr != nil {
			return stop("Cannot safely list work items: " + listErr.Error())
		}
		rows, parseErr := planeMCPItems(result)
		if parseErr != nil {
			return stop("Invalid work item inventory: " + parseErr.Error())
		}
		existingRows = append(existingRows, rows...)
		next, _ := result["next_cursor"].(string)
		if next == "" || next == cursor || result["next_page_results"] == false {
			break
		}
		if page == 9 {
			return stop("Project work item inventory truncated; refusing duplicate creation")
		}
		cursor = next
	}
	ids := make([]string, 0, len(req.Items))
	for _, item := range req.Items {
		existing := ""
		for _, row := range existingRows {
			eid, _ := row["external_id"].(string)
			title, _ := row["name"].(string)
			if eid == item.Key || title == item.Name {
				id := planeMCPID(row)
				if id == "" || (existing != "" && id != existing) {
					return stop("Ambiguous matching task " + item.Key)
				}
				existing = id
			}
		}
		if existing != "" {
			out["existing"] = append(out["existing"].([]map[string]string), map[string]string{"key": item.Key, "id": existing})
			ids = append(ids, existing)
			continue
		}
		created, createErr := call("workitem", map[string]any{
			"action": "create", "project_id": req.ProjectID,
			"name": item.Name, "description_stripped": item.Description, "priority": item.Priority,
			"external_source": planeSprintExternalSource, "external_id": item.Key,
		})
		if createErr != nil {
			return stop("Task " + item.Key + " may have been created: " + createErr.Error())
		}
		id := planeMCPID(created)
		if id == "" {
			return stop("Task " + item.Key + " may have been created but no valid UUID was returned")
		}
		out["created"] = append(out["created"].([]map[string]string), map[string]string{"key": item.Key, "id": id})
		ids = append(ids, id)
		existingRows = append(existingRows, map[string]any{"id": id, "name": item.Name, "external_id": item.Key})
	}
	if len(ids) != len(req.Items) {
		return stop("Not all work items resolved")
	}
	_, err = call("cycle", map[string]any{
		"action": "manage_workitems", "project_id": req.ProjectID,
		"cycle_id": cycleID, "add_ids": strings.Join(ids, ","),
	})
	if err != nil {
		return stop("Sprint membership outcome unknown: " + err.Error())
	}
	out["attached"] = true
	// Check cycle membership before declaring completion.
	seen := map[string]bool{}
	cursor = ""
	for page := 0; page < 10; page++ {
		reqArgs := map[string]any{"action": "list_workitems", "project_id": req.ProjectID, "cycle_id": cycleID, "per_page": 100}
		if cursor != "" {
			reqArgs["cursor"] = cursor
		}
		res, err := call("cycle", reqArgs)
		if err != nil {
			return stop("Sprint membership verification failed: " + err.Error())
		}
		rows, err := planeMCPItems(res)
		if err != nil {
			return stop("Sprint verification returned invalid list")
		}
		for _, row := range rows {
			if id := planeMCPID(row); id != "" {
				seen[id] = true
			}
		}
		next, _ := res["next_cursor"].(string)
		if next == "" || next == cursor || res["next_page_results"] == false {
			break
		}
		if page == 9 {
			return stop("Sprint verification returned too many pages")
		}
		cursor = next
	}
	for _, id := range ids {
		if !seen[id] {
			return stop("Cycle membership incomplete; inspect Plane")
		}
	}
	out["verified"] = true
	out["verified_count"] = len(ids)
	out["status"] = "done"
	return out, nil
}
