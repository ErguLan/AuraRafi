# Inspector / Sessions UI Polish

## Design read

Sessions should read as a compact project control surface, not as a stack of
generic cards. The active edge remains the only strong accent; structure comes
from spacing, icon semantics, and a single intentional list scroll region.

## Direction

This pass binds to the existing `technical / utilitarian` RafUI language in
`.ai/STUDIO_GRADE_UI.md`: near-black tinted surfaces, Ubuntu roles, compact
control radii, orange reserved for focus and primary creation, and purposeful
motion only.

## Changes implemented

- Keep the heading and create controls outside the session list scroll region.
- Prevent the create action from growing into the free column height.
- Give every session the same three action slots: Open, Duplicate, Remove.
- Disable Open and Remove for the active session instead of changing row shape.
- Add semantic session icons and an explicit localized Active label.
- Use lighter alpha-derived surfaces, restrained borders, and focused states.
- Preserve command IDs and host/application ownership unchanged.

## States and interaction contract

| State | Treatment |
|---|---|
| Active | Raised surface, orange edge, semantic icon, `Active` label; Open/Remove disabled. |
| Inactive | Quiet alternate surface; Open, Duplicate, and Remove available. |
| Hover | Slight surface lift and structural border; no layout shift. |
| Focus | Orange border; keyboard order follows create input, create button, then rows. |
| Empty | Controls remain visible; list occupies the remaining region without a fake row. |
| Narrow | Controls stay in one row until their minimum width, then use RafUI compact layout. |
| Reduced motion | No additional animation; existing panel transition remains host-owned. |

## Performance and accessibility

- One vertical scroll owner: the session list.
- No per-frame persistence, command, or registry mutation is introduced.
- Stable semantic IDs remain unchanged for existing commands.
- All new visible copy uses the existing locale dictionaries.
- Icon meaning is paired with text and does not replace keyboard-targetable labels.

## Pre-flight result

- Visual consistency: pass. Uses the Studio Grade UI defaults for palette,
  typography, radius, and motion.
- Default review: pass. No gradient, glow, pill, nested card, or decorative
  badge is needed for this brief.
- State coverage: pass for active, inactive, hover, focus, disabled, empty, and narrow layouts.
- Accessibility: pass for visible focus, localized labels, stable controls, and keyboard order.
- Performance: pass by inspection for this UI-only pass; runtime FPS validation remains a live QA step.

## Build handoff

Implemented in the retained Rust RafUI surface at
`crates/raf_editor/src/panels/inspector_surface.rs`. The host and session
command pipeline remain the source of truth. Do not move session logic into
the surface or introduce a second UI toolkit.
