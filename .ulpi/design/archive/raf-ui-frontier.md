---
project: ProyectRaf
feature: RafUI Frontier Core
binds_to: .ulpi/design/DESIGN.md
aesthetic_direction: technical / utilitarian
design_system: RafUI retained primitives
---

# RafUI Frontier Core

## Design Read

RafUI should feel like a precision instrument for building software: dense,
quiet, immediate, and exact at the edge of interaction. The bet is that a
small Rust-native core can deliver the predictability of a retained renderer
without importing a browser runtime or hiding layout decisions behind magic.

## Product problem

The previous core could describe panels, styles, and events, but common
professional behaviors were implemented inside bridges: tooltips were trapped
by surface clipping, translated text needed guessed widths, hover animation was
frame-step based, and visual constants accumulated in large editor files.

The frontier core makes those behaviors first-class and shared across GPU and
CPU hosts.

## Primary flow

```text
Author semantic nodes
  -> resolve theme, responsive rules, and i18n keys
  -> measure intrinsic text and image bounds
  -> update pointer/focus/hover session state
  -> place overlays in window coordinates
  -> advance motivated motion
  -> compile one retained draw list
  -> present through GPU or CPU recovery
```

## State coverage

| State | Required behavior |
| --- | --- |
| Initial | No tooltip, no animation loop, stable structural geometry |
| Hover enter | `HoverEnter`, tooltip resolves, compact overlay fades in below the anchor |
| Hover move | Overlay follows the pointer anchor without changing owner geometry |
| Hover leave | `HoverLeave`, overlay target fades to zero, no stale tooltip remains |
| Window edge | Preferred placement flips, then shifts inside the viewport |
| Long translation | Text is measured from the atlas and constrained by max width |
| Compact dock | Structural rows preserve minimums, then wrap/stack according to policy |
| High DPI | Logical pointer/layout coordinates stay unchanged; target pixels increase |
| Reduced motion | Transition jumps to its target and schedules no nonessential frames |
| CPU recovery | Same layout, text, overlay semantics, and diagnostics as GPU |
| Invalid layout | Diagnostics report zero-size or clipping warnings before visual review |
| Keyboard only | Focusable controls expose labels, Tab order, Enter/Space actions, Escape dismissal |

## Component specifications

### Global overlay

Purpose: render transient content above its owner surface without allowing the
owner's clip rectangle to corrupt placement.

Contract: `UiPlacement` chooses the preferred side, flips to the opposite side
when necessary, then shifts into the viewport. Overlay geometry is logical;
the compositor chooses physical target density.

### Tooltip

Purpose: explain an icon-only or unfamiliar command without changing toolbar
geometry.

Visual: neutral dark/light gray fill, one-pixel neutral border, 4px radius,
10.5px bundled body face, 8px horizontal breathing room, no brown slab, no
gradient, no shadow-heavy decoration.

Behavior: `BottomStart`, 8px gap, 120ms ease-out entry, matching exit, text
measured from the atlas, flip/shift near window edges, no pointer capture.

Accessibility: source control owns the accessibility label and tooltip key;
keyboard focus exposes the same text without requiring a pointer hover.

### Icon button

Purpose: stable command target for toolbars and dense editor chrome.

Contract: familiar high-density icon, 32px desktop target, focusable, stable
semantic ID, typed command action, accessibility label, tooltip key, active
orange edge only for selected/focused state.

### Structural panel header

Purpose: give hierarchy/properties/sessions sections a consistent reading
anchor without creating nested decorative cards.

Contract: fixed 32px row, panel-title text role, token border, one active edge
when selected, no content-dependent resizing.

## Engineering boundaries

| Concern | Module |
| --- | --- |
| Nodes and recipes | `crates/raf_ui/src/node.rs`, `components.rs` |
| Size modes | `crates/raf_ui/src/layout.rs` |
| Overlay placement | `crates/raf_ui/src/overlays.rs` |
| Input and hover time | `crates/raf_ui/src/focus.rs`, `interaction.rs` |
| Motion | `crates/raf_ui/src/motion.rs` |
| Density | `crates/raf_ui/src/environment.rs` |
| Text measurement and draw list | `crates/raf_render/src/ApiGraphicBasic/ui_surface/` |
| Tooltip surface recipe | `crates/raf_editor/src/panels/raf_ui_tooltip.rs` |
| Temporary texture placement | `crates/raf_editor/src/panels/raf_ui_surface_bridge.rs` |

The eframe bridge may compose a completed RafUI texture during migration. It
must not paint retained rectangles, text, tooltips, menus, or interaction
state itself.

## Accessibility and performance

- All user-visible strings remain JSON i18n keys in English and Spanish.
- Focus is visible through the locked active edge and never depends only on color.
- GPU is normal; CPU is recovery, testing, headless, or unsupported hardware.
- Motion stops repaint requests when a tween settles.
- Intrinsic measurement uses the bounded text atlas and does not allocate an
  unbounded cache per frame.
- Logical geometry is transformed into the physical target before GPU NDC
  conversion. Exact 1:1 retained-surface presentation uses nearest sampling;
  fractional DPI uses linear sampling.
- Diagnostics are available from both host paths and can feed golden layout tests.

## Build handoff

Target: Codex / Rust RafUI engineer.

Implement exactly this contract using the existing retained document,
`UiSurfaceFrame`, `UiSurfaceDrawList`, theme tokens, and typed action boundary.
Do not redesign the visual identity or introduce a browser/runtime widget
layer. Keep the Viewport, Schematic, and PCB renderer-owned.

## Design Pre-Flight

- Identity lock: pass. All values bind to `DESIGN.md`.
- Anti-slop: pass. No purple/blue glow, gradient, fake glass, nested cards, or decorative animation.
- State coverage: pass. Hover, leave, edge placement, translation, compact, DPI, reduced motion, GPU, CPU, keyboard, and invalid layout are specified.
- Accessibility: pass. Focus, labels, keyboard actions, translated text, and reduced motion are explicit.
- Layout craft: pass. Owner surfaces and global overlay layers are distinct structural families.
- Cognitive load: pass. Transient explanations remain contextual and never compete with the primary command row.

Scored self-critique: distinctiveness 4, hierarchy 4, consistency 4,
accessibility 4, state coverage 4, copy quality 3, restraint 4, motion
motivation 4. Total 31/32. The only 3 is copy quality because tooltips remain
dependent on each surface's existing localization catalog; the core does not
invent labels that the product has not defined.
