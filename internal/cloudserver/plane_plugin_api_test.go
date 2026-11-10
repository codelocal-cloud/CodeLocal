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

func TestPlaneCloudConnectionRejectsEndpointAndAuthMismatch(t *testing.T) {
	server := &Server{}
	cases := []struct {
		name       string
		endpoint   string
		credential cloud.PluginCloudCredential
		wantError  bool
	}{
		{
			name:     "official OAuth",
			endpoint: plugindomain.PlaneHostedMCPOAuthURL,
			credential: cloud.PluginCloudCredential{
				Kind: "oauth", Endpoint: plugindomain.PlaneHostedMCPOAuthURL, Token: "test-token",
			},
		},
		{
			name:     "untrusted MCP endpoint",
			endpoint: "https://other.example.com/mcp",
			credential: cloud.PluginCloudCredential{
				Kind: "oauth", Endpoint: "https://other.example.com/mcp", Token: "test-token",
			},
			wantError: true,
		},
		{
			name:     "bearer instead of OAuth",
			endpoint: plugindomain.PlaneHostedMCPOAuthURL,
			credential: cloud.PluginCloudCredential{
				Kind: "bearer", Endpoint: plugindomain.PlaneHostedMCPOAuthURL, Token: "test-token",
			},
			wantError: true,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			config, err := server.pluginCloudConfig(context.Background(), "user", "plane", tc.endpoint, tc.credential, false)
			if (err != nil) != tc.wantError {
				t.Fatalf("error = %v, wantError %v", err, tc.wantError)
			}
			if err == nil && config.Endpoint != plugindomain.PlaneHostedMCPOAuthURL {
				t.Fatalf("unexpected MCP endpoint: %q", config.Endpoint)
			}
		})
	}
}
