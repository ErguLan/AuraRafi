---
feature: startup-splash
binds_to: .ai/STUDIO_GRADE_UI.md
register: product
aesthetic_direction: technical / utilitarian, startup ignition variant
---

# Startup Splash

## Design Read

The editor should arrive as a compact tool booting up, not as a mostly empty
full-sized workbench. The mark is the one focal point. The progress bar proves
that the engine is preparing resources before the Hub is usable.

## Screen Contract

- The startup viewport is a compact `720 x 600` logical-point, borderless,
  non-resizable window. It centers itself on the primary monitor.
- The surface uses the existing dark RafUI palette even when the later editor
  preference is light. It is a temporary product boot state, not a second
  theme.
- One `560 x 500` surface panel contains the existing engine mark, `RAFI`, the
  orange signature rule, localized loading state, progress bar, `Engine`, and
  the build version.
- The old marketing sentence and Yoll credit are absent. Avoid decorative glow,
  gradients, extra cards, fake status indicators, or invented controls unless a
  future brief explicitly gives them a real product purpose.
- Only semantic RafUI palette tokens are used. The orange rule and progress
  fill are the existing product signature.

## Flow: Start Engine

**Goal:** make startup legible while the engine warms the interface resources.

```
[Executable starts]
        |
        v
[Compact splash]
        |
        +--> preload Hub and editor icon textures
        |
        +--> keep visible for at least 2.4 seconds
        |
        v
[Resources ready or 6 second recovery limit]
        |
        v
[Restore native window chrome and open Hub]
```

### States

| State | Visual | Behavior |
| --- | --- | --- |
| Warming | Brand, `Cargando espacio de trabajo...`, partial bar | Preloads actual icon textures. |
| Ready | Bar reaches 100% | Restores window decorations, resizability, and editor dimensions. |
| Resource fallback | Bar still advances | Missing icon resources count as complete so startup cannot stall. |
| Slow storage | Same compact screen up to six seconds | Enters the Hub after the recovery limit. |

## Component: StartupSplash

| Element | Contract |
| --- | --- |
| Brand mark | Existing embedded `editor/icon.png`, contained at 172 px. |
| Wordmark | Bundled Ubuntu Light at 36 px. Static product identity. |
| Progress | 390 px track, 6 px height, semantic raised surface plus accent fill. Percentage is visible text. |
| Status | Localized i18n key. No user input or buttons. |
| Motion | Repaint pacing only. No bounce, glow, or looping decoration. |

## Accessibility

- There is no interactive focus path while the splash is visible.
- Status and percentage use the existing text and muted-text contrast tokens.
- Reduced-motion users see the same discrete progress updates without a
  decorative animation.

## Acceptance Criteria

- Startup opens in the compact borderless splash before the Hub.
- The screen does not show the native File/Edit menu while loading.
- Progress represents icon preloading and cannot block for longer than six
  seconds.
- `app.loading_workspace` and `app.engine` exist in English and Spanish.
- Hub transition restores native decorations, normal resize behavior, and the
  standard editor size.

## Design Pre-Flight

- Visual consistency: pass. Uses the Studio Grade UI defaults for palette, type,
  spacing, and the orange signature.
- Default review: pass. No unsupported effect, fake copy, or decorative status
  furniture is needed for this brief.
- State coverage: pass. Warming, ready, resource fallback, and slow storage
  recovery are defined.
- Accessibility: pass. No interactive controls, visible text progress, and
  token-based contrast.
- Self-critique: distinctiveness 3, hierarchy 4, consistency 4,
  accessibility 3, state coverage 3, copy 4, restraint 4, motion 4.

## Build Handoff

Implement exactly this specification with the existing RafUI retained surface,
ApiGraphicBasic presentation bridge, i18n files, and native viewport commands.
Do not redesign the editor shell or add a separate startup renderer.
