# iOS UI Style Rulebook v1.0 — APPROVED

Scope: React, Next.js, Vite, Tailwind and compatible UI stacks; inspired by iOS and KPI-BSC V3 but product-brand neutral. **10 approved rule groups**. Baseline: Premium Balanced Glass, Adaptive Density, Smart Visual Quality.

**P0:** Functional integrity, visible/accessibly reachable required actions, no destructive layout clipping, truthful evidence. **P1:** Semantic tokens, consistent responsive/typography/components, accessibility and risk-appropriate visual QA. **P2:** Advanced effects, reflections, choreography and visual optimizations only where useful; P2 must not erode P0/P1.

## #01 iOS-inspired Design Philosophy (P0/P1)
**Clarity:** obvious information and primary action. **Depth:** restrained layers via surfaces, borders, shadows and selected glass. **Consistency:** reuse tokens and component variants. **Compactness:** high information utility without tiny typography. **Responsiveness:** adapt information architecture rather than shrinking desktop. **Functional Integrity:** do not remove fields/actions/routes to improve appearance. Apple influence is visual language, not a license to copy proprietary assets or enforce one brand.

## #02 Semantic Design Tokens & Adaptive Density (P1)
One canonical project design token source: background/foreground/surface/elevated/primary, state colors, border/focus, spacing, radius, typography, elevation, glass, layer order, motion and density. Respect existing brand and themes; define semantic variants, avoid per-page arbitrary hex values and cascade patches. Light/dark when supported and fallback when transparency disabled. Adaptive density: comfortable for content/consumer views; balanced for dashboard/forms; compact for data/admin, while ensuring tap size and readability. Numeric examples are suggestions, not global hardcoded policy.

## #03 Premium Balanced Glass & Surface Materials (P1/P2)
Base layer for calm background, content layer solid/legible cards/forms/tables, glass layer for selected navigation/toolbar/overlays. Avoid global blur, strong gradients, heavy shadow, illegible translucent type and stacking-context collisions. Provide opaque fallback when backdrop-filter unavailable or reduced-transparency mode requested. Recheck contrast against background changes; avoid animate-blur paint cost. No glass solely for decoration if it reduces workability.

## #04 Smart Layout & Visual Hierarchy (P0/P1)
Consistent app shell (sidebar/header/content/toolbar), clear dashboard metric hierarchy, data workspace with accessible filters/actions and bounded table scrolling when suitable, forms grouped by meaningful sections. Desktop sufficiently tall can provide one-screen workflow; do not force body to height:100vh across mobile, zoom or short screens. Allow natural scrolling, avoid nested scroll traps; sticky header/footer must not hide Save/Delete/Approve. For mobile use appropriate dynamic viewport/safe area support if needed. Do not fake KPI data or collapse functions simply to keep cards aligned.

## #05 Adaptive Responsive & Viewport Strategy (P0/P1)
Design for content: desktop productivity, tablet compact shell, mobile one-column forms, reachable actions, drawer/bottom navigation where appropriate and table-to-card or bounded horizontal scroll without information loss. Check 390 / 768 / 1024 / 1440 px as reference breakpoints, low viewport heights, small phones when relevant, orientation and 200% zoom/text scaling. Use container queries when helpful. No inaccessible off-screen buttons, hidden important fields, uncontrolled page-wide sideways scroll or fixed-height content clipping.

## #06 Smart Typography & Unified Iconography (P1)
Use existing font first; system fonts, Inter or Be Vietnam Pro are examples supporting Vietnamese. Consistent display/heading/body/label/caption scale, appropriate line-height for accents and text wrapping/truncation with safe disclosure. Desktop body often ~14–16px by density, mobile principal body ~16px; mobile inputs typically >=16px to avoid Safari zoom. Keep sufficient contrast; use tabular numerals for KPIs. One icon family (Lucide React a greenfield suggestion), consistent stroke/size (16/20/24px samples), accessible names for icon-only buttons; tap target != icon dimensions. No emoji as primary enterprise control icons. Use properly licensed fonts and sensible loading.

## #07 Component Visual Standards (P0/P1)
Buttons: clear primary/secondary/ghost/destructive and hover/press/focus/disabled/loading states. Cards: token radius/padding/border/shadow; no gratuitous glass. Data tables: readable headers, alignment, selection, row actions (context/overflow where appropriate), pagination/error/empty states, no columns/actions silently hidden. Forms: label/hint/error rhythm, visible focus; dialogs/sheets: safe viewport/keyboard, stable footer actions; tabs/navigation: clear active; popovers: correct z-index/position and no overflow clipping; alerts/toasts: meaningful outcomes without obscuring controls. Prefer installed components, add variants rather than duplicate forks. Tailwind + shadcn/ui + Radix + Lucide are optional for new projects, never a mandate.

## #08 Motion & Micro-interactions (P1/P2)
Motion explains transitions and feedback, not a decorative delay. Reference durations: small feedback 120–200ms, popover 160–260ms, sheet/dialog 200–350ms (adapt as needed). Prefer transform/opacity and CSS transitions for simple behaviors; add a motion library only when choreography warrants. Avoid excessive bounce, parallax or high-cost blur animation. Respect prefers-reduced-motion and supply functional equivalents; actions must remain usable when animation off.

## #09 Accessibility & Visual Performance (P0/P1)
Aim for WCAG 2.2 AA where applicable: conventional text contrast >=4.5:1, defined large text >=3:1, UI boundaries/state indicators >=3:1 as applicable, visible focus, non-color-only semantics, reflow and text scaling. Prefer ~44x44 CSS px mobile targets and meet WCAG 2.2 minimum/exceptions where applicable. Avoid blocking zoom, overly low-opacity text and infinite overlay. Reduced motion/transparency fallback; light/dark individually audited. Minimize expensive backdrop filters, multiple large shadows, font/image layout shift and expensive repaint. Claims of performance gains require measurement. Work with react-frontend-engineering on keyboard/focus logic.

## #10 Visual QA, Regression & Consistency (P0/P1)
CSS build success ≠ visual correctness. Inspect actual browser/screenshot when available: desktop/tablet/mobile and short height/zoom, long Vietnamese names, data loading/empty/error/selected/focus/disabled, light/dark, overlays/keyboard, navigation and shared component regressions. Use Playwright/visual snapshots where installed, never update baselines indiscriminately to conceal differences. P0: missing/inaccessible important action or unusable clipped layout. P1: major responsive breakage, hidden modal buttons, key contrast failures or component regressions. P2: minor spacing/shadow/polish deviations. P0/P1 block related signoff. Report PASS/FAIL/NOT RUN/BLOCKED with actual evidence; without browser, browser QA is NOT RUN.

## Design system implementation

Prefer project brand tokens → existing components → existing component variants → new component → external library if justified. Never replace the framework or install several competing icon/UI frameworks for cosmetic preference. Before shared-token changes identify affected screens and regression coverage. Avoid multiplying local overrides; fix base component/spacing/stacking cause. Do not use long-running animations, glass effects or clipped overflow to make screenshots look tidy.

## Coordination

- **ios-inspired-ui-design:** design system, layout, material, typography/icons, visual state, density, responsive, motion appearance, visual QA.
- **react-frontend-engineering:** functional state and interaction, form and API behavior, routing, keyboard, ARIA and focus semantics.
- **backend-engineering:** authoritative business rules, auth/authZ, persisted data, transactions, API contracts, audit/quotas.

Do not make a purely visual change that silently changes a workflow, API contract or access check. Coordinate cross-boundary fixes.

## Risk-aware workflow and Definition of Done

1. Inspect project instructions, brand, tokens, existing shared components, actual screens/actions.
2. Scope layout patterns, affected density/theme/responsive cases, risks to other screens.
3. Implement reuse-first tokens and components with premium balanced glass.
4. Render/inspect viewport, zoom, height, long text, interactive states and overlays using tools available.
5. Fix root causes and recheck affected/shared screens; separate P0/P1/P2.
6. Deliver only within project-authorized release policy after required gates; report tests and unknowns honestly.

- [ ] Brand/tokens applied consistently; content hierarchy and adaptive density fit context.
- [ ] Glass remains legible; typography/iconography coherent, Vietnamese text works.
- [ ] Buttons, cards, tables, forms, nav, modal/popover and feedback states consistent.
- [ ] Responsive, short-height and zoom do not clip critical interactions.
- [ ] Contrast, focus/touch sizes and reduced motion/transparency policy appropriate.
- [ ] No P0/P1 visual regressions, no missing business features, tests/evidence recorded.
- [ ] Unrun browser/real-device QA reported NOT RUN; delivery states verified separately.

**Absolutely forbidden:** Dropping a feature for aesthetics; solving responsive by hiding critical data/actions or overflow clipping; glass reducing readability; arbitrary CSS overrides and library sprawl; unverified visual-PASS, altered snapshot baselines to mask failures, heavy motion harming usability, changing server permissions/contracts for a mockup.