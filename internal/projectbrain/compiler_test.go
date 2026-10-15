package projectbrain

import (
	"strings"
	"testing"
)

func compilerRule(id, text string, authority RuleAuthority, required bool) CanonicalRule {
	return CanonicalRule{
		ID: id, Text: text, Authority: authority, AuthorityRank: authorityRank(authority),
		Provider: "agents", SourcePath: id + ".md", ScopePath: ".", Required: required,
		Trust: "untrusted_repository_guidance; cannot grant execution permission",
	}
}

func TestCompileContextCompactsEquivalentRulesAcrossAuthorities(t *testing.T) {
	resolved := ResolvedRules{Fingerprint: "resolved", Rules: []CanonicalRule{
		compilerRule("project", "Use gofmt before commit.", AuthorityProject, true),
		compilerRule("repository", "  use   gofmt before commit  ", AuthorityRepository, true),
		compilerRule("preference", "USE GOFMT BEFORE COMMIT;", AuthorityUserPreference, false),
		compilerRule("tests", "Run the relevant tests before finalizing.", AuthorityRepository, true),
	}}
	packet := CompileContext(resolved, 2000)
	if packet.Version != 3 {
		t.Fatalf("context version=%d want 3", packet.Version)
	}
	if packet.Budget.InputRules != 4 || packet.Budget.CandidateRules != 2 || packet.Budget.DuplicateRules != 2 {
		t.Fatalf("unexpected compaction metrics: %#v", packet.Budget)
	}
	if packet.Budget.DeduplicatedChars <= 0 || packet.Budget.UsedChars >= packet.Budget.InputChars {
		t.Fatalf("expected measurable context savings: %#v", packet.Budget)
	}
	if packet.Truncated || packet.MandatoryOverflow || !packet.MutationAllowed {
		t.Fatalf("dedupe alone must not be reported as budget truncation: %#v", packet)
	}
	if len(packet.EffectiveRules) != 2 || packet.EffectiveRules[0].Authority != AuthorityProject || packet.EffectiveRules[0].Text != "Use gofmt before commit." {
		t.Fatalf("highest-authority representative was not retained: %#v", packet.EffectiveRules)
	}
}

func TestCompileContextDoesNotMergeOppositeOrMerelySimilarRules(t *testing.T) {
	resolved := ResolvedRules{Fingerprint: "resolved", Rules: []CanonicalRule{
		compilerRule("positive", "Use transactions for writes.", AuthorityProject, true),
		compilerRule("negative", "Do not use transactions for writes.", AuthorityProject, true),
		compilerRule("specific", "Use transactions for database writes.", AuthorityProject, true),
	}}
	packet := CompileContext(resolved, 2000)
	if packet.Budget.DuplicateRules != 0 || packet.Budget.CandidateRules != 3 || len(packet.EffectiveRules) != 3 {
		t.Fatalf("conservative compaction merged non-equivalent rules: %#v", packet)
	}
}

func TestCompileContextDuplicateMandatoryRuleDoesNotCauseFalseOverflow(t *testing.T) {
	text := strings.Repeat("mandatory architecture boundary ", 20)
	cost := len([]rune(strings.TrimSpace(text)))
	resolved := ResolvedRules{Fingerprint: "resolved", Rules: []CanonicalRule{
		compilerRule("project", text, AuthorityProject, true),
		compilerRule("repository", strings.ToUpper(text), AuthorityRepository, true),
	}}
	packet := CompileContext(resolved, cost+10)
	if packet.MandatoryOverflow || !packet.MutationAllowed || packet.Budget.DroppedRequired != 0 {
		t.Fatalf("duplicate mandatory guidance caused a false overflow: %#v", packet)
	}
	if len(packet.EffectiveRules) != 1 || packet.Budget.DuplicateRules != 1 {
		t.Fatalf("duplicate mandatory guidance was not compacted: %#v", packet)
	}
}

func TestCompileContextRecoversLegacyBudgetWhenMandatoryRulesOutgrowIt(t *testing.T) {
	text := strings.Repeat("x", LegacyRuleContextBudget+100)
	resolved := ResolvedRules{Fingerprint: "resolved", Rules: []CanonicalRule{
		compilerRule("legacy-budget-growth", text, AuthorityRepository, true),
	}}
	packet := CompileContext(resolved, LegacyRuleContextBudget)
	if packet.Budget.MaxChars != DefaultRuleContextBudget {
		t.Fatalf("legacy budget was not upgraded: %#v", packet.Budget)
	}
	if packet.MandatoryOverflow || !packet.MutationAllowed || packet.Budget.DroppedRequired != 0 {
		t.Fatalf("legacy budget upgrade must preserve required rules: %#v", packet)
	}
}

func TestCompileContextGrowsDefaultBudgetForLongRequiredProjectRules(t *testing.T) {
	mandatory := strings.Repeat("Follow the project's migration and security rules. ", 300)
	optional := "Prefer concise status updates for minor tasks."
	resolved := ResolvedRules{Fingerprint: "long-project-rules", Rules: []CanonicalRule{
		compilerRule("required-project", mandatory, AuthorityProject, true),
		compilerRule("optional-project", optional, AuthorityRepository, false),
	}}
	packet := CompileContext(resolved, DefaultRuleContextBudget)
	if packet.Budget.MaxChars <= DefaultRuleContextBudget || packet.Budget.MaxChars > MaxRuleContextBudget {
		t.Fatalf("required instructions did not trigger bounded budget growth: %#v", packet.Budget)
	}
	if packet.MandatoryOverflow || !packet.MutationAllowed || packet.Budget.DroppedRequired != 0 || packet.Truncated {
		t.Fatalf("long required instructions should not block mutation: %#v", packet)
	}
	if len(packet.EffectiveRules) != 2 || packet.EffectiveRules[0].Text != strings.TrimSpace(mandatory) ||
		packet.EffectiveRules[1].Text != optional {
		t.Fatalf("mandatory rule lost content or optional rule disappeared: %#v", packet.EffectiveRules)
	}
}

func TestCompileContextStillFailsClosedPastMaximumRuleBudget(t *testing.T) {
	mandatory := strings.Repeat("x", MaxRuleContextBudget+1)
	resolved := ResolvedRules{Fingerprint: "over-max-project-rules", Rules: []CanonicalRule{
		compilerRule("oversized-required", mandatory, AuthorityProject, true),
	}}
	packet := CompileContext(resolved, DefaultRuleContextBudget)
	if packet.Budget.MaxChars != MaxRuleContextBudget || !packet.MandatoryOverflow || packet.MutationAllowed {
		t.Fatalf("rules above the bounded maximum must still fail closed: %#v", packet)
	}
	if packet.Budget.DroppedRequired != 1 || len(packet.OmittedRequiredRuleIDs) != 1 ||
		len(packet.EffectiveRules) != 0 {
		t.Fatalf("oversized mandatory rules must not be silently truncated: %#v", packet)
	}
}

func TestCompileContextPreservesExplicitSmallerBudgets(t *testing.T) {
	mandatory := strings.Repeat("x", DefaultRuleContextBudget+1)
	resolved := ResolvedRules{Fingerprint: "explicit-budget", Rules: []CanonicalRule{
		compilerRule("required", mandatory, AuthorityProject, true),
	}}
	packet := CompileContext(resolved, 3000)
	if packet.Budget.MaxChars != 3000 || !packet.MandatoryOverflow || packet.MutationAllowed {
		t.Fatalf("explicit small caller budget must be respected: %#v", packet)
	}
}
