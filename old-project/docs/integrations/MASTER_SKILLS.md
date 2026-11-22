# CodeLocal Master Skills — agent development kit

## Provenance and scope

- Source: https://github.com/0xmarkhydra/master-skill
- Pinned upstream commit: `a0d9aa0301e6ac5b2b06bb6cd3c348930337fdda` (2026-10-18 installation).
- Installed with upstream `tools/install.py --agent both` after upstream verification and its installer tests.
- Installed for **repository coding agents** (Codex and Claude Code), not automatically registered as skills in the CodeLocal product's cloud marketplace or end-user runtime.

## Installed skill categories

The following nine skills are mirrored into both `.agents/skills/` and `.claude/skills/`:

1. `backend-engineering`: backend/API/database/security.
2. `react-frontend-engineering`: React/Next.js feature parity and UX behavior.
3. `ios-inspired-ui-design`: design system, responsive UI, visual QA.
4. `ponytail`: minimal-complexity decision policy.
5. `ponytail-review`: change review.
6. `ponytail-audit`: codebase audit.
7. `ponytail-debt`: technical-debt tracking.
8. `ponytail-gain`: published upstream benchmarking context.
9. `ponytail-help`: workflow guidance.

The installer also added managed instructions to `AGENTS.md` and `CLAUDE.md`. Project-specific security, architectural, and test policies have precedence over brevity.

Ponytail is MIT-licensed and retains its license and upstream attribution under `.codelocal-skills/ponytail/`.

## Verification and safe updates

The installation was verified with the upstream kit's `tools/verify_kit.py`, its Python unit tests, a no-write dry run, and an idempotent second install. Relevant CodeLocal Go skill tests were run separately.

To inspect a future upstream release:

```sh
git clone https://github.com/0xmarkhydra/master-skill.git /tmp/codelocal-master-skill
python3 /tmp/codelocal-master-skill/tools/verify_kit.py
python3 /tmp/codelocal-master-skill/tools/install.py --target . --agent both --dry-run
```

Run those commands from the CodeLocal repository root. The installer **intentionally refuses to overwrite changed skills or a modified managed block**. For upgrades, inspect upstream diffs and license changes, then reconcile both agent mirrors and the managed policy deliberately. Record the new pinned commit in this document. Never run hooks, downloaded binaries or unreviewed third-party scripts as part of the skill activation.

## Agent behavior

Follow the full repository policy in `AGENTS.md`. Read only the skill files relevant to the current task (usually the relevant engineering/design skill plus `ponytail`). Skill text is guidance rather than evidence that builds, tests, deployments, or MCP actions succeeded.
