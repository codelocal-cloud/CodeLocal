@AGENTS.md

<!-- CODELOCAL-SKILLS:BEGIN -->
## CodeLocal Skills — coding agent instructions

Before a coding task, inspect the repository, existing instructions, project stack,
Git state, affected workflows and contracts. Read the applicable skill(s), not all
nine indiscriminately:

- Server/API/DB/auth/security/migrations: .claude/skills/backend-engineering/SKILL.md and relevant references.
- React/Vite/Next.js frontend behavior, state/forms/API/routing/feature parity:
  .claude/skills/react-frontend-engineering/SKILL.md and relevant references.
- Design system, iOS-inspired UI, responsive, visual quality:
  .claude/skills/ios-inspired-ui-design/SKILL.md and relevant references.
- Any code creation/fix/refactor: additionally read .claude/skills/ponytail/SKILL.md.
  Apply Ponytail **Ultra** as a simplicity *decision policy*, not a plugin mode.
  Challenge unneeded complexity; reuse existing code/platform/dependencies first.
- On explicit review/audit/debt/benchmark/help requests, use
  .claude/skills/ponytail-review/SKILL.md, .claude/skills/ponytail-audit/SKILL.md,
  .claude/skills/ponytail-debt/SKILL.md, .claude/skills/ponytail-gain/SKILL.md, or
  .claude/skills/ponytail-help/SKILL.md as appropriate.

Precedence: project/user-approved scope, correctness, backend authorization, data
and API compatibility, feature parity, accessibility, tests and release safety
always beat code brevity (P0/P1 quality gates). Ponytail Ultra must NOT silently remove requirements,
validation, security checks, error handling or tests. Follow project branch/CI
policies. Report PASS/FAIL/NOT RUN/BLOCKED from evidence; do not invent success,
commit, push, merge, deployment or live testing.

This block was installed by CodeLocal Skills. Existing project instructions remain
in effect. Ponytail is third-party MIT-licensed; provenance is under
`.codelocal-skills/ponytail/`. No marketplace plugin or hooks are installed.
<!-- CODELOCAL-SKILLS:END -->
