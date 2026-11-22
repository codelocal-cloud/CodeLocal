package taskexecution

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/repository"
)

var (
	ErrExecutionProviderMismatch = errors.New("existing task execution bundle uses a different provider")
	ErrRepositoryCoverageChanged = errors.New("existing task execution bundle has different repository coverage")
	ErrBindingUnavailable        = errors.New("existing task execution binding is unavailable")
	ErrActiveCheckoutBusy        = errors.New("active checkout is already leased by another live task")
	ErrWorktreeLimitReached      = errors.New("worktree limit reached and no older task could be archived")
)

const (
	DefaultMaxWorktrees        = 3
	RuntimeSettingMaxWorktrees = "CODELOCAL_MAX_WORKTREES"
)

type activeCheckoutLease struct {
	workspaceKey string
	taskID       string
	ownerID      string
	expiresAt    time.Time
}

var activeCheckoutLeases = struct {
	sync.Mutex
	items map[string]activeCheckoutLease
}{items: map[string]activeCheckoutLease{}}

type Manager struct {
	Store         *Store
	Worktrees     *LocalWorktreeProvider
	Active        *ActiveCheckoutProvider
	retentionMu   sync.Mutex
	maxWorktrees  int
	worktreeInUse func(string) bool
}

func NewManager(store *Store, worktrees *LocalWorktreeProvider) *Manager {
	if store == nil {
		store = NewStore("")
	}
	if worktrees == nil {
		worktrees = NewLocalWorktreeProvider("")
	}
	return &Manager{Store: store, Worktrees: worktrees, Active: NewActiveCheckoutProvider(), maxWorktrees: DefaultMaxWorktrees}
}

type providerPreparer interface {
	Prepare(context.Context, PrepareRequest) (Bundle, error)
}

func (m *Manager) provider(provider Provider) (providerPreparer, error) {
	switch provider {
	case ProviderLocalWorktree:
		return m.Worktrees, nil
	case ProviderActiveCheckout:
		if m.Active == nil {
			m.Active = NewActiveCheckoutProvider()
		}
		return m.Active, nil
	default:
		return nil, ErrExecutionProviderMismatch
	}
}

func existingCoverage(bindings []RepositoryBinding) map[string]RepositoryBinding {
	out := map[string]RepositoryBinding{}
	for _, binding := range bindings {
		out[strings.TrimSpace(binding.RepositoryID)+"\x00"+strings.TrimSpace(binding.RepositoryPath)] = binding
	}
	return out
}

func missingRepositories(existing []RepositoryBinding, requested []repository.Checkout) ([]repository.Checkout, error) {
	coverage := existingCoverage(existing)
	byPath, byID := map[string]string{}, map[string]string{}
	for _, binding := range existing {
		byPath[strings.TrimSpace(binding.RepositoryPath)] = strings.TrimSpace(binding.RepositoryID)
		byID[strings.TrimSpace(binding.RepositoryID)] = strings.TrimSpace(binding.RepositoryPath)
	}
	missing := []repository.Checkout{}
	for _, repo := range requested {
		id, path := strings.TrimSpace(repo.ID), strings.TrimSpace(repo.RelativePath)
		if _, ok := coverage[id+"\x00"+path]; ok {
			continue
		}
		if knownID, ok := byPath[path]; ok && knownID != id {
			return nil, ErrRepositoryCoverageChanged
		}
		if knownPath, ok := byID[id]; ok && knownPath != path {
			return nil, ErrRepositoryCoverageChanged
		}
		missing = append(missing, repo)
	}
	return missing, nil
}

func validateBindings(ctx context.Context, bundle Bundle) error {
	for _, binding := range bundle.RepositoryBindings {
		info, err := os.Stat(binding.LocalPath)
		if err != nil || !info.IsDir() {
			return ErrBindingUnavailable
		}
		if !gitCommandOK(ctx, binding.LocalPath, "rev-parse", "--is-inside-work-tree") {
			return ErrBindingUnavailable
		}
	}
	return nil
}

func activeCheckoutKey(path string) string {
	path = strings.TrimSpace(path)
	if path == "" {
		return ""
	}
	if resolved, err := filepath.EvalSymlinks(path); err == nil {
		path = resolved
	}
	if absolute, err := filepath.Abs(path); err == nil {
		path = absolute
	}
	return filepath.Clean(path)
}

func activeCheckoutPathsFromRequest(req PrepareRequest) []string {
	out := make([]string, 0, len(req.Repositories))
	seen := map[string]struct{}{}
	for _, repo := range req.Repositories {
		key := activeCheckoutKey(repo.Root)
		if key == "" {
			continue
		}
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, key)
	}
	return out
}

func activeCheckoutPathsFromBundle(bundle Bundle) []string {
	out := make([]string, 0, len(bundle.RepositoryBindings))
	seen := map[string]struct{}{}
	for _, binding := range bundle.RepositoryBindings {
		key := activeCheckoutKey(binding.LocalPath)
		if key == "" {
			continue
		}
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, key)
	}
	return out
}

func acquireActiveCheckoutLease(paths []string, workspaceKey, taskID, ownerID string, ttl time.Duration) error {
	if len(paths) == 0 {
		return nil
	}
	workspaceKey = strings.TrimSpace(workspaceKey)
	taskID = strings.TrimSpace(taskID)
	ownerID = strings.TrimSpace(ownerID)
	if workspaceKey == "" || taskID == "" || ownerID == "" {
		return errors.New("workspace, task, and owner are required for live checkout lease")
	}
	if ttl <= 0 {
		ttl = 15 * time.Minute
	}
	now := time.Now().UTC()
	expiresAt := now.Add(ttl)
	activeCheckoutLeases.Lock()
	defer activeCheckoutLeases.Unlock()
	for key, lease := range activeCheckoutLeases.items {
		if !lease.expiresAt.After(now) {
			delete(activeCheckoutLeases.items, key)
		}
	}
	for _, path := range paths {
		lease, ok := activeCheckoutLeases.items[path]
		if !ok {
			continue
		}
		if lease.workspaceKey == workspaceKey && lease.taskID == taskID && lease.ownerID == ownerID {
			continue
		}
		return ErrActiveCheckoutBusy
	}
	for _, path := range paths {
		activeCheckoutLeases.items[path] = activeCheckoutLease{
			workspaceKey: workspaceKey,
			taskID:       taskID,
			ownerID:      ownerID,
			expiresAt:    expiresAt,
		}
	}
	return nil
}

func releaseActiveCheckoutLease(workspaceKey, taskID, ownerID string) {
	activeCheckoutLeases.Lock()
	defer activeCheckoutLeases.Unlock()
	for key, lease := range activeCheckoutLeases.items {
		if lease.workspaceKey == workspaceKey && lease.taskID == taskID && lease.ownerID == ownerID {
			delete(activeCheckoutLeases.items, key)
		}
	}
}

func (m *Manager) EnsureLocal(ctx context.Context, req PrepareRequest, ownerID string, leaseTTL time.Duration) (Bundle, error) {
	return m.Ensure(ctx, req, ProviderLocalWorktree, ownerID, leaseTTL)
}

func (m *Manager) Ensure(ctx context.Context, req PrepareRequest, requested Provider, ownerID string, leaseTTL time.Duration) (Bundle, error) {
	m.retentionMu.Lock()
	defer m.retentionMu.Unlock()
	existing, ok, err := m.Store.Get(req.WorkspaceKey, req.TaskID)
	if err != nil {
		return Bundle{}, err
	}
	if !ok {
		if requested == ProviderLocalWorktree {
			if err := m.enforceWorktreeLimit(ctx, req.WorkspaceKey, req.TaskID, len(req.Repositories)); err != nil {
				return Bundle{}, err
			}
		}
		activeLeaseHeld := requested == ProviderActiveCheckout
		if activeLeaseHeld {
			if err := acquireActiveCheckoutLease(activeCheckoutPathsFromRequest(req), req.WorkspaceKey, req.TaskID, ownerID, leaseTTL); err != nil {
				return Bundle{}, err
			}
		}
		preparer, err := m.provider(requested)
		if err != nil {
			if activeLeaseHeld {
				releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
			}
			return Bundle{}, err
		}
		bundle, err := preparer.Prepare(ctx, req)
		if err != nil {
			if activeLeaseHeld {
				releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
			}
			return Bundle{}, err
		}
		bundle, err = m.Store.Put(bundle)
		if err != nil {
			if activeLeaseHeld {
				releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
			}
			return Bundle{}, err
		}
		claimed, err := m.Store.Claim(req.WorkspaceKey, req.TaskID, ownerID, leaseTTL)
		if err != nil && activeLeaseHeld {
			releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
		}
		return claimed, err
	}
	if err := validateBindings(ctx, existing); err != nil {
		return Bundle{}, err
	}
	activeLeaseHeld := existing.Provider == ProviderActiveCheckout
	if activeLeaseHeld {
		if err := acquireActiveCheckoutLease(activeCheckoutPathsFromBundle(existing), req.WorkspaceKey, req.TaskID, ownerID, leaseTTL); err != nil {
			return Bundle{}, err
		}
	}
	missing, err := missingRepositories(existing.RepositoryBindings, req.Repositories)
	if err != nil {
		if activeLeaseHeld {
			releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
		}
		return Bundle{}, err
	}
	claimed, err := m.Store.Claim(req.WorkspaceKey, req.TaskID, ownerID, leaseTTL)
	if err != nil {
		if activeLeaseHeld {
			releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
		}
		return Bundle{}, err
	}
	if len(missing) == 0 {
		return claimed, nil
	}
	if existing.Provider == ProviderLocalWorktree {
		if err := m.enforceWorktreeLimit(ctx, req.WorkspaceKey, req.TaskID, len(missing)); err != nil {
			_, _ = m.Store.Release(req.WorkspaceKey, req.TaskID, ownerID)
			return Bundle{}, err
		}
	}
	preparer, err := m.provider(existing.Provider)
	if err != nil {
		_, _ = m.Store.Release(req.WorkspaceKey, req.TaskID, ownerID)
		if activeLeaseHeld {
			releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
		}
		return Bundle{}, err
	}
	expansionReq := req
	expansionReq.Repositories = missing
	if activeLeaseHeld {
		if err := acquireActiveCheckoutLease(activeCheckoutPathsFromRequest(expansionReq), req.WorkspaceKey, req.TaskID, ownerID, leaseTTL); err != nil {
			_, _ = m.Store.Release(req.WorkspaceKey, req.TaskID, ownerID)
			releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
			return Bundle{}, err
		}
	}
	expansion, err := preparer.Prepare(ctx, expansionReq)
	if err != nil {
		_, _ = m.Store.Release(req.WorkspaceKey, req.TaskID, ownerID)
		if activeLeaseHeld {
			releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
		}
		return Bundle{}, err
	}
	claimed.RepositoryBindings = append(claimed.RepositoryBindings, expansion.RepositoryBindings...)
	updated, err := m.Store.Put(claimed)
	if err != nil && activeLeaseHeld {
		releaseActiveCheckoutLease(req.WorkspaceKey, req.TaskID, ownerID)
	}
	return updated, err
}

func (m *Manager) SetState(workspaceKey, taskID, ownerID string, next State) (Bundle, error) {
	bundle, ok, err := m.Store.Get(workspaceKey, taskID)
	if err != nil {
		return Bundle{}, err
	}
	if !ok {
		return Bundle{}, os.ErrNotExist
	}
	if bundle.Lease.OwnerID != "" && strings.TrimSpace(bundle.Lease.OwnerID) != strings.TrimSpace(ownerID) {
		return Bundle{}, ErrLeaseHeld
	}
	bundle.State = next
	return m.Store.Put(bundle)
}

func (m *Manager) Release(workspaceKey, taskID, ownerID string) (Bundle, error) {
	released, err := m.Store.Release(workspaceKey, taskID, ownerID)
	if err == nil && released.Provider == ProviderActiveCheckout {
		releaseActiveCheckoutLease(workspaceKey, taskID, ownerID)
	}
	return released, err
}
