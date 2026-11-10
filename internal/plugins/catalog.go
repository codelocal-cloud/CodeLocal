package plugins

import (
	"fmt"
	"sort"
)

const ManagedPenpotMCPURL = "https://design.codelocal.cloud/mcp/stream"

// Plane's PAT endpoint supports remote, per-account connections without OAuth
// redirect allowlisting. Self-hosted Plane can use the local stdio MCP Hub.
const PlaneHostedMCPPATURL = "https://mcp.plane.so/http/api-key/mcp"

type CatalogEntry struct {
	Manifest         Manifest        `json:"manifest"`
	Featured         bool            `json:"featured,omitempty"`
	DefaultInstalled bool            `json:"defaultInstalled,omitempty"`
	Runtime          *RuntimeBinding `json:"runtime,omitempty"`
}

func BuiltinCatalog() []CatalogEntry {
	return []CatalogEntry{
		builtinPenpotCatalogEntry(),
		builtinPlaneCatalogEntry(),
	}
}

func builtinPenpotCatalogEntry() CatalogEntry {
	return CatalogEntry{
		Featured:         true,
		DefaultInstalled: true,
		Runtime:          &RuntimeBinding{ServerName: "penpot", Targets: []ExecutionTarget{ExecutionLocal, ExecutionCloud}},
		Manifest: Manifest{
			SchemaVersion: SchemaVersion,
			ID:            "penpot",
			Name:          "Penpot Design",
			Version:       "2.17.0",
			Publisher:     Publisher{ID: "codelocal", Name: "CodeLocal", Verified: true},
			Description:   "Create and edit product designs through CodeLocal's managed Penpot workspace and MCP bridge.",
			Categories:    []string{"Design", "Developer Tools"},
			Scope:         ScopeSystem,
			Distribution:  DistributionInternal,
			Components: []Component{{
				Kind: ComponentAppTemplate,
				ID:   "penpot-design",
				AppTemplate: &AppTemplateDefinition{
					Transport:    TransportMCPHTTP,
					Auth:         AuthDefinition{Kind: AuthHeaderReference},
					Capabilities: []Capability{CapabilityExternalRead, CapabilityExternalWrite, CapabilityNetwork},
					Execution:    []ExecutionTarget{ExecutionLocal, ExecutionCloud},
					Fields: []AppTemplateField{
						{Key: "endpoint", Label: "Hosted Penpot MCP endpoint", Required: true, Description: ManagedPenpotMCPURL},
						{Key: "userToken", Label: "Penpot MCP key", Required: true, Secret: true, Description: "Generate this key in Penpot under Account → Integrations → MCP Server."},
					},
				},
			}},
		},
	}
}

// builtinPlaneCatalogEntry adds an opt-in hosted Plane MCP integration.
// The workspace-scoped PAT is encrypted per CodeLocal account; tool calls still require approval.
func builtinPlaneCatalogEntry() CatalogEntry {
	return CatalogEntry{
		Featured: true,
		Manifest: Manifest{
			SchemaVersion: SchemaVersion,
			ID:            "plane",
			Name:          "Plane Project Management",
			Version:       "0.2.0",
			Publisher:     Publisher{ID: "codelocal", Name: "CodeLocal", Verified: true},
			Description:   "Manage Plane projects, work items, cycles, and modules through the official Plane MCP server using a workspace-scoped access token.",
			Categories:    []string{"Project Management", "Productivity", "Developer Tools"},
			Scope:         ScopeSystem,
			Distribution:  DistributionInternal,
			Components: []Component{{
				Kind: ComponentAppTemplate,
				ID:   "plane-mcp",
				AppTemplate: &AppTemplateDefinition{
					Transport:    TransportMCPHTTP,
					Auth:         AuthDefinition{Kind: AuthHeaderReference},
					Capabilities: []Capability{CapabilityExternalRead, CapabilityExternalWrite, CapabilityExternalDelete, CapabilityNetwork},
					Execution:    []ExecutionTarget{ExecutionCloud},
					Fields: []AppTemplateField{
						{Key: "endpoint", Label: "Official Plane PAT MCP endpoint", Required: true, Description: PlaneHostedMCPPATURL},
						{Key: "workspaceSlug", Label: "Plane workspace slug", Required: true},
						{Key: "bearerToken", Label: "Plane personal or workspace access token", Required: true, Secret: true},
					},
				},
			}},
		},
	}
}

func builtinCatalogEntry(id, name, description string, categories []string, capabilities []Capability, featured bool) CatalogEntry {
	return CatalogEntry{
		Featured: featured,
		Manifest: Manifest{
			SchemaVersion: SchemaVersion,
			ID:            id,
			Name:          name,
			Version:       "0.1.0",
			Publisher:     Publisher{ID: "codelocal", Name: "CodeLocal", Verified: true},
			Description:   description,
			Categories:    append([]string(nil), categories...),
			Scope:         ScopeSystem,
			Distribution:  DistributionInternal,
			Components: []Component{{
				Kind: ComponentAppTemplate,
				ID:   id + "-app",
				AppTemplate: &AppTemplateDefinition{
					Transport:    TransportMCPHTTP,
					Auth:         AuthDefinition{Kind: AuthHeaderReference},
					Capabilities: append([]Capability(nil), capabilities...),
					Execution:    []ExecutionTarget{ExecutionLocal, ExecutionCloud},
					Fields: []AppTemplateField{
						{Key: "endpoint", Label: "MCP endpoint", Required: true, Description: fmt.Sprintf("HTTPS MCP endpoint for the %s integration.", name)},
						{Key: "bearerEnv", Label: "Bearer token environment variable", Description: "Optional local environment variable name. CodeLocal Cloud never receives the token value."},
					},
				},
			}},
		},
	}
}

func FindBuiltin(pluginID string) (CatalogEntry, bool) {
	for _, entry := range BuiltinCatalog() {
		if entry.Manifest.ID == pluginID {
			return entry, true
		}
	}
	return CatalogEntry{}, false
}

func ManifestCapabilities(manifest Manifest) []Capability {
	seen := map[Capability]struct{}{}
	for _, component := range manifest.Components {
		if component.App != nil {
			for _, capability := range component.App.Capabilities {
				seen[capability] = struct{}{}
			}
		}
		if component.AppTemplate != nil {
			for _, capability := range component.AppTemplate.Capabilities {
				seen[capability] = struct{}{}
			}
		}
	}
	out := make([]Capability, 0, len(seen))
	for capability := range seen {
		out = append(out, capability)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}
