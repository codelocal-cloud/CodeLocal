package plugins

import "testing"

func TestPlaneIsOptInCloudOAuthPlugin(t *testing.T) {
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
	if template.Transport != TransportMCPHTTP || template.Auth.Kind != AuthOAuth2 {
		t.Fatalf("Plane cloud plugin must use HTTP with OAuth: %#v", template)
	}
	if len(template.Execution) != 1 || template.Execution[0] != ExecutionCloud {
		t.Fatalf("Plane cloud plugin should not claim device execution: %#v", template.Execution)
	}
	if len(template.Fields) != 1 || template.Fields[0].Description != PlaneHostedMCPOAuthURL {
		t.Fatalf("unexpected Plane MCP endpoint: %#v", template.Fields)
	}
	if err := ValidateManifest(entry.Manifest); err != nil {
		t.Fatalf("Plane plugin manifest is invalid: %v", err)
	}
}
