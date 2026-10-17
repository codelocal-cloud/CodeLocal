package taskexecution

import (
	"context"
	"errors"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"
)

const (
	MinMaxWorktrees = 1
	MaxMaxWorktrees = 20
)

type archivedWorktree struct {
	binding  RepositoryBinding
	repoRoot string
}

func NormalizeMaxWorktrees(value int) int {
	if value < MinMaxWorktrees || value > MaxMaxWorktrees {
		return DefaultMaxWorktrees
	}
	return value
}

func ParseMaxWorktrees(value string) int {
	parsed, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil {
		return DefaultMaxWorktrees
	}
	return NormalizeMaxWorktrees(parsed)
}

func (m *Manager) SetMaxWorktrees(value int) {
	m.retentionMu.Lock()
	m.maxWorktrees = NormalizeMaxWorktrees(value)
	m.retentionMu.Unlock()
}

func (m *Manager) MaxWorktrees() int {
	m.retentionMu.Lock()
	defer m.retentionMu.Unlock()
	return NormalizeMaxWorktrees(m.maxWorktrees)
}

func (m *Manager) SetWorktreeInUse(check func(string) bool) {
	m.retentionMu.Lock()
	m.worktreeInUse = check
	m.retentionMu.Unlock()
}

func (m *Manager) bundleIsRunning(bundle Bundle) bool {
	if m.worktreeInUse == nil {
		return false
	}
	for _, binding := range bundle.RepositoryBindings {
		if m.worktreeInUse(binding.LocalPath) {
			return true
		}
	}
	return false
}

func existingBindingCount(bundle Bundle) int {
	count := 0
	for _, binding := range bundle.RepositoryBindings {
		if info, err := os.Stat(binding.LocalPath); err == nil && info.IsDir() {
			count++
		}
	}
	return count
}

func bundleIsLeased(bundle Bundle, now time.Time) bool {
	return strings.TrimSpace(bundle.Lease.OwnerID) != "" && bundle.Lease.ExpiresAt.After(now)
}

func oldestBundles(bundles []Bundle) {
	sort.SliceStable(bundles, func(i, j int) bool {
		if bundles[i].CreatedAt.Equal(bundles[j].CreatedAt) {
			return bundles[i].TaskID < bundles[j].TaskID
		}
		return bundles[i].CreatedAt.Before(bundles[j].CreatedAt)
	})
}

func (m *Manager) ReconcileLimit(ctx context.Context, workspaceKey string) error {
	m.retentionMu.Lock()
	defer m.retentionMu.Unlock()
	return m.enforceWorktreeLimit(ctx, workspaceKey, "", 0)
}

func (m *Manager) enforceWorktreeLimit(ctx context.Context, workspaceKey, protectedTaskID string, requiredSlots int) error {
	limit := NormalizeMaxWorktrees(m.maxWorktrees)
	if requiredSlots > limit {
		return fmt.Errorf("%w: task needs %d worktrees but limit is %d", ErrWorktreeLimitReached, requiredSlots, limit)
	}
	bundles, err := m.Store.ListWorkspace(workspaceKey)
	if err != nil {
		return err
	}
	current := 0
	for _, bundle := range bundles {
		if bundle.Provider == ProviderLocalWorktree {
			current += existingBindingCount(bundle)
		}
	}
	target := limit - requiredSlots
	if current <= target {
		return nil
	}
	oldestBundles(bundles)
	var lastErr error
	for _, bundle := range bundles {
		if current <= target {
			break
		}
		count := existingBindingCount(bundle)
		if bundle.Provider != ProviderLocalWorktree || bundle.TaskID == protectedTaskID {
			continue
		}
		if count == 0 {
			if err := m.Store.Delete(workspaceKey, bundle.TaskID); err != nil {
				lastErr = err
			}
			continue
		}
		if bundleIsLeased(bundle, time.Now().UTC()) || m.bundleIsRunning(bundle) {
			continue
		}
		if err := m.archiveAndRemoveBundle(ctx, bundle); err != nil {
			lastErr = err
			continue
		}
		if err := m.Store.Delete(workspaceKey, bundle.TaskID); err != nil {
			return err
		}
		current -= count
	}
	if current > target {
		if lastErr != nil {
			return fmt.Errorf("%w: current=%d limit=%d required=%d: %v", ErrWorktreeLimitReached, current, limit, requiredSlots, lastErr)
		}
		return fmt.Errorf("%w: current=%d limit=%d required=%d", ErrWorktreeLimitReached, current, limit, requiredSlots)
	}
	return nil
}

func archiveBranch(taskID string) string {
	return "codelocal/archive/" + digestKey("archive", taskID)[:12]
}

func mainWorktreeRoot(ctx context.Context, worktreePath string) (string, error) {
	out, err := gitCommand(ctx, worktreePath, "worktree", "list", "--porcelain")
	if err != nil {
		return "", err
	}
	for _, line := range strings.Split(out, "\n") {
		if strings.HasPrefix(line, "worktree ") {
			return strings.TrimSpace(strings.TrimPrefix(line, "worktree ")), nil
		}
	}
	return "", errors.New("authoritative worktree is unavailable")
}

func (p *LocalWorktreeProvider) archive(ctx context.Context, taskID string, binding RepositoryBinding) (archivedWorktree, error) {
	if _, err := os.Stat(binding.LocalPath); os.IsNotExist(err) {
		return archivedWorktree{binding: binding}, nil
	} else if err != nil {
		return archivedWorktree{}, err
	}
	if strings.TrimSpace(binding.BranchName) == "" {
		return archivedWorktree{}, errors.New("worktree branch is unknown")
	}
	repoRoot, err := mainWorktreeRoot(ctx, binding.LocalPath)
	if err != nil {
		return archivedWorktree{}, err
	}
	status, err := gitCommand(ctx, binding.LocalPath, "status", "--porcelain")
	if err != nil {
		return archivedWorktree{}, err
	}
	if strings.TrimSpace(status) != "" {
		if _, err := gitCommand(ctx, binding.LocalPath, "add", "-A"); err != nil {
			return archivedWorktree{}, err
		}
		message := "chore(codelocal): checkpoint task " + strings.TrimSpace(taskID)
		if _, err := gitCommand(ctx, binding.LocalPath,
			"-c", "user.name=CodeLocal", "-c", "user.email=codelocal@localhost",
			"commit", "-m", message); err != nil {
			return archivedWorktree{}, err
		}
	}
	if _, err := gitCommand(ctx, binding.LocalPath, "remote", "get-url", "origin"); err != nil {
		return archivedWorktree{}, errors.New("origin remote is required to archive an old worktree")
	}
	head, err := gitCommand(ctx, binding.LocalPath, "rev-parse", "HEAD")
	if err != nil {
		return archivedWorktree{}, err
	}
	remoteRef := "refs/heads/" + archiveBranch(taskID)
	if _, err := gitCommand(ctx, binding.LocalPath, "push", "origin", "HEAD:"+remoteRef); err != nil {
		return archivedWorktree{}, err
	}
	remote, err := gitCommand(ctx, binding.LocalPath, "ls-remote", "--heads", "origin", remoteRef)
	if err != nil {
		return archivedWorktree{}, err
	}
	fields := strings.Fields(remote)
	if len(fields) < 2 || fields[0] != head || fields[1] != remoteRef {
		return archivedWorktree{}, errors.New("remote archive verification failed")
	}
	return archivedWorktree{binding: binding, repoRoot: repoRoot}, nil
}

func (p *LocalWorktreeProvider) removeArchived(ctx context.Context, archived archivedWorktree) error {
	if archived.repoRoot == "" {
		return nil
	}
	if _, err := gitCommand(ctx, archived.repoRoot, "worktree", "remove", "--force", archived.binding.LocalPath); err != nil {
		return err
	}
	if archived.binding.BranchName != "" && gitCommandOK(ctx, archived.repoRoot, "show-ref", "--verify", "--quiet", "refs/heads/"+archived.binding.BranchName) {
		if _, err := gitCommand(ctx, archived.repoRoot, "branch", "-D", archived.binding.BranchName); err != nil {
			return err
		}
	}
	_, err := gitCommand(ctx, archived.repoRoot, "worktree", "prune")
	return err
}

func (m *Manager) archiveAndRemoveBundle(ctx context.Context, bundle Bundle) error {
	archived := make([]archivedWorktree, 0, len(bundle.RepositoryBindings))
	for _, binding := range bundle.RepositoryBindings {
		item, err := m.Worktrees.archive(ctx, bundle.TaskID, binding)
		if err != nil {
			return err
		}
		archived = append(archived, item)
	}
	for _, item := range archived {
		if err := m.Worktrees.removeArchived(ctx, item); err != nil {
			return err
		}
	}
	return nil
}
