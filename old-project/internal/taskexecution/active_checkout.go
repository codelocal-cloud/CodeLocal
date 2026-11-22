package taskexecution

import (
	"context"
	"strings"
)

// ActiveCheckoutProvider binds task execution directly to the authoritative
// checkout opened by the user. It never creates, resets, stashes, or removes
// worktrees; existing edit, approval, and verification policies still govern
// every operation performed inside that checkout.
type ActiveCheckoutProvider struct{}

func NewActiveCheckoutProvider() *ActiveCheckoutProvider { return &ActiveCheckoutProvider{} }

func activeCheckoutBranch(ctx context.Context, root string) string {
	branch, err := gitCommand(ctx, root, "symbolic-ref", "--quiet", "--short", "HEAD")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(branch)
}

func (p *ActiveCheckoutProvider) Prepare(ctx context.Context, req PrepareRequest) (Bundle, error) {
	if err := validatePrepareRequest(req); err != nil {
		return Bundle{}, err
	}
	bundle := newBundle(req)
	bundle.Provider = ProviderActiveCheckout
	for _, repo := range req.Repositories {
		source, err := sourceState(ctx, repo)
		if err != nil {
			return Bundle{}, err
		}
		bundle.RepositoryBindings = append(bundle.RepositoryBindings, repositoryBinding(
			req,
			repo,
			activeCheckoutBranch(ctx, repo.Root),
			source,
			repo.Root,
		))
	}
	bundle.State = StateReady
	return bundle, nil
}
