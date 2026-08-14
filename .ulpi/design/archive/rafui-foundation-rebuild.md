---
project: ProyectRaf
feature: RafUI foundation rebuild
register: product
aesthetic_direction: technical / utilitarian
design_system: RafUI Core Primitives
status: foundation-implemented
implementation_scope: foundation_only
---

# RafUI Foundation Rebuild

## Decision

The next editor interface must not be authored until RafUI can express and
render professional desktop UI behavior without panel-specific workarounds.
The foundation is accepted as one complete quality bundle: seven gates must
pass before Game or Electronics chrome is reconstructed.

This is not a visual reskin and it is not a return to the deleted RafUI Studio.
It is a renderer-neutral UI runtime, component contract and native workbench
host shared by editor surfaces and future in-game UI.

## Design read

AuraRafi is a precise technical instrument: canvas first, dense but readable,
quiet at rest and explicit when the user acts. White/neutral information
creates hierarchy; orange is a signal for active state, focus and primary
agency. Thin contours organize regions. Gradients, decorative blur, glowing
chrome, oversized controls and ornamental cards are excluded.

The signature remains the active edge defined in `DESIGN.md`. It must be
rendered as stable physical geometry, never as a blurred texture effect.

## Objective

Designing a new surface should require composition and domain wiring, not
renderer repair. An author should be able to build a Hierarchy, Inspector,
toolbar, menu or game HUD from documented primitives and receive correct
layout, density, text, icons, input, clipping, overlays and accessibility by
default.

Success means:

- no Egui dependency in RafUI, the future editor shell or product surfaces;
- no `wgpu` type in RafUI, editor, Game UI or Electronics UI public code;
- one ApiGraphicBasic presentation contract for GPU and CPU recovery;
- one deterministic logical layout at every supported DPI;
- no surface-specific tooltip, sampling, font or resize workaround;
- no giant application file containing document, layout, rendering and domain
  mutation together;
- the same primitives can be embedded in Game, Electronics and later runtime
  UI without forking their behavior.

## The rendering boundary

```text
Game / Electronics / runtime UI state
  -> focused surface builder
  -> UiDocument + semantic IDs + typed UiAction
  -> measure / arrange / clip / hit test
  -> backend-neutral UiDisplayList
  -> ApiGraphicBasic UiPresenter
       -> private GPU backend
       -> CPU recovery backend
  -> native window surface
```

RafUI describes meaning, layout intent, visual tokens and actions.
ApiGraphicBasic resolves physical presentation resources. The native host
supplies window size, DPI, input, IME, cursor and a presentation surface.

WGPU may remain temporarily as a private ApiGraphicBasic backend. It is not
allowed to remain the public UI architecture. A literal immediate removal of
WGPU would require replacing device, queue, swapchain, resource, shader,
synchronization and recovery implementations before RafUI work could continue.
The correct immediate move is to seal it behind ApiGraphicBasic handles and
backend traits so every upper layer is already independent from it.

## Why text, icons and geometry are separate

They share logical layout but do not share rasterization or sampling policy:

- **Geometry** is rectangles, borders, paths and clip regions. One-logical-pixel
  edges snap to physical pixels when the DPI permits it. Geometry is not a
  bitmap and must not inherit font supersampling.
- **Text** is shaped from real font faces, measured in logical units and
  rasterized at the physical text density. Regular, Medium and Bold are actual
  bundled font weights, not one Light face darkened by changing alpha
  coverage.
- **Icons** resolve from a semantic icon ID. Monochrome technical icons use
  compact vector/path data rasterized and cached for the requested density.
  Multicolor art may use explicit high-density bitmap assets with generated
  mip levels. An icon never reuses a low-resolution PNG stretched to arbitrary
  DPI.

The three streams join only in `UiDisplayList`, where they preserve their own
resource and sampling metadata. This avoids the previous failure where scaling
one completed UI texture changed borders, glyphs and icons together.

## Implementation checkpoint

The renderer boundary, semantic icon stream, global tooltip contract, color
cascade, CPU recovery path, AGB-owned text atlas, and bundled Ubuntu
Regular/Medium/Bold faces are implemented. The remaining acceptance work for a
future full editor shell is native window ownership. The font files and UFL
license live under `crates/raf_render/assets/fonts`; AGB selects the real
outline by semantic weight and does not darken coverage synthetically.

## Gate 1: Native host and ApiGraphicBasic ownership

### Required work

- Replace the Eframe application boundary with a Winit/native event-loop host.
- Introduce backend-neutral ApiGraphicBasic surface/frame/target handles.
- Move direct WGPU device, queue, texture-view and surface calls into a private
  backend module.
- Make the UI presenter accept `UiDisplayList`, resource handles, target size
  and clear/composition policy rather than raw backend objects.
- Preserve the CPU compositor as a first-class recovery implementation.
- Move viewport rectangle allocation and presentation out of Egui. The
  renderer receives a canvas region resolved by the same workbench layout.

### Acceptance

- `rg "egui|eframe" crates/raf_ui` returns no product dependency.
- The future editor application and surface builders import no Egui symbols.
- `rg "wgpu::" crates/raf_ui crates/raf_editor` returns no public/product UI
  use after the native migration.
- GPU loss can fall back to CPU presentation without changing the document or
  layout.

## Gate 2: Typography and icon fidelity

### Typography

- Keep semantic `UiTextRole`, `UiFontWeight` and text style declarations in
  RafUI.
- Move font registry, glyph rasterization, atlas allocation and backend upload
  behind ApiGraphicBasic.
- Bundle real Regular, Medium and Bold faces with an explicit fallback chain.
- Add deterministic line breaking, baseline alignment, ellipsis, wrapping and
  intrinsic measurement.
- Keep locale resolution before measurement and cache by text, face, weight,
  size, width, locale and raster density.
- Reserve atlas guard pixels and use subpixel placement only where the backend
  can produce stable results.

### Icons

- Add `UiIconId`, semantic size and state; document builders never reference a
  filesystem path directly.
- Define one lightweight technical icon family with consistent stroke,
  optical box, corner treatment and baseline.
- Prefer offline-authored path data for monochrome editor icons. Cache the
  raster result by icon, density, size, state and theme.
- Keep colored preview art separate from command icons.
- Validate icons at their actual 14, 16, 18, 20 and 24 logical-pixel uses, not
  only in a 64 or 256 pixel source preview.

### Acceptance

- Text remains readable and stable at 100%, 125%, 150% and 200% DPI.
- Hover/focus elsewhere cannot soften or rerasterize unrelated text.
- Regular, Medium and Bold have distinct real glyph outlines and unchanged
  layout semantics.
- Icons do not shimmer, crawl, change silhouette or bleed neighboring atlas
  content during hover, resize or motion.

The current AGB implementation satisfies the ownership, bounded atlas,
intrinsic measurement, DPI, icon, and multi-face typography requirements.

## Gate 3: Deterministic responsive layout, clipping and scrolling

### Required work

- Replace repair-style intrinsic sizing with an explicit two-pass
  `measure -> arrange` contract.
- Support minimum, preferred, maximum, fit-content, fill and aspect constraints
  without guessed text widths.
- Resolve compact/narrow/regular/wide behavior from container constraints, not
  from hardcoded screen screenshots.
- Define one clip stack shared by paint and hit testing.
- Give each content region one intentional scroll owner; structural rails,
  topbars and fixed docks do not accidentally scroll.
- Add nested scroll routing, wheel/trackpad normalization, scrollbar policy,
  scroll-to-focus and stable restoration.
- Add list virtualization for Hierarchy, Assets, logs and component catalogs.
- Recompute from canonical constraints after resize; never retain a
  hover-expanded width or stale compact layout.

### Acceptance

- Collapsing and restoring a panel restores labels and hit regions correctly.
- Text truncates or reflows according to the component contract; it never
  disappears permanently after resize.
- A child outside the clip cannot paint or receive pointer input.
- Ten thousand tree/log rows remain bounded by visible-row work.

## Gate 4: Interaction, focus, keyboard and accessibility

### Required work

- Normalize Winit pointer, wheel, key, modifier, repeat, character and IME
  events into backend-neutral `UiInputState`.
- Add pointer capture, drag threshold, click count and cancellation.
- Keep hover, pressed, focus, selection and disabled as separate states.
- Add focus scopes, declared tab order, roving focus for toolbars/tree rows,
  directional navigation, default action and Escape dismissal stack.
- Ensure Enter/Space activation and shortcut dispatch use the same typed command
  as pointer activation.
- Add semantic role, name, value, state and description to interactive nodes so
  a platform accessibility adapter can be connected without rewriting
  surfaces.
- Ensure every icon-only control has an accessible label and tooltip key.

### Acceptance

- Every core primitive is operable without a mouse.
- Focus never becomes trapped behind a closed overlay or removed node.
- Hover changes paint only; it never changes a control's measured size.
- Input replay produces the same action sequence from the same document/state.

## Gate 5: Global overlays, menus and tooltips

### Required work

- Maintain one window-level overlay root with ordered layers for menus,
  popovers, drag previews, modals and tooltips.
- Keep semantic ownership with the trigger while removing owner clipping from
  overlay placement.
- Measure content first, then place with preferred side, gap, edge flip and
  viewport shift.
- Tooltips use delayed hover intent, also appear for keyboard focus, and
  disappear on leave, press, Escape or invalid target.
- Tooltip animation is a fast 120ms opacity/2px translation that never changes
  measured size or source layout. Reduced motion removes translation.
- Menus own focus scopes, keyboard navigation, outside-click dismissal and
  typed command activation.

### Acceptance

- A tooltip appears below its icon or pointer anchor, never on top of the icon
  unless edge flipping is the only valid placement.
- Tooltip width is intrinsic to its text within min/max bounds; no brown bar,
  giant fixed rectangle or owner-surface expansion is allowed.
- Opening or closing any overlay leaves the underlying layout fingerprint
  unchanged.

## Gate 6: Workbench docking and resize

### Required work

- Build a `UiWorkbench` coordinator from serializable dock data plus transient
  drag/resize state.
- Treat central canvas, supporting docks, floating panels, tab stacks,
  splitters and status regions as explicit structural roles.
- Enforce per-panel minimums, allowed sides, collapse policy and restoration
  repair.
- Separate dock model persistence from domain/session data.
- Route dock animation through one monotonic motion clock and respect reduced
  motion.
- Make Game and Electronics provide workspace descriptors and panel
  capabilities, not separate docking implementations.

### Acceptance

- Resize, float, dock, tab, hide, restore and invalid-layout repair are covered
  by deterministic tests.
- The viewport always receives the resolved center rectangle and does not
  participate in dock calculations.
- A supporting panel cannot cover the central canvas without an explicit
  floating/overlay policy.

## Gate 7: Definitive primitives, diagnostics and visual QA

### Core primitives

The first stable kit consists of:

- `Panel`, `PanelHeader` and `SectionHeader`;
- `Toolbar`, `IconButton`, `TextButton` and separators;
- `SegmentedControl` and `EditorTab`;
- `Tree`, virtualized `TreeRow` and disclosure/visibility/context actions;
- `Field`, `NumericField`, `TextField`, `Select`, `Toggle` and validation;
- `ScrollRegion`, virtualized list/grid and scrollbar;
- `Menu`, `Popover`, `Tooltip`, `Dialog` and empty/loading/error states;
- `DockArea`, `DockTabStack`, `Splitter` and floating panel;
- `CanvasSlot`, viewport HUD rail and status readout.

Every primitive owns complete geometry and states: initial, hover, pressed,
focused, selected, disabled, error, compact, high-contrast and reduced-motion.
Surface code composes primitives and maps typed actions; it does not redraw
their borders, tooltip or focus treatment.

### Diagnostics and golden QA

- Emit data-only diagnostics for layout boxes, clip stacks, hit regions,
  focus order, text/icon requests, atlas pages, zero-size nodes, paint order,
  batches and cache invalidation reasons.
- Add deterministic reference surfaces for every primitive, not a user-facing
  RafUI Studio screen.
- Capture golden images at 100%, 125%, 150% and 200% DPI, dark/light, GPU/CPU,
  compact/regular and initial/hover/focus/pressed states.
- Compare geometry exactly and images with a documented perceptual threshold.
- Run resize sweeps and recorded input sequences in CI.
- Require a real visual review; compilation and unit tests are necessary but
  never proof of visual quality.

### Acceptance

- GPU and CPU frames have equivalent layout, clipping, text metrics and action
  regions.
- A state change invalidates only the necessary paint/resources.
- No one-pixel border moves, unrelated label softens or icon changes shape
  between adjacent state snapshots.

## Target module ownership

```text
crates/raf_ui/
  document.rs       semantic tree and stable identity
  layout.rs         constraints only
  text.rs           semantic roles/styles/requests only
  icons.rs          semantic icon IDs/requests only
  interaction.rs    backend-neutral state machine
  overlays.rs       placement and ownership
  docking.rs        serializable workbench model
  components/       definitive recipes and state contracts
  semantics.rs      accessibility metadata

crates/raf_render/src/ApiGraphicBasic/
  surface/          backend-neutral native surface/frame handles
  ui_surface/
    compile/        measure, arrange, clip, hit test, display-list build
    typography/     font registry, shaping/rasterization and atlas
    icons/          path/bitmap resolution and atlas
    presenter/      backend-neutral UiPresenter
    cpu/            recovery compositor
  backends/
    wgpu/           temporary private GPU implementation
    native/         future owned platform backends

crates/raf_editor/
  native_app.rs     window lifecycle and product routing only
  workbench/        Game/Electronics workspace descriptors
  surfaces/         focused builders and typed action adapters
```

Exact filenames may follow existing crate conventions during implementation,
but these ownership boundaries are normative.

## Errors from the previous attempt that are now forbidden

1. Fixing a renderer/density defect inside one panel or screenshot-specific
   surface.
2. Letting Egui draw, measure, place or own input for permanent RafUI chrome.
3. Exposing raw WGPU types above the private ApiGraphicBasic backend.
4. Scaling a completed low-density UI texture to solve DPI.
5. Sharing one sampling rule among text, icons and solid geometry.
6. Treating a synthetic alpha adjustment as a real font weight.
7. Stretching one PNG source across every logical size and DPI.
8. Putting a tooltip/menu in its trigger's row layout or enlarging the owner to
   make it visible.
9. Allowing hover/focus to alter measured control dimensions.
10. Using magic widths/heights copied from one screenshot instead of
    constraints and intrinsic measurement.
11. Combining shell orchestration, surface trees, rendering, transient input
    and domain mutation in one large file.
12. Duplicating scene, CAD, selection or command state inside UI widgets.
13. Inventing actions/features visible in a concept image but absent from the
    domain.
14. Claiming visual completion from `cargo check` or tests without the DPI and
    state screenshot matrix.
15. Rebuilding all editor panels before the seven foundation gates pass.

## Implementation order

This is one foundation delivery with seven internal gates, not seven partial
editor redesign releases:

1. Freeze semantic/document and display-list contracts with tests.
2. Seal WGPU and remove Egui from the future native host boundary.
3. Replace typography and icon resource pipelines.
4. Implement measure/arrange, clipping, scrolling and virtualization.
5. Complete normalized input, focus, accessibility and global overlays.
6. Complete workbench docking/resize and definitive component recipes.
7. Run the full golden DPI/theme/backend/input matrix and repair every failure.

Only after all seven pass should the new Game/Electronics shell be composed in
one coherent implementation pass.

## Future differentiation

These are enabled by the foundation but are not part of the first rebuild:

- one semantic UI runtime for editor tools and user-authored in-game UI;
- deterministic input recording/replay and visual time-travel for bug reports;
- command semantics shared by pointer, keyboard, console, automation and
  assistive tooling without duplicate handlers;
- live device/DPI/theme simulation from a headless snapshot command;
- performance budgets visible per surface: layout time, paint time, atlas
  pressure, batches and invalidation cause;
- hot-reloadable declarative UI documents validated against typed actions;
- native accessibility adapters built from the same semantic tree;
- backend parity across WGPU transition, owned native GPU backends and CPU
  recovery without rewriting product surfaces.

## Settings

The deleted Settings surface is intentionally not restored during this
foundation pass. Its data/actions must be inventoried from Git history and
reconnected later using the definitive primitives. The old Settings visual tree
is not a permanent implementation candidate.

## Per-control theming

Every surface may change the color of an individual control without creating a
new renderer path. The existing `UiStyle` plus ordered `UiStyleSheet` supports
ID, class and kind selectors, and `UiStylePatch` can override fill, border,
text and opacity. The foundation keeps this contract and adds semantic theme
tokens as the preferred default: a panel can use `surface`, one selected row
can use `selection`, and a warning icon can use `warning` without hardcoding a
new palette into its builder. Per-control overrides are opt-in, bounded by the
theme, and must preserve contrast and the active-edge identity.

This is deliberately small: themes remain data, surface builders remain
simple, and changing one component's color does not fork GPU/CPU rendering.

## Design pre-flight

- Aesthetic coherence: pass. Technical/utilitarian, canvas-first and restrained
  orange are locked.
- Identity: pass. The active edge and fine structural contour remain the
  recognizable signature.
- State coverage: pass at specification level. Pointer, keyboard, resize,
  overlay, DPI, theme, backend and recovery states are explicit.
- Accessibility: pass at specification level. Semantics, focus scopes, IME,
  keyboard parity, high contrast and reduced motion are required.
- Architecture: pass at specification level. RafUI, ApiGraphicBasic, backend,
  host and product ownership are separated.
- Visual proof: pending by definition. It is satisfied only by the Gate 7
  golden matrix and manual review after implementation.
