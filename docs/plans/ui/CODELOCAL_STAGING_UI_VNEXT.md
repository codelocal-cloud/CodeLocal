# CodeLocal UI VNext — staging workstream

Date: 2026-10-18
Branch: `staging` (existing npm beta branch, synchronized with `main` without discarding prior Jev/beta commits)
Status: Phase 1 implementation in progress

## Business analysis and boundaries

**Outcome:** Make CodeLocal's browser dashboard feel like a polished AI workspace: fast navigation, clear connection status, predictable actions and consistent compact layout. The referenced 9BizClaw UI is inspiration for information architecture and density, **not** a source of copied product assets or visual identity.

**Users and scenarios**
1. Returning developer: switch quickly between Chat, Projects, MCP, Plugins and Skills without scanning a long menu.
2. Integrator: find an installed Plugin, inspect real connection status, then configure/manage it through the existing authorization and API flow.
3. Mobile user: open the dashboard drawer, navigate by touch, escape/close the drawer and reach account controls without content clipping.

**Hard constraints:** Keep every route, backend API, CSRF requirement, installation, OAuth, disconnect and account behavior. Real statuses only; no fake connected metrics. Preserve current Go/Next.js and CodeLocal skill architecture. No new UI dependency, no changes to public marketing identity during the first phase.

## Phased checklist

| Phase | Delivery | Acceptance |
| --- | --- | --- |
| 0. Foundation | Inspect existing staging history, merge `main` without history loss, bring in Master Skills | Git merge clean, Jev and npm beta workflow preserved |
| 1. Shell | Two-tier desktop navigation (quick icon rail + fully labeled menu), original CodeLocal charcoal/mint dashboard tokens | All existing navigation links remain reachable; active route visible; mobile drawer unchanged behavior |
| 1. Plugin catalog | More legible app-like cards, clearer hierarchy and search controls | Real install/manage/status flows and error/retry unchanged, no fabricated integrations |
| 1. QA | Lint, typecheck, UI unit tests, build, production npm dependency audit | No regression; screenshots and manual responsive checks tracked separately |
| 2. MCP Connections | Better connection grouping for runtime/client settings and credentials | Preserve device trust, auth, reconnection and errors; compare against current route |
| 3. Skills, Settings, Workspace | Reuse the shared shell/tokens and unify form fields, state and density | Existing workflow parity + focus/overflow checks |
| 4. Chat + responsive polish | Align chat composer and side panels with common design system | Message send, history, stream/error, mobile keyboard tests |
| 5. Accessibility + release | Browser matrix 390/768/1024/1440, 200% zoom, keyboard, reduced motion and screen-reader navigation | Signoff only with observed evidence before any production merge |

## Phase 1 details

- Desktop quick rail gives access to Overview, Chat, Projects, MCP Connections, Plugins, Skills and Settings. The longer panel retains **all** primary, secondary and admin/account actions.
- Mobile keeps a single drawer, overlay, Escape, focus management and safe-area padding. The icon rail disappears at the mobile breakpoint.
- Dashboard tokens MUST inherit the original CodeLocal brand from `web/src/app/globals.css`: `--brand-canvas: #0b0d0e`, `--surface: #15191a`, `--surface-raised: #202627`, `--brand-accent: #b6f3d3`, `--accent-strong: #8ce2b8` and secondary cyan `#7fc7de`. iOS-inspired visual design means material and structure, not swapping the established mint/charcoal brand for violet. Connection success stays semantic green; errors stay red.
- Plugin cards are styled but use existing real `PluginItem` / `PluginConnection` data and existing mutation handlers.

## Verification and release safety

- Required web checks from `web/AGENTS.md`: `npm run lint`, `npm run typecheck`, `npm run build`, `npm audit --omit=dev`; also `npm run test:ui` for source-level regression.
- Browser visual QA is **NOT RUN** until actually observed in a browser at each target viewport; source-level tests are not a substitute.
- `staging` already publishes an npm **beta** artifact on workflow triggers. Check workflow effects and CI before pushing. Do not merge to `main` or ship production without approval and verification.
