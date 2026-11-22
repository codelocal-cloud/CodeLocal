# Visual QA and regression verification checklist

Use with iOS Style Rule #10 and React Rule #11. A static build or screenshot mock is not evidence the actual UI works.

## Inspection matrix

| Dimension | Cases to inspect when affected |
| --- | --- |
| Width | ~390 mobile, 768 tablet, 1024 laptop, 1440 desktop |
| Height | Low-height laptop/mobile, virtual keyboard and safe-area |
| Scaling | 200% browser zoom, increased font size, long Vietnamese text |
| Theme | Light/dark if supported; reduced transparency and reduced motion |
| Interactive states | Default, hover, keyboard focus, disabled, loading, selected, error, empty, forbidden |
| Overlay | Modal footer, sheet, dropdown portal, tooltip, sticky header, overflow clipping |
| Data | Zero vs unknown, large numbers, many rows, empty list, long labels |
| Regression | Shared app shell, tokens/components used by other screens |

## Inspect → implement → render → fix → recheck

1. Inspect actual components/tokens/screens, user roles and required action controls.
2. Implement through shared tokens/components before per-screen patches.
3. If a browser is available, run the UI and inspect relevant screen sizes/states, console and network errors as appropriate.
4. Log a specific defect with route, viewport, state, evidence and impact.
5. Fix root cause (layout/grid sizing, overflow, stacking context, typography or breakpoint); **do not** hide critical content with clipping or display:none.
6. Repeat inspection at affected widths and shared component consumers. Never blindly accept visual baselines.

## Severity and release gate

- P0 blocker: required action unreachable, content hidden, primary flow unusable; do not approve release.
- P1 major: significant responsive break, modal action clipped, critical contrast failure or shared regression; fix before signoff.
- P2 minor: spacing/shadow/small transition polish; track by risk/project acceptance.
- PASS means visually inspected with actual evidence. FAIL means defect reproduced. NOT RUN means no check. BLOCKED means tool/environment/access missing. No browser means runtime visual QA NOT RUN, not PASS.

## Accessibility checks

Semantic/keyboard behavior belongs to react-frontend-engineering. Visual QA should test WCAG 2.2 AA contrast rules (ordinary text 4.5:1, large text 3:1, UI non-text indicators 3:1 when applicable), visible focus and meaning beyond color; check mobile target area around 44 CSS px preferred, zoom/reflow and fallback for reduced motion/transparency. Do not claim full WCAG conformance from a screenshot or automated scan alone.

## Example handoff

Route/component: ...
Brand/theme/density: ...
Viewport(s) rendered: ...
States checked: ...
PASS: ...
FAIL: ...
NOT RUN: ...
BLOCKED: ...
Regression/known risks: ...
Functional actions unchanged? (verify with react-frontend-engineering) ...
Delivery and browser/device UAT independently verified? ...