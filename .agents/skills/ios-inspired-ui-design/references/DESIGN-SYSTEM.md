# Design tokens, balanced glass, adaptive layout — implementation reference

This describes **choices**, not fixed brand colors. Reuse destination brand and UI system first; never hardcode KPI-BSC/ASIA palette.

## Semantic token families

- Colors: background, foreground, surface, surface-elevated, primary, primary-foreground, secondary, muted, border, focus, success, warning, danger, info, disabled.
- Spacing: consistent small steps (e.g. 4/8/12/16/24/32 CSS px); scale is illustrative.
- Radius: graduated tiers (e.g. 12/16/20/24 CSS px); do not maximize radius on every item.
- Typography: display/title/subtitle/body/label/caption, line heights suitable for Vietnamese.
- Elevation: flat/raised/floating/overlay; named z-index layers for sticky, menus, dialog, toasts.
- Glass: tint, alpha, blur, subtle border/highlight, opaque accessibility fallback.
- Motion: fast/normal/slow durations, easing, reduced-motion overrides.
- Density: comfortable/balanced/compact variants by screen task.

Prefer CSS custom properties or project token system as source-of-truth, consumed through Tailwind/design-system mapping where in use. Avoid per-screen arbitrary hardcoded colors, shadow rules, duplicated utility strings or endless CSS specificity overrides.

## Balanced glass application

Base: calm app canvas. Content: readable, usually solid or near-solid forms, cards, report and data tables. Glass: purposeful translucent navigation, toolbar and floating panels. Always check text contrast on real background. When backdrop-filter unsupported or reduced transparency preferred, use an opaque surface. Avoid glass across large scroll regions and animation of large blur radii.

## Adaptive density by use case

| Use case | Density | Notes |
| --- | --- | --- |
| SaaS marketing/content | Comfortable | generous reading rhythm, larger touch areas |
| General dashboard and forms | Balanced | section hierarchy, reachable actions |
| Admin data workspace | Compact on desktop | small but readable labels, scoped table scrolling |
| Mobile of any above | Adaptive/comfortable touch | never compress targets simply to fit viewport |

Desktop sufficiently tall may keep nav/filters and primary actions visible. Avoid fixed 100vh with clipped content for short screens, text enlargement or mobile. Use natural scroll where necessary, overflow container only for genuinely large tables. Respect safe-area and virtual keyboards.

## Component anatomy checklist

- Button: primary/secondary/ghost/danger + default/hover/press/focus/loading/disabled.
- Data table: heading, sort indicators, row state, status, action overflow (edit/delete when appropriate), loading/empty/error and bounded viewport.
- Form: label/hint/input/error/required, group logic, save/cancel reachable; field state is handled by react-frontend-engineering.
- Modal/sheet: header/content/footer, max available height, scroll region if necessary, safe mobile action area.
- Popover/dropdown: portal/layer rules, collision avoidance at viewport edges, escape/focus coordination.
- Navigation/tab: unambiguous active state, appropriate labels, no hiding important workflows in arbitrary tabs.
- KPI card: units, period, data-freshness and missing-vs-zero visual clarity; do not invent data.
- Toast/alert: clear type and message; not a substitute for field error.

## Visual stack and dependencies

For greenfield React, Tailwind CSS + shadcn/ui + Radix primitives + Lucide React may help AI consistency. If Chakra/MUI/Ant/other design system already exists, adopt that first; avoid parallel libraries unless explicitly planned. One coherent icon set. CSS transitions for ordinary effects, an animation library only for complex sequences and with reduced motion support.