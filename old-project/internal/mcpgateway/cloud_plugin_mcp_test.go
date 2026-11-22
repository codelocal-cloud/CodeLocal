package mcpgateway

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/cloudmcp"
	"github.com/0xmarkhydra/codelocal/internal/plugins"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestCloudPlaneAliasIsExactAndFailClosed(t *testing.T) {
	s := &Service{}
	for _, alias := range []string{"cloud:plan", "cloud:penpot", "cloud:plane.evil", "cloud:plane/../other"} {
		result, err := s.callPlaneCloudMCP(context.Background(), "account-a", map[string]any{"action": "search", "server": alias}, nil)
		if err != nil || result == nil || !result.IsError {
			t.Fatalf("alias %q must not resolve as an authorized Plane Cloud plugin", alias)
		}
	}
	result, err := s.callPlaneCloudMCP(context.Background(), "account-a", map[string]any{"action": "search", "server": planeCloudMCPAlias}, nil)
	if err != nil || result == nil || !result.IsError {
		t.Fatal("Cloud MCP without an initialized secure store must fail closed")
	}
}

func TestCloudPlaneCredentialValidationPrecedesSecretLookup(t *testing.T) {
	s := &Service{Store: &cloud.Store{}}
	base := cloud.PluginConnection{
		UserID: "account-a", PluginID: "plane", DeviceID: cloud.PluginCloudDeviceID,
		Endpoint: plugins.PlaneCodeLocalMCPPATURL, State: cloud.PluginConnectionReady,
	}
	for _, tc := range []struct {
		name string
		c    cloud.PluginConnection
	}{
		{"cross-account", func() cloud.PluginConnection { c := base; c.UserID = "account-b"; return c }()},
		{"wrong-plugin", func() cloud.PluginConnection { c := base; c.PluginID = "other"; return c }()},
		{"local-device", func() cloud.PluginConnection { c := base; c.DeviceID = "device"; return c }()},
		{"unready", func() cloud.PluginConnection { c := base; c.State = cloud.PluginConnectionConfigured; return c }()},
		{"untrusted-origin", func() cloud.PluginConnection { c := base; c.Endpoint = "https://evil.example/mcp"; return c }()},
		{"suffix-spoofing", func() cloud.PluginConnection {
			c := base
			c.Endpoint = plugins.PlaneCodeLocalMCPPATURL + ".evil"
			return c
		}()},
	} {
		t.Run(tc.name, func(t *testing.T) {
			_, err := s.planeCloudMCPConfig(context.Background(), "account-a", tc.c)
			if err == nil || !strings.Contains(err.Error(), "not authorized") {
				t.Fatalf("connection must be rejected before querying the encrypted credentials: %v", err)
			}
		})
	}
}

func TestCloudPlaneToolPreviewCannotDispatchWithoutUserConfirmation(t *testing.T) {
	s := &Service{}
	tool := cloudmcp.ToolSummary{Name: "create_work_item", InputSchema: json.RawMessage(`{"type":"object"}`)}
	result, err := s.callApprovedPlaneCloudMCP(
		context.Background(), "account-a", cloud.PluginConnection{}, cloudmcp.Config{},
		[]cloudmcp.ToolSummary{tool},
		map[string]any{
			"action": "call", "server": planeCloudMCPAlias, "tool": "create_work_item",
			"arguments": map[string]any{"name": "task"},
		}, nil,
	)
	if err != nil || result == nil || !result.IsError {
		t.Fatalf("missing MCP session must refuse the call: %v", err)
	}
	raw, _ := json.Marshal(result.Content)
	if !strings.Contains(string(raw), "executionStarted") || !strings.Contains(string(raw), "false") {
		t.Fatalf("missing confirmation must explicitly report that no dispatch occurred: %s", raw)
	}
}

func TestCloudPlaneSearchOnlyFindsMatchingTools(t *testing.T) {
	tool := cloudmcp.ToolSummary{Name: "create_work_item", Description: "Create Plane task"}
	if !cloudPluginMCPToolMatch(tool, "") || !cloudPluginMCPToolMatch(tool, "CREATE") ||
		!cloudPluginMCPToolMatch(tool, "task") || cloudPluginMCPToolMatch(tool, "delete") {
		t.Fatal("Cloud Plane tool matching failed")
	}
}

func TestCompactMCPMaintainsLegacyActionsWithCloudAlias(t *testing.T) {
	var def *compactToolDef
	for _, entry := range compactToolDefinitions() {
		if entry.Name == "mcp" {
			d := entry
			def = &d
			break
		}
	}
	if def == nil || !strings.Contains(def.Description, planeCloudMCPAlias) {
		t.Fatal("current MCP tool must advertise the cloud:plane alias")
	}
	actions := allCompactActions(t, *def)
	if len(actions) != 4 || actions[0] != "list" || actions[1] != "search" ||
		actions[2] != "info" || actions[3] != "call" {
		t.Fatalf("legacy MCP actions must remain compatible: %v", actions)
	}
}

// Compile guard: approval uses an MCP request session, never an arbitrary
// model-provided approvalToken from mcp(action=call).
var _ *mcp.CallToolRequest
