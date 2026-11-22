package cloudserver

import "testing"

func TestNextDashboardRoutingIncludesSkillsForumsAndSingleTrailingSlash(t *testing.T) {
	for _, path := range []string{
		"/dashboard/skills",
		"/dashboard/skills/",
		"/dashboard/forums",
		"/dashboard/forums/",
		"/dashboard/forums/topic-123",
		"/dashboard/forums/topic-123/",
		"/dashboard/admin",
		"/dashboard/admin/",
	} {
		if !isNextDashboardPath(path) {
			t.Fatalf("expected %q to use Next dashboard presentation", path)
		}
	}
}

func TestPlaneMCPApprovalPageRequiresExactOwnerScopedRoute(t *testing.T) {
	id := "abb5aadbdca67680dbe6060b080044d04afe07a3a412eb5c"
	for _, path := range []string{
		"/dashboard/plugins/approve/" + id,
		"/dashboard/plugins/approve/" + id + "/",
	} {
		if !isNextDashboardPath(path) {
			t.Fatalf("expected %q to reach authenticated Next dashboard", path)
		}
	}
	for _, path := range []string{
		"/dashboard/plugins/approve",
		"/dashboard/plugins/approve/",
		"/dashboard/plugins/approve/other",
		"/dashboard/plugins/approve/" + id + "/execute",
		"/dashboard/plugins/approve/" + id + "//",
		"/dashboard/plugins/approve/" + id[:47],
		"/dashboard/plugins/approve/" + id[:47] + "G",
		"/dashboard/plugins/approve/" + id + "ff",
		"/dashboard/plugins/approve/" + id + "?evil",
	} {
		if isNextDashboardPath(path) {
			t.Fatalf("path %q must not be routed to the approval UI", path)
		}
	}
}

func TestNextDashboardRoutingDoesNotWidenToNestedOrDoubleSlashPaths(t *testing.T) {
	for _, path := range []string{
		"/dashboard/admin/users",
		"/dashboard/admin//",
		"/dashboard/skills/private",
		"/dashboard/forums//",
	} {
		if isNextDashboardPath(path) {
			t.Fatalf("expected %q to remain outside the Next dashboard allowlist", path)
		}
	}
}

func TestAdminAuthorizationUsesCanonicalPresentationPath(t *testing.T) {
	if got := canonicalNextPresentationPath("/dashboard/admin/"); got != "/dashboard/admin" {
		t.Fatalf("expected trailing-slash admin path to canonicalize for authorization, got %q", got)
	}
	if got := canonicalNextPresentationPath("/dashboard/admin//"); got != "/dashboard/admin//" {
		t.Fatalf("double-slash path must not be normalized into an authorized admin route, got %q", got)
	}
}

func TestNextPublicRoutingIncludesForumsFamily(t *testing.T) {
	for _, path := range []string{"/forums", "/forums/", "/forums/forum_123", "/forums/forum_123/"} {
		if !isNextPublicPagePath(path) {
			t.Fatalf("expected %q to use Next public presentation", path)
		}
	}
}
