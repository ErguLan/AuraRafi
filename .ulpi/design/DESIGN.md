---
project: ProyectRaf
register: product
aesthetic_direction: technical / utilitarian
color_strategy: restrained
design_system: RafUI retained primitives
design_variance: 3
motion_intensity: 2
visual_density: 8
---

# RafUI Design Language

## Design Read

Rafi should feel like a calm instrument panel for building complex work: dense when necessary, visually quiet by default, and orange only when the engine is asking for attention or confirming agency.

## Signature

The signature is the active edge: a thin warm-orange rule appears only on the selected workspace, focused field, active dock, or primary command. It gives the editor a recognisable operational identity without gradients, glows, decorative noise, or a costly visual effect.

## Inspiration

The supplied Hub sketches establish an asymmetric workbench composition: a restrained navigation rail, a large task area, and a narrow actions/activity rail. They are layout references only. Rafi keeps that hierarchy and rejects invented project types, fake account UI, stock imagery, decorative badges, and product claims that do not exist.

## Color (locked)

The source implementation is `StudioUiPalette`. The semantic names below are normative; surfaces must not introduce raw colors outside this set.

| role | dark OKLCH / hex | light OKLCH / hex | use |
|---|---|---|---|
| background | `oklch(0.15 0.010 250)` / `#080B0F` | `oklch(0.985 0.002 100)` / `#FAFAFA` | window and empty workspace |
| surface | `oklch(0.18 0.012 250)` / `#0D1116` | `oklch(1.00 0.000 0 / 98%)` / `#FFFFFF` | panels and forms |
| raised | `oklch(0.23 0.014 250)` / `#191F26` | `oklch(1.00 0.000 0)` / `#FFFFFF` | menus, active controls, floating panels |
| canvas | `oklch(0.16 0.010 250)` / `#090C10` | `oklch(0.975 0.002 100)` / `#F7F7F7` | 2D and 3D non-document backgrounds |
| border | `oklch(0.28 0.014 250)` / `#262D36` | `oklch(0.85 0.000 0)` / `#D2D2D2` | one-pixel structural separation |
| text | `oklch(0.95 0.008 250)` / `#EDEFF2` | `oklch(0.23 0.004 260)` / `#1C1C1E` | primary text |
| muted text | `oklch(0.68 0.014 250)` / `#979FAA` | `oklch(0.49 0.004 260)` / `#606064` | secondary detail only |
| accent | `oklch(0.68 0.16 58)` / `#E8851C` | `oklch(0.64 0.15 56)` / `#E07418` | active edge, focus, one primary action |
| success | `oklch(0.62 0.12 145)` / `#4EA266` | `oklch(0.53 0.11 145)` / `#357E4A` | completed state |
| warning | `oklch(0.70 0.14 82)` / `#E0A02A` | `oklch(0.55 0.12 82)` / `#AA6F13` | caution state |
| danger | `oklch(0.57 0.16 25)` / `#CB4949` | `oklch(0.51 0.15 25)` / `#B13939` | destructive state |

Contrast contract: dark text on dark background is approximately 17:1; muted text on dark background is approximately 7:1; light text on paper is approximately 16:1; muted text on paper is approximately 6:1. Accent is never small body text on a surface. A filled accent command uses dark ink (`#121214`) for approximately 6:1 contrast in both themes.

## Type (locked)

| role | family | use | notes |
|---|---|---|---|
| display | Ubuntu Light, bundled vector font | screen titles only | 20px maximum in editor chrome; no marketing-scale type |
| body | Ubuntu Light, bundled vector font | labels, descriptions, form values | 13px / 18px default |
| utility | Ubuntu Mono when bundled; current monospace role otherwise | paths, IDs, measurements, console values | never use for prose |

The current rasterizer already ships a bundled Ubuntu vector font. No web font loader or system-font dependency is permitted. A real Ubuntu Mono asset may be added later only as an explicit small, bundled asset change.

## Scales (locked)

- Spacing: `0, 2, 4, 8, 12, 16, 20, 24, 32, 40, 48` pixels.
- Radius: `0` for rails, toolbars, and canvases; `4` for controls; `6` for menus and standalone project cards. No pills unless the control is intrinsically binary and compact.
- Elevation: base `0`, dropdown `20`, floating dock `40`, modal `50`, tooltip `60`.
- Motion: `120ms` feedback, `220ms` dock/menu transition, `360ms` layout transition, cubic-bezier `(0.16, 1, 0.3, 1)`. No bounce, no perpetual decoration, and reduced-motion removes nonessential transitions.
- Responsive thresholds: compact `760`, narrow `1024`, regular `1280`, wide `1536` logical pixels. UI scales from container constraints, never from viewport-width typography.

## Voice

Register: direct, technical, and calm. Commands use verb-first i18n keys: `Open`, `Save`, `Duplicate`, `Remove from recent`, `Cancel`. Status names state facts, not marketing language. Every user-visible string comes from i18n.

## Identity Lock

Every screen must read as the same product if placed side by side.

## RafUI Authoring Guardrails

- Build surfaces from stable semantic IDs, classes, layout constraints, and
  i18n keys. Do not use the rendered label, a color value, or transient index
  as identity.
- Use `UiTheme` and `StudioUiPalette` tokens only. A selected text label may
  use the theme focus color while a filled primary action uses orange with dark
  ink; do not turn every selected control into an orange slab.
- Rails, command rows, canvas slots, and fixed bottom docks are structural
  regions, not scroll views. A content panel owns one intentional scroll
  region. Do not let a static rail scroll because its body happens to be tall.
- A menu is an elevated `UiNodeKind::Menu` overlay, opened by a trigger and
  owned by the surface session. It closes on Escape, outside click, accepted
  command, or invalid target. It is never a hidden card glued into a layout.
- Icon-only buttons require a familiar consistent icon, accessible label, and
  tooltip. Generate/use one coherent high-density icon family; do not stretch
  low-resolution PNGs or mix unrelated visual styles.
- A minimap, cable route, selection outline, component count, and status read
  from the same scene/CAD model as the renderer. Decorative or approximate UI
  copies are rejected.
- Keep native window chrome native. App commands sit in the shared command row
  or a documented host integration; do not imitate operating-system titlebar
  buttons or invent product/account chrome that has no backend behavior.
- Every finished surface is checked in dark, light, compact, regular, high-DPI,
  keyboard-only, GPU, and CPU recovery presentation modes.

## Frontier RafUI Core Contract

The technical/utilitarian identity is expressed through precise geometry and
quiet motion, not through decorative effects. The engine's reusable UI core
must provide the following primitives:

- Global overlays with BottomStart placement, edge flipping, and viewport
  shifting. Tooltips are compact neutral surfaces with a one-pixel border and
  the active orange edge reserved for focus and command agency.
- Intrinsic sizing for localized labels and popovers. Content measurement is
  resolved through the bundled text atlas before final paint.
- Session-owned pointer enter/leave, hover intent, focus, active state, and
  monotonic time. Documents remain serializable and free of transient state.
- Time-based transitions using the locked 120ms feedback and 220ms dock motion
  values. Reduced motion removes nonessential interpolation.
- Reusable semantic recipes for icon buttons, panel headers, tree rows, and
  tooltips. Recipe output remains ordinary RafUI nodes and inherits the same
  tokens.
- Logical-point layout with physical-pixel target allocation and a bounded
  raster scale. GPU and CPU recovery consume equivalent retained frames.
- Data-only diagnostics for boxes, clipping, hit regions, text requests,
  zero-size geometry, and z-order so visual review is evidence-backed.

The signature remains the active edge. A tooltip does not become a brown bar,
a focused button does not expand, and motion never exists only to decorate the
screen. Every interaction must improve reading, targeting, or state feedback.

The complete quality charter is recorded in
`.ulpi/design/aurarafi-ui-quality.md`. It is the review authority for
pixel-shimmer, subpixel-jitter, texture-bleeding, resampling, and GPU/CPU
visual parity defects.
