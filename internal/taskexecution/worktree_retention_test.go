package taskexecution

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/repository"
)

func addTaskRemote(t *testing.T, repo repository.Checkout) string {
	t.Helper()
	remote := filepath.Join(t.TempDir(), "remote.git")
	gitTaskTest(t, filepath.Dir(remote), "init", "--bare", "-q", remote)
	gitTaskTest(t, repo.Root, "remote", "add", "origin", remote)
	gitTaskTest(t, repo.Root, "push", "-u", "origin", "HEAD")
	return remote
}

func ensureAndReleaseTask(t *testing.T, manager *Manager, repo repository.Checkout, taskID string) Bundle {
	t.Helper()
	bundle, err := manager.EnsureLocal(context.Background(), PrepareRequest{
		TaskID: taskID, WorkspaceKey: "workspace", WorkspaceID: "ws", ProjectID: "project",
		Repositories: []repository.Checkout{repo},
	}, "agent-"+taskID, time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Release("workspace", taskID, "agent-"+taskID); err != nil {
		t.Fatal(err)
	}
	return bundle
}

func TestManagerDefaultsToThreeWorktrees(t *testing.T) {
	manager := NewManager(nil, nil)
	if got := manager.MaxWorktrees(); got != 3 {
		t.Fatalf("default max worktrees=%d want 3", got)
	}
	for _, test := range []struct {
		value string
		want  int
	}{
		{"", 3},
		{"invalid", 3},
		{"0", 3},
		{"21", 3},
		{"1", 1},
		{"20", 20},
	} {
		if got := ParseMaxWorktrees(test.value); got != test.want {
			t.Fatalf("ParseMaxWorktrees(%q)=%d want %d", test.value, got, test.want)
		}
	}
}

func TestManagerArchivesOldestDirtyWorktreeBeforeRemovingIt(t *testing.T) {
	repo := makeTaskRepo(t)
	remote := addTaskRemote(t, repo)
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)

	oldest := ensureAndReleaseTask(t, manager, repo, "task-1")
	oldestPath := oldest.RepositoryBindings[0].LocalPath
	if err := os.WriteFile(filepath.Join(oldestPath, "app.txt"), []byte("saved remotely\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	ensureAndReleaseTask(t, manager, repo, "task-2")
	ensureAndReleaseTask(t, manager, repo, "task-3")
	newest := ensureAndReleaseTask(t, manager, repo, "task-4")

	if _, err := os.Stat(oldestPath); !os.IsNotExist(err) {
		t.Fatalf("oldest worktree still exists: %v", err)
	}
	if _, found, err := manager.Store.Get("workspace", "task-1"); err != nil || found {
		t.Fatalf("oldest bundle was not removed: found=%v err=%v", found, err)
	}
	archiveRef := "refs/heads/" + archiveBranch("task-1")
	archiveCommit := strings.TrimSpace(gitTaskTest(t, repo.Root, "--git-dir", remote, "rev-parse", archiveRef))
	content := gitTaskTest(t, repo.Root, "--git-dir", remote, "show", archiveCommit+":app.txt")
	if content != "saved remotely\n" {
		t.Fatalf("archive branch lost dirty state: %q", content)
	}
	if _, err := os.Stat(newest.RepositoryBindings[0].LocalPath); err != nil {
		t.Fatalf("newest worktree was removed instead of oldest: %v", err)
	}
}

func TestManagerDoesNotRemoveLeasedOldestWorktree(t *testing.T) {
	repo := makeTaskRepo(t)
	addTaskRemote(t, repo)
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)
	manager.SetMaxWorktrees(1)
	active, err := manager.EnsureLocal(context.Background(), PrepareRequest{
		TaskID: "task-active", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo},
	}, "agent-active", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	_, err = manager.EnsureLocal(context.Background(), PrepareRequest{
		TaskID: "task-new", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo},
	}, "agent-new", time.Minute)
	if !errors.Is(err, ErrWorktreeLimitReached) {
		t.Fatalf("new task error=%v want ErrWorktreeLimitReached", err)
	}
	if _, err := os.Stat(active.RepositoryBindings[0].LocalPath); err != nil {
		t.Fatalf("active worktree was removed: %v", err)
	}
}

func TestManagerDoesNotRemoveWorktreeUsedByRunningProcess(t *testing.T) {
	repo := makeTaskRepo(t)
	addTaskRemote(t, repo)
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)
	manager.SetMaxWorktrees(1)
	running := ensureAndReleaseTask(t, manager, repo, "task-running")
	runningPath := running.RepositoryBindings[0].LocalPath
	manager.SetWorktreeInUse(func(path string) bool { return path == runningPath })

	_, err := manager.EnsureLocal(context.Background(), PrepareRequest{
		TaskID: "task-new", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo},
	}, "agent-new", time.Minute)
	if !errors.Is(err, ErrWorktreeLimitReached) {
		t.Fatalf("new task error=%v want ErrWorktreeLimitReached", err)
	}
	if _, err := os.Stat(runningPath); err != nil {
		t.Fatalf("running worktree was removed: %v", err)
	}
}

func TestManagerKeepsWorktreeWhenArchivePushFails(t *testing.T) {
	repo := makeTaskRepo(t)
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)
	manager.SetMaxWorktrees(1)
	oldest := ensureAndReleaseTask(t, manager, repo, "task-old")
	oldestPath := oldest.RepositoryBindings[0].LocalPath
	if err := os.WriteFile(filepath.Join(oldestPath, "app.txt"), []byte("must survive\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := manager.EnsureLocal(context.Background(), PrepareRequest{
		TaskID: "task-new", WorkspaceKey: "workspace", Repositories: []repository.Checkout{repo},
	}, "agent-new", time.Minute)
	if !errors.Is(err, ErrWorktreeLimitReached) {
		t.Fatalf("new task error=%v want ErrWorktreeLimitReached", err)
	}
	content, readErr := os.ReadFile(filepath.Join(oldestPath, "app.txt"))
	if readErr != nil || string(content) != "must survive\n" {
		t.Fatalf("failed archive removed or changed local worktree: content=%q err=%v", content, readErr)
	}
}

func TestManagerRemovesWholeMultiRepositoryBundle(t *testing.T) {
	firstRepo := makeTaskRepo(t)
	secondRepo := makeTaskRepo(t)
	secondRepo.ID = "repo-b"
	secondRepo.RelativePath = "backend"
	addTaskRemote(t, firstRepo)
	addTaskRemote(t, secondRepo)
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)
	manager.SetMaxWorktrees(2)
	old, err := manager.EnsureLocal(context.Background(), PrepareRequest{
		TaskID: "task-old", WorkspaceKey: "workspace", Repositories: []repository.Checkout{firstRepo, secondRepo},
	}, "agent-old", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Release("workspace", "task-old", "agent-old"); err != nil {
		t.Fatal(err)
	}
	ensureAndReleaseTask(t, manager, firstRepo, "task-new")

	for _, binding := range old.RepositoryBindings {
		if _, err := os.Stat(binding.LocalPath); !os.IsNotExist(err) {
			t.Fatalf("multi-repository bundle left worktree %q: %v", binding.LocalPath, err)
		}
	}
	if _, found, err := manager.Store.Get("workspace", "task-old"); err != nil || found {
		t.Fatalf("multi-repository bundle metadata remains: found=%v err=%v", found, err)
	}
}

func TestManagerReconcilesExistingInstallToDefaultLimit(t *testing.T) {
	repo := makeTaskRepo(t)
	addTaskRemote(t, repo)
	manager := NewManager(
		NewStore(filepath.Join(t.TempDir(), "state")),
		NewLocalWorktreeProvider(filepath.Join(t.TempDir(), "worktrees")),
	)
	manager.SetMaxWorktrees(5)
	oldest := ensureAndReleaseTask(t, manager, repo, "legacy-task-1")
	ensureAndReleaseTask(t, manager, repo, "legacy-task-2")
	ensureAndReleaseTask(t, manager, repo, "legacy-task-3")
	ensureAndReleaseTask(t, manager, repo, "legacy-task-4")

	manager.SetMaxWorktrees(DefaultMaxWorktrees)
	if err := manager.ReconcileLimit(context.Background(), "workspace"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(oldest.RepositoryBindings[0].LocalPath); !os.IsNotExist(err) {
		t.Fatalf("upgrade reconcile did not remove oldest legacy worktree: %v", err)
	}
	bundles, err := manager.Store.ListWorkspace("workspace")
	if err != nil {
		t.Fatal(err)
	}
	if len(bundles) != DefaultMaxWorktrees {
		t.Fatalf("upgrade reconcile left %d bundles want %d", len(bundles), DefaultMaxWorktrees)
	}
}
