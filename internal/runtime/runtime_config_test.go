package runtime

import (
	"strings"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/taskexecution"
)

func TestRuntimeConfigEnvironmentExportsOnlyNonSecretEnvKeys(t *testing.T) {
	snapshot := cloud.RuntimeConfigSnapshot{Values: map[string]string{
		"FFMPEG_PATH": "ffmpeg", "video.aspect": "9:16",
		taskexecution.RuntimeSettingMaxWorktrees: "3",
	}}
	env := runtimeConfigEnvironment(snapshot)
	if env["FFMPEG_PATH"] != "ffmpeg" {
		t.Fatalf("expected runtime config value missing: %#v", env)
	}
	if _, ok := env["video.aspect"]; ok {
		t.Fatalf("preference key leaked into environment: %#v", env)
	}
	if _, ok := env["VBEE_API_KEY"]; ok {
		t.Fatal("secret unexpectedly merged into ordinary runtime config")
	}
	if _, ok := env[taskexecution.RuntimeSettingMaxWorktrees]; ok {
		t.Fatal("internal worktree setting leaked into task environment")
	}
}

func TestRuntimeTaskExecutionProviderDefaultsSafeAndMapsLive(t *testing.T) {
	if got := runtimeTaskExecutionProvider(cloud.RuntimeConfigSnapshot{}); got != taskexecution.ProviderLocalWorktree {
		t.Fatalf("default provider=%q want %q", got, taskexecution.ProviderLocalWorktree)
	}
	if got := runtimeTaskExecutionProvider(cloud.RuntimeConfigSnapshot{ExecutionMode: cloud.RuntimeExecutionLive}); got != taskexecution.ProviderActiveCheckout {
		t.Fatalf("live provider=%q want %q", got, taskexecution.ProviderActiveCheckout)
	}
}

func TestRuntimeTaskWorktreeLimitDefaultsForUpgrades(t *testing.T) {
	if got := runtimeTaskWorktreeLimit(cloud.RuntimeConfigSnapshot{}); got != 3 {
		t.Fatalf("missing setting limit=%d want 3", got)
	}
	snapshot := cloud.RuntimeConfigSnapshot{Values: map[string]string{taskexecution.RuntimeSettingMaxWorktrees: "7"}}
	if got := runtimeTaskWorktreeLimit(snapshot); got != 7 {
		t.Fatalf("configured limit=%d want 7", got)
	}
}

func TestResolveRuntimeSnapshotNormalizesOpenMontageWithoutInstalling(t *testing.T) {
	snapshot := resolveRuntimeSnapshot(cloud.RuntimeConfigSnapshot{SystemProjects: []cloud.RuntimeSystemProject{{ID: "openmontage", Enabled: true}}})
	if len(snapshot.SystemProjects) != 1 {
		t.Fatalf("projects=%#v", snapshot.SystemProjects)
	}
	project := snapshot.SystemProjects[0]
	if project.Path != "" || project.Name != cloud.OpenMontageName || !project.SystemApp || !project.Managed || project.Hidden || !strings.Contains(project.Source, "OpenMontage") {
		t.Fatalf("project=%#v", project)
	}
}

func TestManagedRuntimeSystemProjectsFiltersDisabledProjects(t *testing.T) {
	settings := map[string]cloud.RuntimeMaterializedConfig{
		"one": {Snapshot: resolveRuntimeSnapshot(cloud.RuntimeConfigSnapshot{SystemProjects: []cloud.RuntimeSystemProject{{ID: "openmontage", Enabled: true}}})},
		"two": {Snapshot: cloud.RuntimeConfigSnapshot{SystemProjects: []cloud.RuntimeSystemProject{{ID: "disabled", Managed: true, Enabled: false}}}},
	}
	projects := managedRuntimeSystemProjects(settings)
	if len(projects) != 1 || projects[0].ID != "openmontage" {
		t.Fatalf("projects=%#v", projects)
	}
}

func TestManagedRuntimeSystemProjectsDoesNotBootstrapWithoutCloudSettings(t *testing.T) {
	if projects := managedRuntimeSystemProjects(nil); len(projects) != 0 {
		t.Fatalf("fresh runtime must not auto-install system apps: %#v", projects)
	}
}

func TestManagedRuntimeSystemProjectsRespectsExplicitOpenMontageDisable(t *testing.T) {
	settings := map[string]cloud.RuntimeMaterializedConfig{
		"one": {Snapshot: cloud.RuntimeConfigSnapshot{SystemProjects: []cloud.RuntimeSystemProject{{ID: "openmontage", Managed: true, Enabled: false}}}},
	}
	if projects := managedRuntimeSystemProjects(settings); len(projects) != 0 {
		t.Fatalf("projects=%#v", projects)
	}
}

func TestValidateManagedSystemProjectRejectsUnexpectedSource(t *testing.T) {
	project := resolveRuntimeSnapshot(cloud.RuntimeConfigSnapshot{SystemProjects: []cloud.RuntimeSystemProject{{ID: "openmontage", Enabled: true}}}).SystemProjects[0]
	project.Source = "https://example.com/not-openmontage.git"
	if err := validateManagedSystemProject(project); err == nil {
		t.Fatal("expected unexpected managed source to be rejected")
	}
}
