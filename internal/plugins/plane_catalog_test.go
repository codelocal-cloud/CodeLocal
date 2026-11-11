package plugins

import "testing"

func TestPlaneIsOptInCloudPATPlugin(t *testing.T) {
	entry, ok := FindBuiltin("plane")
	if !ok {
		t.Fatal("Plane plugin missing from built-in catalog")
	}
	if entry.DefaultInstalled || !entry.Featured {
		t.Fatalf("Plane should be opt-in and featured: %#v", entry)
	}
	if entry.Manifest.Scope != ScopeSystem || entry.Manifest.Distribution != DistributionInternal {
		t.Fatalf("unexpected Plane manifest ownership: %#v", entry.Manifest)
	}
	if entry.Runtime != nil {
		t.Fatal("Plane must not claim a managed local runtime")
	}
	if len(entry.Manifest.Components) != 1 {
		t.Fatalf("unexpected Plane component count: %d", len(entry.Manifest.Components))
	}
	component := entry.Manifest.Components[0]
	if component.Kind != ComponentAppTemplate || component.AppTemplate == nil {
		t.Fatalf("Plane must expose an MCP connection template: %#v", component)
	}
	template := component.AppTemplate
	if template.Transport != TransportMCPHTTP || template.Auth.Kind != AuthHeaderReference {
		t.Fatalf("Plane cloud plugin must use HTTP with a token: %#v", template)
	}
	if len(template.Execution) != 1 || template.Execution[0] != ExecutionCloud {
		t.Fatalf("Plane cloud plugin should not claim device execution: %#v", template.Execution)
	}
	if len(template.Fields) != 3 || template.Fields[0].Description != PlaneCodeLocalMCPPATURL || !template.Fields[2].Secret {
		t.Fatalf("unexpected Plane MCP endpoint: %#v", template.Fields)
	}
	if !IsPlaneMCPPATEndpoint(PlaneHostedMCPPATURL) || !IsPlaneMCPPATEndpoint(PlaneCodeLocalMCPPATURL) {
		t.Fatal("trusted Plane MCP endpoints not registered")
	}
	for _, raw := range []string{"https://evil.example/http/api-key/mcp", "https://planemcp-plane.up.railway.app/http/mcp", "https://planemcp-plane.up.railway.app.evil.com/http/api-key/mcp", "http://planemcp-plane.up.railway.app/http/api-key/mcp"} {
		if IsPlaneMCPPATEndpoint(raw) {
			t.Fatalf("untrusted Plane endpoint was allowed: %s", raw)
		}
	}
	if err := ValidateManifest(entry.Manifest); err != nil {
		t.Fatalf("Plane plugin manifest is invalid: %v", err)
	}
}
