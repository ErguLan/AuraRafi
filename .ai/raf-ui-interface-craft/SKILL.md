---
name: raf-ui-interface-craft
description: Create, refine, and migrate ProyectRaf editor interfaces with Rust-native RafUI, especially downbars, tool groups, Console, Assets, Project, status bars, and retained surfaces. Use when an interface must match a screenshot, when an Egui panel is being migrated, when visual density or overlap bugs need correction, or when a new RafUI surface must preserve the editor architecture.
---

# RafUI Interface Craft

Use this guide whenever changing editor chrome in ProyectRaf. Treat the interface as a retained Rust document with an application host, not as HTML/CSS and not as a second scene or CAD implementation.

## Establish context

1. Read `.ai/SYSTEM_TRUTH.md`, `.ai/instructions.md`, `.ai/APIGRAPHICBASIC.md`, `docs/RAF_UI.md`, and `docs/EDITOR_RAFUI.md` before editing.
2. Inspect the current host, surface builder, backend model, locale keys, and the executable that will be tested.
3. Separate three things: current code contract, historical Egui behavior, and screenshot/design intent. Recover behavior from deleted files only after understanding the current surface.
4. Preserve unrelated worktree changes and do not restore decommissioned panels wholesale.

## Keep the RafUI boundary

Use this split for every surface:

- `*_surface.rs`: declarative `UiNode` tree, layout, styles, icons, text keys, and typed event bindings.
- `*_surface_host.rs` or host module: bridge lifetime, input mirroring, action dispatch, scrolling, and placement.
- backend model: console entries, project data, selection, commands, and persistence; do not put domain mutation in the surface builder.
- Egui/eframe: legacy compatibility placement only while the native Winit host
  is being migrated. Never add new UI behavior, layout logic, or interaction
  semantics to Egui. The target implementation is RafUI plus the native Winit
  window host.

Give every interactive node a stable semantic ID. Emit `UiAction` commands or typed control actions and let the host apply them. Keep source code and locale keys in English; provide English and Spanish locale values.

## Visual contract

Use the existing RafUI industrial language:

- dark semantic surfaces, Ubuntu typography, compact one-pixel structural edges;
- warm orange only for active, focus, warning, or primary action states;
- clear hierarchy through spacing, weight, and surface levels;
- no gradients, glow, glassmorphism, decorative blur, fake data, or emoji in source;
- reuse semantic `UiIconId` recipes before creating PNG assets; generate raster icons only when a real icon shape is missing.

Match the supplied composition before inventing new features. A screenshot is a layout and density reference, not permission to recreate every old panel.

### Motion is part of the default interaction contract

Prioritize purposeful transitions for menus, selection, drag/reorder, docking,
panel creation/removal, and structural layout changes. Use shared
`UiTween`/`UiMotionSpec` primitives, keep transient targets in the host, and
keep surfaces declarative. Preserve reduced-motion behavior and reject
decorative always-on animation. Every transition needs a performance review of
GPU/CPU frame time, allocations, texture/atlas work, and idle repaints.

## Dense layout rules

- Give compact toolbar controls explicit widths when their labels must remain visible. Do not rely on `fit_content()` for a row of controls that can collide.
- For a vertical scroll list, make entries stretch across the cross-axis. Otherwise text can receive a tiny width and wrap one word per line.
- Use fixed row heights for tabs, log rows, asset rows, and tree rows; keep padding and gaps intentional.
- Use `grow` for the main flexible content and `UiAlign::Stretch` for the cross-axis. Set `min_size[0]` to zero on flexible text nodes so a narrow panel can shrink without overlap.
- Use `FitContent` width for natural single-line labels such as tabs and status items. For a vertical text row that must fill its panel, use `width_mode: Fill` and `height_mode: FitContent` (or the equivalent builder chain). Fit the row's height, not its cross-axis width.
- Treat icon, gap, and horizontal padding as part of a text row's measurement budget. Reserve them before asking the atlas for intrinsic text size; otherwise the label can wrap into a narrow column even when the panel has room.
- Keep ordinary tree/list labels single-line by contract. If wrapping is intentional, the measured line count must determine the row height before the next sibling is placed; never paint multiline text inside a fixed-height row.
- Use monospace text for timestamps, command output, coordinates, and JSON. Keep human labels in the normal UI role.
- Add a layout test when a bug involves overlap, zero-width children, clipping, or a resize boundary. Assert rectangles, not just node existence.
- When a panel becomes narrow, preserve interaction and hierarchy first; clip or scroll secondary text rather than letting controls paint over each other.
- Treat `UiLayoutBox.content_rect` as the only paint origin for text, icons, and caret geometry. Do not compensate for padding with per-panel `x` offsets.
- Keep text requests bounded by the content box and clip their quads to that same box. A row can be correctly laid out and still look broken if the painter ignores its content bounds.
- Focused text inputs must retain `UiTextEditState` when hosts seed the same value again. Draw the caret from atlas metrics so it stays aligned with the actual glyph advance.
- Retained interaction exposes semantic cursor hints: `Text` for text inputs and `PointingHand` for actionable controls. Native bridges translate the hint; surfaces do not depend on egui cursor types.

## Application bar and settings contract

The editor shell is split into a RafUI application bar and a RafUI downbar.
The application bar currently exposes `File`, `Edit`, `View`, `Project`, and
Help, plus the editable command search, global Settings, and native window
commands. Project Settings is a downbar tab, not a second top-bar action.
Do not add Build, Play, or runtime-status controls until the runtime contract
is explicitly connected.

Keep the menu tree in a renderer-neutral `UiApplicationMenu`. Its command IDs
must be stable, and the menu must be filtered by `ProjectType`: Game menus do
not advertise electronics commands, and Electronics menus do not inherit Game
tools accidentally. Recover old command IDs when useful, but do not bring back
RafUI Studio or obsolete panels as hidden dependencies.

Engine Settings and Project Settings are separate drafts and persistence
boundaries:

- `settings_surface.rs` owns the seven global sections: Appearance,
  Performance, Editor, Viewport, Scripting, AI, and Platform.
- `settings_surface_host.rs` owns draft mutation, validation, save/cancel, and
  `aura_rafi_settings.ron` persistence.
- `project_settings_surface.rs` owns the six project sections: Overview,
  Layout, Runtime, Saving, Scripting, and Graphics. Its header keeps the
  project name beside the `Project Settings` title and has no Cancel action.
  The Layout section must expose a real `Reset Panels` action.
- `project_settings_surface_host.rs` owns project mutation and immediate
  accepted-value persistence to `project.ron`; Project Settings is not a
  second draft boundary.

Never put persistence or domain mutation in the surface builder. Every control
needs a stable value key, and segment controls must receive a real label key;
generic placeholders such as app.value are a common source of misleading UI.

### Live-control rules

Controls are not complete when their action exists; they must also be visible,
focusable, and seeded from the current model:

- UiNode::toggle and UiNode::range require core RafUI presentation geometry.
  Do not draw their tracks or thumbs as per-surface rectangles.
- Range settings that users must type need a sibling UiNode::text_input with
  a distinct .text value key. The slider remains the fast pointer/keyboard
  path; the text field is the precise path.
- Hosts seed a field only when UiControlState::has_text is false. Reseeding
  every frame destroys partial edits such as an intentionally empty numeric
  field and makes typing feel broken.
- Apply typed numeric values only after parsing and clamping at the host
  boundary. Invalid intermediate text remains in the transient control state
  until the user completes it; it must not be overwritten by the old model
  value on the next frame.
- A focused text field must keep its caret state across document rebuilds.
  Pointer focus, keyboard text, backspace/delete, Ctrl/Cmd+A, arrows, Home,
  End, and submit behavior are part of the same control contract.
- The command search may be visually inactive while its command backend is
  pending, but it must be a real text input or be removed. Never ship a
  decorative fake input with a shortcut label.

## PNG icon provenance

Use semantic `UiIconId` recipes only when the shape is part of the engine's
built-in vocabulary. For application-bar and editor-specific art, prefer the
repository's generated PNGs: keep the painter in
`tools/ui_assets/generate_editor_icons.py`, generate to
`editor/assets/ui_icons/`, and embed the resulting bytes in the host. The
PowerShell generator is a local fallback when Python is unavailable. These are
project-owned assets, not downloaded icon packs or Font Awesome resources.

## Console and Output contract

The beta downbar uses one `Console` group. Output is not a second tab: engine logs, user submissions, warnings, errors, and command results all enter the same `ConsolePanel` stream.

Keep these behaviors when changing the visual surface:

- clear entries, level filters, auto-scroll, command submit, autocomplete, and command history;
- command results may expose a bounded JSON toggle without expanding every row by default;
- timestamp, sender, and message occupy separate visual columns so they cannot overlap;
- command output lines remain readable and aligned inside the console width;
- the input remains disabled when the existing project/settings contract disables commands.

## Downbar interaction contract

Keep the downbar model in raf_ui::docking and the editor host responsible for
actions. The default document is one ordered group containing the currently
supported tabs. A tab can be dragged into another group or to a group edge to
create a split, with a hard maximum of three groups. Empty source groups are
removed immediately and the remaining tracks reflow; an empty group is never a
drop target or a fake surface. Adjacent groups may be resized through splitters.
While dragging, reorder tabs from horizontal pointer movement and render a
transient orange/purple drop preview before committing on release. The preview
must never become persistent state by itself.
Never remove the source tab node from the retained surface while its pointer is
captured. RafUI captures the drag actions at pointer-down so `DragMove` and
`DragEnd` still reach the host if the surface is rebuilt during the gesture;
the source may be rendered as a dimmed ghost instead.

Persist the layout per project, including tab order, active tab, group weights,
height, and collapsed state. Store dock metadata in a project-local editor
layout file rather than coupling raf_core project metadata to RafUI types.
Load and normalize the file when the project changes, discard unsupported tabs,
deduplicate IDs globally across groups, restore missing supported tabs and the
preferred active tab, persist any repair, and fall back to the one-group default
when the file is absent or invalid. This prevents duplicate icons, a tab body
under the wrong tab, and tabs disappearing after a split.
When the serialized dock version is older than the current contract, discard
that stale arrangement once and persist the clean default instead of trying to
repair interaction state that no longer matches the host.
Project Settings must route its `Reset Panels` action to the dock host; it
resets only the active project's panel layout and does not alter scene,
project-setting, console, or session data.

Do not invent Project tree rows, sessions, assets, logs, Node Editor panels, or
Agent panels. Build them from real application state and show a truthful empty
state when the source is empty.

## Known visual defects to track

Keep this list current. Move an item to resolved only after a focused test and a fresh executable screenshot.

- [x] Output and Console rendered as one Console tab.
- [x] Downbar body uses the actual group bounds and is no longer a black zero-height surface.
- [x] Adjacent downbar boundaries accept horizontal resize with minimum widths.
- [x] Console tabs expose semantic drag actions and the host can move them between groups.
- [x] Tab drag preview supports horizontal reorder and a transient split target; commit only on release.
- [x] Persisted dock repair deduplicates tabs globally and removes empty source groups after a split.
- [x] RafUI pointer capture survives a rebuilt drag source and clears stale gestures on layout reset.
- [x] Console toolbar controls overlap when compact sizing is left implicit (`Clear`, `Auto-scroll`, and filters). The surface now reserves fixed control widths and explicit gaps; keep this regression test active.
- [x] Console timestamps, senders, messages, and command lines wrap into misleading vertical word stacks. The surface now uses separate columns, stretch rows, bounded text tracks, and monospace output; verify once more in a fresh executable screenshot.
- [ ] Console rows lose readable density near the minimum downbar height/width. Fix with fixed row heights, bounded JSON expansion, and scrollable secondary content.
- [ ] Assets rows are too basic or inconsistent in icon, label, row height, and hover treatment. Fix with semantic file-type icons and a shared row class.
- [ ] Project tree rows use fragile leading spaces or collide text/icons. Fix with depth-based padding, stable row classes, and explicit hierarchy styling.
- [ ] Status bar and downbar need a final DPI and narrow-window pass.
- [ ] Validate tab dragging and splitter resizing in a fresh running executable, not only through unit tests.
- [x] RafUI paint contract now carries a content rect, clips text/icons to it, and keeps nested text rows on their measured tracks. Code-level fix and renderer geometry tests are complete; fresh executable screenshot remains.
- [x] Textbox interaction now has a default hover lift, semantic cursor hints, caret rendering, and caret-preserving control seeding. Code-level fix and RafUI interaction/style/state tests are complete; fresh executable screenshot remains.
- [x] Settings segment rows use explicit labels and no longer rely on a generic `app.value` placeholder. Keep a narrow-window screenshot in the visual QA pass.
- [x] Native window intent contract covers drag, eight-way resize, minimize, maximize, close request, and system-menu request. Native event-loop integration remains a separate migration checkpoint.
- [x] Downbar defaults to one ordered group, supports edge split/move gestures up to three groups, removes empty sources, and persists layout per project. Keep fresh executable drag/reload QA active.
- [x] Project Settings is hosted as a real downbar tab; the top bar keeps only global Settings.
- [x] Window-control tooltip keys resolve to English and Spanish labels instead of leaking translation IDs.
- [x] RafUI paints retained toggle/range geometry, and Settings exposes editable numeric fields without per-frame reseeding.
- [x] The application command search is an actual retained text input; its execution backend can remain unmounted without pretending it is functional.
- [x] Project rows are derived from the active project filesystem; unsupported Node Editor/Agent placeholder panels are not part of the default dock.
- [x] Each visible downbar group owns an independent retained surface bridge and texture; splitting a dock no longer paints the last group's toolbar into every group.
- [ ] Profile dock/menu transitions on GPU and CPU fallback across idle, hover, drag, split, DPI, and reduced-motion paths; optimize invalidation, caching, or repaint frequency if measurements show a regression.

## Verification loop

1. Format with `cargo fmt --all`.
2. Run focused tests for `raf_ui` and `raf_editor`.
3. Run `cargo check -p raf_ui -p raf_editor -p aura_rafi_editor`.
5. Start a fresh executable and inspect the application bar, Settings, Project Settings, and downbar at normal, narrow, and resized heights.
6. Drag tabs into the center and both edges, verify no empty source remains, verify the three-group cap, reload a different project, and confirm each project restores its own layout.
7. Compare against the current screenshot: menu density, panel boundaries, label alignment, log readability, active state, and status density.
8. Profile non-trivial transitions for CPU/GPU frame time, allocations, texture/atlas uploads, and idle repaint behavior; include reduced-motion and CPU fallback.
9. Run git diff --check and report files, commands, evidence, and remaining visual limitations. Do not claim a visual fix from compilation alone.

## Updating this guide

When the user asks to update the RafUI interface workflow, edit this `SKILL.md` in place. Add the new failure mode to `Known visual defects`, update the relevant contract or rule, and only mark it resolved with current code plus validation evidence. Keep the file under 500 lines and avoid creating auxiliary README, changelog, or quick-reference files.
