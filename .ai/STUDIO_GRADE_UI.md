---
name: Studio Grade UI
version: 1.0.0
status: active
scope: product-wide visual defaults for AuraRafi and RafUI
authority: visual-defaults
---

# Studio Grade UI

This is AuraRafi's visual product guide. It defines the default visual language,
interaction quality bar, and the way design references are translated into a
real interface. It is intentionally a default system, not a replacement for
the technical contracts in `docs/RAF_UI.md`, `docs/EDITOR_RAFUI.md`, or
`.ai/APIGRAPHICBASIC.md`.

## How to read this document

The levels below are deliberate:

- **MUST**: technical, accessibility, truthfulness, and performance constraints.
- **DEFAULT**: the visual language used when a task does not provide a more
  specific direction.
- **ALLOWED WHEN BRIEFED**: visual choices that may differ when the user gives
  a screenshot, reference, or explicit design direction.
- **EXPERIMENTAL**: a capability that needs a bounded implementation and
  measurements before becoming a default.
- **HISTORICAL**: evidence only. It must not guide a new implementation.

An explicit current visual brief may override a DEFAULT or an ALLOWED WHEN
BRIEFED choice. It may not override a MUST constraint. The agent should make a
reasonable design decision and continue; it should ask only when the missing
information would change scope, data ownership, safety, or a materially
different product behavior.

## Authority and precedence

For a UI task, use this order:

1. The current user brief, supplied screenshots, and accepted task-specific
   design decisions for visual intent.
2. Repository architecture, ownership, accessibility, localization, and
   performance rules.
3. The active RafUI and ApiGraphicBasic technical contracts.
4. This guide for visual defaults and quality criteria.
5. A design skill for the working method and documentation format.
6. Archived documents for historical context only.

Screenshots are evidence of composition, hierarchy, density, material,
spacing, and interaction affordance. They are not permission to copy branding,
proprietary assets, fake data, or unrelated product behavior.

## Product design read

AuraRafi should feel like a serious creation instrument: calm while idle,
precise while editing, dense where information matters, and expressive only
when the interface communicates state or agency.

The default signature is a restrained warm-orange active edge or action. It is
not a requirement that every screen use orange, and it must never become a
large decorative fill when a quieter state treatment communicates better.

Game and Electronics share the shell vocabulary, spacing logic, and interaction
quality bar. They do not share domain content blindly. Game surfaces prioritize
scene, hierarchy, assets, nodes, and agent workflows. Electronics surfaces
prioritize schematic, PCB, navigator, inspector, DRC, and simulation workflows.

## Default visual language

These are DEFAULT choices, not immutable identity locks:

- semantic neutral surfaces with clear elevation and one-pixel structural
  separation;
- warm orange reserved for active, focused, selected, warning, or primary
  command states;
- dark and light themes with equivalent hierarchy and readable contrast;
- the bundled Ubuntu family and the existing RafUI text roles unless a task
  explicitly requests a different type direction;
- compact controls with intentional whitespace between semantic groups;
- a coherent high-density icon family, sourced from RafUI semantic icons or
  high-resolution project assets;
- no visual effect whose only purpose is to look busy.

Use `UiTheme`, `StudioUiPalette`, and semantic component recipes as the code
defaults. Do not scatter raw colors or one-off component palettes through
surface builders.

The following are also DEFAULTS, not absolute bans: gradients, stronger color
systems, rounded treatments, large display type, glass-like materials, and
non-industrial compositions. If a brief asks for one, define its scope,
purpose, fallback, and accessibility impact before using it.

## Hierarchy and layout

MUST:

- establish one focal task per view;
- make labels, values, secondary information, and actions visually distinct;
- reserve space using RafUI's actual intrinsic layout contracts;
- keep controls targetable and readable at narrow widths and high DPI;
- use one intentional scroll owner for a long content region;
- keep overlays in the global overlay coordinate space;
- preserve the distinction between renderer-owned canvases and RafUI chrome.

DEFAULT:

- use rails, command rows, contextual panels, and bounded property groups;
- prefer tables, aligned rows, and lists for technical data;
- use cards only when elevation communicates a real grouping;
- avoid nested containers that add visual weight without adding meaning;
- preserve a compact, stable row height for repeated controls.

When a screenshot establishes a different hierarchy, match the screenshot's
reading order and density while retaining the actual RafUI layout and input
contracts.

## Materials and translucency

The default material is opaque. A bounded alpha-only translucent material is
ALLOWED WHEN BRIEFED for chrome such as menus, popovers, toolbars, tooltips,
floating panels, and modal surfaces.

Translucency MUST follow these rules:

- it changes the surface material, not the legibility of text or functional
  icons;
- it uses a semantic theme token, not an arbitrary alpha at each call site;
- it does not require backdrop blur, per-panel framebuffer copies, or a second
  renderer in the initial beta;
- it is never used to make the viewport, schematic, PCB, text, or data harder
  to read;
- high contrast and an explicit reduce-transparency preference resolve it to a
  readable opaque treatment;
- the GPU and CPU paths retain equivalent visual semantics;
- ApiGraphicBasic owns blending, target composition, memory, and budget policy.

Backdrop blur is EXPERIMENTAL. It requires a bounded region, a shared scratch
resource, an explicit cost budget, and GPU/CPU fallback evidence. It is not a
prerequisite for a polished interface.

The active alpha-only beta uses `UiSurfaceMaterial` with `Opaque`,
`TranslucentChrome`, `TranslucentRaised`, `ModalSurface`, and `BackdropScrim`.
Authors request the semantic role on `UiNode`; ApiGraphicBasic resolves theme,
high-contrast, reduce-transparency, and constrained-budget behavior before the
shared CPU/GPU draw list is built. Do not recreate these alpha policies in a
surface.

## Interaction quality

MUST:

- every actionable control has a visible state, focus path, keyboard behavior,
  and truthful disabled or invalid treatment;
- menus, popovers, dropdowns, modals, and drag previews support deterministic
  open, focus, dismissal, and restoration behavior;
- forms expose validation near the field and preserve partial input;
- text overflow has an intentional policy: wrap, clip, ellipsis, scroll, or a
  bounded expansion;
- motion explains a state change, has a destination, and honors reduced motion;
- user-visible text comes from the bilingual i18n catalogs.

DEFAULT:

- use inline editing and contextual actions when they reduce interruption;
- use restrained transitions for opening, selection, dragging, docking, and
  layout changes;
- keep primary actions visually clear without turning every action into an
  accent button.

## References and creative direction

When the user supplies a screenshot or visual reference:

1. identify the visual facts: hierarchy, spacing, density, color behavior,
   surface treatment, typography, states, and interaction cues;
2. separate those facts from brand assets, copy, and unsupported behavior;
3. reproduce the intended reading experience in the project's own components;
4. call out only real conflicts with MUST constraints;
5. record a deliberate exception when a DEFAULT is intentionally changed.

The agent must not reject a reference merely because it differs from the
default palette or material language. It must also not claim that a visual
reference is implemented until the corresponding behavior exists in the host.

## RafUI and graphics boundary

RafUI documents describe presentation. Surfaces emit typed actions; hosts and
domain modules validate, mutate, persist, and report confirmed state. Follow
the active technical contracts for layout, input, overlays, menus, and
ownership.

ApiGraphicBasic is the public graphics owner. WGPU is a private execution
adapter, and CPU is recovery, testing, headless, or incompatibility support.
This guide does not authorize a second renderer, a second scene/CAD model, or
backend-specific types above the graphics boundary.

## Accessibility and performance quality bar

MUST:

- preserve readable contrast in dark, light, high-contrast, and translucent
  states;
- make keyboard focus visible and navigation complete for the control type;
- preserve logical layout and physical-pixel sharpness across supported DPI;
- avoid idle work, unbounded text or image allocations, and unnecessary full
  surface rebuilds;
- validate GPU presentation and CPU recovery where the surface uses both.

For a native RafUI surface, describe platform semantics and keyboard behavior;
do not require web-only ARIA, URL state, analytics, or touch rules unless the
target platform actually uses them.

## Review checklist

Before calling a design or implementation complete, verify:

- the brief and the chosen visual direction are explicit;
- defaults and intentional exceptions are identified;
- normal, hover, focus, active, disabled, invalid, open, empty, loading, and
  overflow states are covered where relevant;
- dark/light, high DPI, narrow layout, keyboard-only, reduced-motion, and
  reduced-transparency cases are considered;
- no fake product data or unsupported interaction was introduced;
- layout diagnostics and focused tests cover geometry and interaction;
- runtime or manual visual review is reported separately from compilation.

## Documentation lifecycle

This guide is the active visual default. Feature-specific briefs may live under
`.ulpi/design/` while they are useful and must link here. They are not a second
global identity and cannot silently redefine architecture.

Documents under `.ai/archive/`, `docs/archive/`, and
`.ulpi/design/archive/` are HISTORICAL unless an explicit task asks to compare
them. `docs/STABILIZATION_STATUS.md` remains a project record and is not edited
or used as the visual authority by this guide.
