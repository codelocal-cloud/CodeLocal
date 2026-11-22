package taskexecution

import (
	"context"
	"errors"
	"path/filepath"
	"testing"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/repository"
)

func TestManagerPersistsAndResumesTaskExecutionBundle(t *testing.T) {
	ctx := context.Background()
	repo := makeTaskRepo(t)
	store := NewStore(filepath.Join(t.TempDir(), "state"))
	provider := NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees"))
	manager := NewManager(store, provider)
	req := PrepareRequest{
		TaskID: "task-a", WorkspaceKey: "workspace", WorkspaceID: "ws", ProjectID: "project",
		Repositories: []repository.Checkout{repo},
	}

	first, err := manager.EnsureLocal(ctx, req, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if first.Lease.OwnerID != "agent-a" || len(first.RepositoryBindings) != 1 {
		t.Fatalf("unexpected first bundle: %#v", first)
	}
	if _, err := manager.EnsureLocal(ctx, req, "agent-b", time.Minute); !errors.Is(err, ErrLeaseHeld) {
		t.Fatalf("second owner must not take an active task: %v", err)
	}
	if _, err := manager.Release(req.WorkspaceKey, req.TaskID, "agent-a"); err != nil {
		t.Fatal(err)
	}
	resumed, err := manager.EnsureLocal(ctx, req, "agent-b", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if resumed.RepositoryBindings[0].LocalPath != first.RepositoryBindings[0].LocalPath {
		t.Fatal("task resume did not reuse its persisted execution binding")
	}
}

func TestManagerActiveCheckoutBindsAuthoritativeRepository(t *testing.T) {
	ctx := context.Background()
	repo := makeTaskRepo(t)
	manager := NewManager(NewStore(filepath.Join(t.TempDir(), "state")), nil)
	req := PrepareRequest{TaskID: "task-live", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo}}

	bundle, err := manager.Ensure(ctx, req, ProviderActiveCheckout, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if bundle.Provider != ProviderActiveCheckout || len(bundle.RepositoryBindings) != 1 {
		t.Fatalf("unexpected live execution bundle: %#v", bundle)
	}
	if got := filepath.Clean(bundle.RepositoryBindings[0].LocalPath); got != filepath.Clean(repo.Root) {
		t.Fatalf("live execution path=%q want authoritative checkout %q", got, repo.Root)
	}
}

func TestManagerKeepsExistingProviderWhenPreferenceChanges(t *testing.T) {
	ctx := context.Background()
	repo := makeTaskRepo(t)
	manager := NewManager(NewStore(filepath.Join(t.TempDir(), "state")), NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")))
	req := PrepareRequest{TaskID: "task-stable-provider", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo}}

	first, err := manager.Ensure(ctx, req, ProviderActiveCheckout, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Release(req.WorkspaceKey, req.TaskID, "agent-a"); err != nil {
		t.Fatal(err)
	}
	resumed, err := manager.Ensure(ctx, req, ProviderLocalWorktree, "agent-b", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if resumed.Provider != ProviderActiveCheckout || resumed.RepositoryBindings[0].LocalPath != first.RepositoryBindings[0].LocalPath {
		t.Fatalf("existing task provider changed after preference update: first=%#v resumed=%#v", first, resumed)
	}
}

func TestManagerActiveCheckoutBlocksConcurrentTasksOnSameRepository(t *testing.T) {
	ctx := context.Background()
	repo := makeTaskRepo(t)
	manager := NewManager(NewStore(filepath.Join(t.TempDir(), "state")), NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")))

	firstReq := PrepareRequest{TaskID: "task-live-a", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo}}
	if _, err := manager.Ensure(ctx, firstReq, ProviderActiveCheckout, "agent-a", time.Minute); err != nil {
		t.Fatal(err)
	}

	secondReq := PrepareRequest{TaskID: "task-live-b", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo}}
	if _, err := manager.Ensure(ctx, secondReq, ProviderActiveCheckout, "agent-b", time.Minute); !errors.Is(err, ErrActiveCheckoutBusy) {
		t.Fatalf("second live task must not share authoritative checkout: %v", err)
	}

	safeReq := PrepareRequest{TaskID: "task-safe", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo}}
	if _, err := manager.Ensure(ctx, safeReq, ProviderLocalWorktree, "agent-safe", time.Minute); err != nil {
		t.Fatalf("safe worktree should remain independent from live checkout lease: %v", err)
	}

	if _, err := manager.Release(firstReq.WorkspaceKey, firstReq.TaskID, "agent-a"); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Ensure(ctx, secondReq, ProviderActiveCheckout, "agent-b", time.Minute); err != nil {
		t.Fatalf("released live checkout should be reusable: %v", err)
	}
}

func TestManagerExpandsRepositoryCoverageWithoutReplacingExistingBinding(t *testing.T) {
	ctx := context.Background()
	firstRepo := makeTaskRepo(t)
	secondRepo := makeTaskRepo(t)
	secondRepo.ID = "repo-b"
	secondRepo.RelativePath = "backend/auth"
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)
	req := PrepareRequest{TaskID: "task-a", WorkspaceKey: "workspace", Repositories: []repository.Checkout{firstRepo}}
	first, err := manager.EnsureLocal(ctx, req, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	firstPath := first.RepositoryBindings[0].LocalPath
	if _, err := manager.Release(req.WorkspaceKey, req.TaskID, "agent-a"); err != nil {
		t.Fatal(err)
	}
	req.Repositories = []repository.Checkout{secondRepo}
	expanded, err := manager.EnsureLocal(ctx, req, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if len(expanded.RepositoryBindings) != 2 {
		t.Fatalf("expected task bundle to expand to two repositories: %#v", expanded.RepositoryBindings)
	}
	foundFirst := false
	for _, binding := range expanded.RepositoryBindings {
		if binding.RepositoryID == firstRepo.ID && binding.LocalPath == firstPath {
			foundFirst = true
		}
	}
	if !foundFirst {
		t.Fatalf("existing task binding was replaced during expansion: %#v", expanded.RepositoryBindings)
	}
}
