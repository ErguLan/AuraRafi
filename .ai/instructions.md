# AuraRafi — AI Prompt & Coding Instructions

This file defines the strict, non-negotiable rules for code quality, behavior, and modular design. All AI Agents must adhere to these policies.

## 1. Syntax & Language Standards
* **NO EMOJIS IN CODE**: Do not use graphical emoji characters in code files, comment lines, or structural documentation.
* **ENGLISH ONLY FOR SOURCE CODE**: Write all code, structural variables, function names, types, comments, and internal documentation in English.
* **i18n STRINGS MULTILINGUAL RULES**: Never use inline string localizations like `if is_es { "Texto" } else { "Text" }`. All UI strings must be routed through `t("namespace.key", self.lang)` mapping. Every new key added must have entries configured in both:
  * `crates/raf_core/locales/en.json` (English)
  * `crates/raf_core/locales/es.json` (Spanish)

---

## 2. Structural & Architectural Modularity
* **SLIM app.rs AND RAFUI-FIRST PROTOCOL**: Do not write raw drawing calls,
  menu trees, or panel-specific business logic directly inside
  `crates/raf_editor/src/app.rs`.
  * `app.rs` acts as a route coordinator, state registry, command boundary,
    persistence owner, and auto-save controller.
  * New editor surfaces belong in a focused `*_surface.rs` document builder and
    a `*_surface_host.rs` action/presentation host. The document owns semantic
    nodes, layout, classes, text keys, and event bindings; the host maps typed
    actions to existing backend behavior.
  * Egui panels may remain as temporary body/presentation adapters during the
    migration, but no new long-lived chrome, menus, or renderer-owned canvas
    should be designed around Egui widgets.
  * Follow `docs/RAF_UI_AUTHORING.md` before adding a RafUI surface, control,
    overlay, dock, scroll view, or menu.
* **COMMAND BUS MUTATIONS**: All modifications to scene assets or schematic shapes must register actions to the `CommandBus` or execute transactional snapshots to sustain the Undo/Redo stack. Avoid silent global state mutations.
* **PERSISTENT CONFIGURATION SETTINGS**: New persistent variables must be declared under `EngineSettings` in `crates/raf_core/src/config.rs` featuring appropriate `#[serde(default)]` serialization overlays.

---

## 3. Manual `/` Console Commands & Tools Consistency
* Every new core action must be linked to its manual Console slash command mapped inside `docs/COMMANDS.md`.
* Console command executes should return proper `CommandOutput` containing:
  * Title block.
  * Informational debug message lines.
  * Structured JSON payloads.
  * A boolean `changed` flag.

---

## 4. Feature Development Flow & Verification
* **COMPLETE IMPLEMENTATION BEFORE VERIFICATION TEST**: When any feature is requested, first program everything completely across all necessary modules and files. Implement all logical branches, structures, and tests.
* **RUN TESTS ONLY AT THE VERY END**: Do not run checks or test executions midway. Only run `cargo test` (or cargo checks under specific request) *at the absolute end* of the complete implementation process to verify system integrity.
* **DO NOT AUTO-FIX OR TOUCH UNRELATED COMPILER ERRORS**: If a compilation error is encountered from unfinished user work or unrelated code sections, **do not attempt to fix it, modify it, or run automatic repairs**. Report comments cleanly and preserve the files exactly as they are.

---

## 5. Vision Triage Protocol
When presented with a visual reference, screenshot, or UI diagnostic page without textual instructions:

* **Triage Layouts**: Inspect matching panel alignments, tabs selections, and menu balances.
* **Diagnose Compilations**: Look for build errors or warnings printed inside terminal panes, console windows, or log outputs.
* **Examine Surfaces**: Verify depth-sorting overlap/interpenetrations, coordinate lines, pad footprints, or cable tracing gaps.
* **Auto-Triage Rule**:
  > **"When I send you an app shot with no context, try your best to figure out what you want me to do with it, diagnose any layout alignment issues, active panel discrepancies, compile errors inside the console or visual bugs in the viewport grid/schematic, and update your appshot triage skill based on what you see."**

---

## 6. ApiGraphicBasic Controlled Hybrid Protocol

All renderer, viewport, CAD, RafUI presentation, shader, GPU asset, and backend
work must load and follow `.ai/APIGRAPHICBASIC.md`.

* **ONE GRAPHICS OWNER**: `ApiGraphicBasic` owns the public graphics contract.
  WGPU is the current adapter/compatibility backend, not an API exposed to
  editor surfaces or documents.
* **CAPABILITY-BY-CAPABILITY MIGRATION**: Move complete responsibilities behind
  Rafi-owned contracts over time. Do not schedule a blind numbered rewrite or a
  big-bang WGPU deletion.
* **ONE BACKEND PER EXECUTION PATH**: Do not mix WGPU and native resources in a
  frame without an explicit, measured interop design.
* **PRESERVE UPPER LAYERS**: Native backends must not duplicate viewport, CAD,
  RafUI, scene, document, or command logic.
* **MEASURE BEFORE DEFAULT SWITCHES**: A native backend becomes default only
  after parity, memory, pacing, idle, recovery, and hardware validation.
* **NO STRONG PROGRAMMING BY IMPLICATION**: Documentation of the hybrid plan is
  not authorization to implement a native backend or remove WGPU.

---

## 7. RafUI Design And Interaction Rules

All new retained UI work must follow `docs/RAF_UI_AUTHORING.md` and
`.ulpi/design/DESIGN.md`.

* **ONE AUTHORITATIVE MODEL**: A RafUI document describes presentation only.
  Scene, CAD, asset, project, and provider state remain in their established
  backend models. A control emits a typed action, then the application/command
  boundary validates and applies it.
* **MENU CONTRACT**: Use a focusable trigger, `UiAction::OpenMenu`, and an
  elevated `UiNodeKind::Menu`. A host must clamp the menu, close it on Escape,
  outside click, accepted command, or invalid target, and preserve keyboard
  navigation. Never model a menu as a permanently present card.
* **APPLICATION MENU CONTRACT**: File/Edit/View/Project/Help use
  `UiApplicationMenu`, stable command IDs, and one application dispatcher.
  The eframe bar is a temporary fallback. A native platform adapter returns
  `UiMenuActivation` values and never owns business logic. Do not duplicate
  the menu tree per host or platform.
* **CANVAS CONTRACT**: Viewport, Schematic, and PCB stay renderer-owned
  surfaces. RafUI may own surrounding chrome and overlays, but a minimap,
  selection, wire/traces, and status must read the same live domain document
  and transform as their canvas.
* **DESIGN TOKENS ONLY**: Use semantic `UiTheme` / `StudioUiPalette` tokens,
  stable spacing, and the approved radius/elevation scale. No per-panel raw
  colors, blue/purple gradients, fake glass, decorative blur, generic
  dashboard widgets, or invented product/account data.
* **WINDOW AND PANEL RULES**: Keep OS titlebar behavior native. Fixed rails,
  top command rows, center renderer surfaces, and fixed bottom docks are not
  generic scroll views. Use one intentional scroll container per long content
  region.
* **QUALITY BAR**: Build high-DPI targets from logical layout and physical
  pixels; use the shared text atlas and coherent high-density icons. Never
  upscale a tiny bitmap as a UI icon. Verify dark/light, compact/regular,
  keyboard focus, GPU host, and CPU recovery before declaring a surface done.
* **RETAINED WORK BUDGET**: `UiSurfaceFrame` is layout/input data and
  `UiSurfaceDrawList` is the only paint payload. Cache both by their actual
  invalidation inputs; retain buffers and deduplicate image uploads. Preserve
  paint order and batch only compatible adjacent GPU work.
* **OVERLAY OWNERSHIP**: A tooltip, menu, popover, modal, or drag preview may
  be semantically owned by a surface, but its placement is resolved in the
  global window coordinate space through `raf_ui::overlays`. Do not solve
  clipping with an egui painter or a larger owner surface.
* **INTRINSIC LAYOUT**: Use `UiSizeMode::FitContent` and the resolved text
  atlas for content-sized controls. Hardcoded widths are allowed only for
  structural rails, stable toolbars, and explicit design constraints.
* **MODULE BOUNDARY**: Tooltip recipes, overlay placement, motion, components,
  diagnostics, and density helpers live in their owning modules. Do not grow
  `app.rs`, a surface bridge, or a single panel file with cross-cutting UI
  infrastructure.
