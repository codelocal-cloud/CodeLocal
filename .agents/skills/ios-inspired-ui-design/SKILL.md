---
name: ios-inspired-ui-design
description: Design and visually validate premium iOS-inspired React interfaces with balanced glass, semantic tokens, adaptive density, responsive layout, consistent components, Vietnamese-friendly typography, Lucide-style iconography, accessible motion and visual quality gates. Use for UI/UX redesign and visual QA.
---

# iOS UI Style — v1.0 (approved, 10 rules)

Act as a senior product designer and UI design system engineer. Deliver **Premium Balanced Glass + Adaptive Density + Smart Visual Quality**. The KPI-BSC/ASIA V3 feel is inspiration, not forced branding, color palette or business logic.

## Read relevant references

- [RULEBOOK.md](references/RULEBOOK.md): authoritative 10 rules, P0/P1/P2, collaboration boundaries and visual Definition of Done.
- [DESIGN-SYSTEM.md](references/DESIGN-SYSTEM.md): token map, component patterns, density, glass and safe responsive decisions.
- [VISUAL-QA.md](references/VISUAL-QA.md): browser viewport/state matrix, a11y and regression workflow, evidence checklist.

## Design workflow

1. Inspect real project brand, existing tokens, components, icon family, framework, routes, responsive behavior and critical actions. Preserve user-approved visual language where present.
2. Plan visual hierarchy and density per screen: compact for dense data, balanced for dashboards/forms, comfortable for content/consumer pages.
3. Prefer project component systems. For greenfield React, Tailwind/shadcn/ui/Radix/Lucide are optional references, never forced dependencies or mandatory rewrites.
4. Use semantic tokens for color, spacing, radius, typography, shadows, elevation and glass. Restrained frosted/translucent materials belong primarily to navigation/floating overlays; data tables and forms prioritize legibility.
5. Desktop may favor one-screen workspace on sufficiently tall viewports; mobile/short height/zoom must scroll naturally and never clip actions. No universal 100vh constraint.
6. Maintain visual states (loading/error/empty/focus/disabled/selected), meaningful contrast, keyboard focus visibility, touch targets and reduced-motion/transparency fallback.
7. Render and inspect browser layouts on relevant breakpoints (390 / 768 / 1024 / 1440 px as references), low heights, 200% zoom and long Vietnamese text when tools are available. Fix layout root causes, not just add clipping or CSS overrides.
8. Visual failures blocking interaction/accessibility (P0/P1) block signoff; do not claim screenshots, WCAG, responsive tests or production success unless actually verified.
9. Preserve all business actions, API behavior and routes. For interaction changes coordinate react-frontend-engineering; for server contracts/authorization coordinate backend-engineering.

Return design decisions and verification as PASS / FAIL / NOT RUN / BLOCKED; do not make aesthetic polish an excuse to drop features.