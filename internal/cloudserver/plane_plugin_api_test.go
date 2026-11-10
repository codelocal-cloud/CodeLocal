package cloudserver

import (
	"context"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	plugindomain "github.com/0xmarkhydra/codelocal/internal/plugins"
)

func TestPlaneAppearsInCatalogButIsNotPreinstalled(t *testing.T) {
	catalog, err := pluginCatalogResponse(nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, plugin := range catalog.Items {
		if plugin.ID != "plane" {
			continue
		}
		if plugin.Installed || plugin.System || !plugin.SetupRequired {
			t.Fatalf("Plane must be opt-in and require connection: %#v", plugin)
		}
		if len(plugin.ExecutionTargets) != 1 || plugin.ExecutionTargets[0] != "cloud" {
			t.Fatalf("unexpected Plane execution targets: %#v", plugin.ExecutionTargets)
		}
		return
	}
	t.Fatal("Plane is missing from plugin catalog")
}

func TestPlaneCloudConnectionRejectsEndpointAuthAndSlugMismatch(t *testing.T) {
	server := &Server{}
	cases := []struct {
		name       string
		endpoint   string
		credential cloud.PluginCloudCredential
		wantError  bool
	}{
		{
			name:       "official PAT",
			endpoint:   plugindomain.PlaneHostedMCPPATURL,
			credential: cloud.PluginCloudCredential{Kind: "bearer", Endpoint: plugindomain.PlaneHostedMCPPATURL, Token: "test-token", WorkspaceSlug: "sample-team"},
		},
		{
			name:       "untrusted MCP endpoint",
			endpoint:   "https://other.example.com/mcp",
			credential: cloud.PluginCloudCredential{Kind: "bearer", Endpoint: "https://other.example.com/mcp", Token: "test-token", WorkspaceSlug: "sample-team"},
			wantError:  true,
		},
		{
			name:       "oauth instead of PAT",
			endpoint:   plugindomain.PlaneHostedMCPPATURL,
			credential: cloud.PluginCloudCredential{Kind: "oauth", Endpoint: plugindomain.PlaneHostedMCPPATURL, Token: "test-token", WorkspaceSlug: "sample-team"},
			wantError:  true,
		},
		{
			name:       "empty workspace",
			endpoint:   plugindomain.PlaneHostedMCPPATURL,
			credential: cloud.PluginCloudCredential{Kind: "bearer", Endpoint: plugindomain.PlaneHostedMCPPATURL, Token: "test-token"},
			wantError:  true,
		},
		{
			name:       "injected header",
			endpoint:   plugindomain.PlaneHostedMCPPATURL,
			credential: cloud.PluginCloudCredential{Kind: "bearer", Endpoint: plugindomain.PlaneHostedMCPPATURL, Token: "test-token", WorkspaceSlug: "bad\r\nx-trick"},
			wantError:  true,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			config, err := server.pluginCloudConfig(context.Background(), "user", "plane", tc.endpoint, tc.credential, false)
			if (err != nil) != tc.wantError {
				t.Fatalf("error=%v wantError=%v", err, tc.wantError)
			}
			if err == nil && (config.Endpoint != plugindomain.PlaneHostedMCPPATURL || config.WorkspaceSlug != "sample-team") {
				t.Fatalf("unexpected Plane MCP config: %#v", config)
			}
		})
	}
}
