# RafUI Authoring Guide

This guide is the practical contract for authoring a RafUI surface. Read it
before adding a new retained editor surface, game UI document, dock, menu, or
control.

RafUI is a Rust-native retained document system. It is not HTML, CSS, a DOM,
or a browser. The familiar ideas are intentional: `UiNode` is a small tree,
`UiLayout` is CSS-like layout data, and `UiStyleSheet` is a deterministic
cascade. The implementation remains serializable Rust data rendered by
ApiGraphicBasic.

For renderer and ownership boundaries, see [Architecture](ARCHITECTURE.md).
For theme and interaction rules, see [RafUI](RAF_UI.md) and
[Editor Shell Migration](../.ulpi/design/editor-shell-migration.md).
The former RafUI Studio helper was removed with the editor chrome. The design
document remains historical context only until the next workbench is defined.

## Current authoring boundary

The active editor shell is being rebuilt with RafUI surfaces. New work belongs
to the current canvas-first shell and must not restore the removed RafUI Studio
helper, obsolete inspector recipes, or decommissioned Egui panels wholesale.
The temporary eframe host may place a surface, but it must not own new layout,
control, or interaction semantics.

## Non-Negotiable Ownership

| Layer | Owns | Must not own |
| --- | --- | --- |
| `UiDocument` | semantic tree, classes, layout, style, localization keys, event bindings | scene/CAD mutation, provider calls, file writes, per-window hover/focus |
| `UiSurfaceSession` / `UiControlState` | input, focus, hover, active state, text buffers, scroll offsets | project persistence, document model mutation without a host action |
| Surface host | render target, texture lifecycle, hit-test dispatch, typed action translation | duplicated scene/CAD business rules |
| Application boundary | commands, undo/redo, document mutation, validation, persistence | hand-drawn retained controls |
| ApiGraphicBasic | GPU composition, text atlas upload, CPU recovery composition | application decisions and editor state |

Never make the visual tree the source of truth for a project. A control emits a
typed `UiAction`; the host validates it against the existing backend and
returns a rebuilt view model on the next frame.

## Required Surface Shape

Every production surface follows the same sequence:

1. Receive a compact immutable view model and the active theme/language.
2. Build a `UiDocument` from stable node IDs and i18n keys.
3. Render through a GPU host; use the CPU compositor only as recovery.
4. Dispatch `UiAction` values at one typed boundary.
5. Apply domain mutations through the existing command/undo/persistence path.
6. Rebuild only after model, layout, theme, or asset changes. Hover, focus,
   text editing, and scroll position remain session state.

The center Viewport, Schematic, and PCB are renderer-owned surfaces. RafUI
creates their surrounding chrome and docks, but it never redraws a CAD or 3D
world as generic UI.

For editable controls, seed a value only when the session does not already
contain it. Re-seeding every frame overwrites partial edits and makes a
numeric field impossible to clear or rewrite. Ranges that need precision
should compose a retained slider with a sibling text input and a distinct
value key ending in .text; parse and clamp that value in the host.

RafUI presentation owns the visual geometry for toggles and ranges. A surface
builder declares their metadata and style class; it must not draw custom
tracks or thumbs with renderer-specific offsets. Focused text fields keep
their caret state through surface rebuilds, and the native bridge maps the
semantic Text or PointingHand cursor hint.

## Downbar contract

The editor downbar starts with one ordered group. Dragging a tab into another
group moves it; releasing near a group edge creates a split. The model allows
at most three groups. Moving the last tab out removes the source group and
reflows the remaining columns immediately. Empty groups are not placeholders
or drop targets.

The host persists the serializable downbar layout per project: tab order,
active tabs, group weights, height, and collapsed state. Persistence is kept
in a project-local editor layout file so raf_core project metadata does not
depend on RafUI. On load, normalize empty groups, duplicate tab IDs,
unsupported tabs, and old layouts before rendering.

Project and Assets surfaces consume real filesystem/application view models.
When a source is empty, render a truthful empty state; never add sample
folders, scenes, sessions, logs, Node Editor panels, or Agent panels just to
fill a screenshot.

## Build a Document

Use stable IDs, semantic classes, and text keys. Do not insert English or
Spanish display copy directly in a node.

```rust
use raf_ui::{
    UiAlign, UiDocument, UiFlow, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiSpacing,
};

fn build_toolbar_document() -> UiDocument {
    let mut document = UiDocument::blank("editor.toolbar");
    document.root = UiNode::new("toolbar-root", UiNodeKind::Root)
        .with_class("editor-root")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            grow: 1.0,
            padding: UiSpacing::xy(12.0, 6.0),
            gap: 8.0,
            align_items: UiAlign::Center,
            justify_content: UiJustify::SpaceBetween,
            ..UiLayout::default()
        });
    document
}
```

The root stays structural. A toolbar, rail, inspector, menu, scroll region,
or canvas slot must be an explicit child with its own ID and class. Do not use
empty nested panels only to create spacing.

## Build Menus Correctly

A menu has three separate responsibilities:

1. A trigger emits `UiAction::OpenMenu`.
2. A `UiNodeKind::Menu` is an elevated overlay with a higher `z_index`.
3. The host opens it at a clamped position, closes it on Escape/outside click,
   and maps each item command into a typed application action.

The document describes the menu; it does not execute a command itself.

```rust
use raf_ui::{
    UiAction, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiPositionMode, UiRect, UiSpacing,
};

fn project_menu_nodes() -> [UiNode; 2] {
    let trigger = UiNode::new("project-menu-trigger", UiNodeKind::Button)
        .with_text_key("app.project_menu")
        .with_class("menu-trigger")
        .focusable()
        .with_accessibility_label_key("app.project_menu")
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::OpenMenu {
                id: "project-menu".to_string(),
            },
        });

    let menu = UiNode::new("project-menu", UiNodeKind::Menu)
        .with_class("context-menu")
        .with_layout(UiLayout {
            position_mode: UiPositionMode::Absolute,
            rect: Some(UiRect::new(0.0, 0.0, 220.0, 0.0)),
            flow: UiFlow::Column,
            padding: UiSpacing::same(6.0),
            gap: 2.0,
            z_index: 20,
            min_size: [180.0, 0.0],
            ..UiLayout::default()
        })
        .with_child(menu_command("project-menu-open", "app.open_project", "project.open"))
        .with_child(menu_command(
            "project-menu-duplicate",
            "app.duplicate_project",
            "project.duplicate",
        ))
        .with_child(menu_command(
            "project-menu-remove",
            "app.remove_from_recent",
            "project.forget_recent",
        ));

    [trigger, menu]
}

fn menu_command(id: &str, text_key: &str, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_text_key(text_key)
        .with_class("menu-item")
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}
```

The host contract for every menu is mandatory:

- Only one menu may be active for one surface session.
- Place it against the trigger when possible; clamp it to the surface bounds.
- Close it on `Escape`, any outside primary/secondary click, surface rebuild
  that removes its target, and after an accepted item command.
- Keyboard focus moves through menu items; Enter runs the focused item.
- A disabled item remains visible but never dispatches its action.
- Menus use `UiNodeKind::Menu`, `z_index >= 20`, and the semantic
  `context-menu` / `menu-item` classes. Do not build a menu as a hidden card.

Use a three-dot overflow trigger only for actions local to one repeated item.
Global `File`, `Edit`, `View`, `Project`, and `Help` commands belong in the
shared editor command row, never inside a random domain panel.

### Build An Application Menu Bar

Do not represent the application menu bar as `UiNodeKind::Menu`. Use the
shared `UiApplicationMenu` model so the eframe fallback and a native platform
adapter consume the same command IDs.

```rust
use raf_ui::{UiApplicationMenu, UiMenu, UiMenuCommand, UiMenuItem};

fn editor_menu() -> UiApplicationMenu {
    UiApplicationMenu {
        menus: vec![UiMenu::new("file", "app.file")
            .with_item(UiMenuItem::Command(
                UiMenuCommand::new("project.save", "app.save_menu")
                    .with_accelerator("Ctrl+S"),
            ))
            .with_item(UiMenuItem::Separator)
            .with_item(UiMenuItem::Command(UiMenuCommand::new(
                "project.exit_to_hub",
                "app.exit_to_hub",
            )))],
    }
}
```

Application-menu rules are mandatory:

- Command IDs are stable domain verbs, never translated display text.
- Labels are i18n keys. The platform host resolves them for the active language.
- The app owns the one command dispatcher. A native adapter returns
  `UiMenuActivation`; it does not run persistence or domain mutations.
- The eframe bar is a compatibility fallback, not a native-menu claim.
- Do not create a second File/Edit/View implementation for a platform. Extend
  the shared model and its dispatcher instead.

## Style With Tokens and Classes

Node-local style establishes a base. `UiStyleSheet` applies ordered rules by
ID, class, or `UiNodeKind`; later matching rules win. Use classes for repeated
roles and IDs only for a genuinely singular surface element.

```rust
use raf_ui::{
    UiNodeKind, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet,
};

fn menu_styles() -> UiStyleSheet {
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("context-menu".to_string()),
                UiStylePatch {
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("menu-item".to_string()),
                UiStylePatch {
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Button),
                UiStylePatch {
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}
```

Theme colors must come from `UiTheme::raf_ui()` / `StudioUiPalette` semantic
tokens. Orange identifies active state, focus, or the one primary action.

### Per-control color overrides

Every control can receive a future theme token or a bounded local override by
stable ID, semantic class, or node kind. Use `UiStyleSheet` and
`UiStylePatch` for this instead of adding a renderer branch or hardcoding a
new palette in a surface builder. Fill, border, text and opacity are separate
properties, so changing one icon, row, field or status control does not
recolor the entire surface. Keep contrast and the restrained active-edge
identity intact.
Filled orange commands use dark ink. Blue/purple gradients, decorative glow,
fake glass, and arbitrary per-panel color constants are forbidden.

## Responsive, Scroll, and Docking Rules

- A row that can preserve minimum sizes uses `UiCompactMode::Wrap`; otherwise
  use `UiCompactMode::Auto` and add a `UiResponsiveRule` that stacks controls.
- Use `UiNode::scroll_view` / `UiOverflow::ScrollY` for one intentional scroll
  region. Rails, top command rows, and fixed bottom docks do not scroll.
- Put min/max dimensions in `UiLayout`; do not let labels or hover state change
  a control's geometry.
- Use `UiNode::grid` with automatic `min_column_width` for repeated project,
  asset, or template cards.
- Use `DockPanelPolicy::Fixed` for infrastructure (`center`, `bottom`) and
  `Movable` only for supporting docks. Respect `allowed_dock_sides`.
- A floating panel needs a title bar, z-order, clamp, and lower-right resize
  behavior through `DockWorkspaceController`; do not recreate those gestures
  inside an individual panel.

## Canvas Fidelity Rules

The renderer-backed canvas is the visual source of truth. A minimap, selection
overlay, cable route, and status readout must consume the same scene/CAD model
and visible-world transform as the canvas. Never draw a decorative minimap,
an independently approximated selection, or UI-only cables that disagree with
the underlying document.

For temporary GPU/CPU migration parity, an authoritative low-cost interaction
overlay is acceptable when it is bounded and documented. It must not become a
second CAD/scene implementation. Once the GPU path has feature parity, remove
the duplicate overlay rather than leaving both to drift.

## Review Checklist

Before calling a RafUI surface complete, verify:

- Stable node IDs; no text-derived IDs.
- i18n key exists in both English and Spanish JSON files.
- Dark, Light, System, compact, and regular layouts are covered.
- Keyboard focus, Enter/Space, Escape, outside-click menu dismissal, and
  pointer hit testing work.
- Logical-point input matches physical-pixel target rendering at high DPI.
- Hidden content does not render, hit-test, or request idle repaint.
- An application menu uses `UiApplicationMenu`; context menus use
  `UiNodeKind::Menu`, never the other way around.
- An unchanged surface reports cache reuse and does not upload UI geometry
  again; image and atlas updates remain independently invalidated.
- The host maps actions to existing commands/validation/persistence rather
  than reimplementing backend behavior.
- Viewport/Schematic/PCB remain dedicated renderer surfaces.
- GPU and CPU recovery paths render the same document semantics.

## Frontier Authoring Contract

### Intrinsic controls

Use intrinsic modes when a control's size is derived from its content:

```rust
UiNode::new("save-hint", UiNodeKind::Label)
    .with_layout(UiLayout::fit_content())
    .with_text_key("app.save_hint")
```

Use fixed dimensions for structural rails and toolbars. Do not approximate a
localized label with a guessed width when `FitContent` is appropriate. The
renderer resolves the text key, syncs the bounded atlas, and applies the
measured result before building the final draw list.

### Global overlays

Menus, tooltips, popovers, modals, and drag previews must use the overlay
contract. They may be authored by the owning surface but are composed in a
window-level layer:

```rust
let placement = place_overlay(
    anchor_rect,
    [tooltip_width, tooltip_height],
    window_rect,
    UiPlacement::BottomStart,
    8.0,
);
```

The resolver first uses the requested side, then flips to its opposite side,
then shifts inside the window. Never solve an out-of-bounds overlay by
increasing the owner surface or by painting a second egui widget over it.

### Hover and motion

Hosts must pass monotonic `UiInputState::time_seconds`. Hover transitions are
session events. Visual entry/exit uses `UiTween` and a shared `UiMotionSpec`;
the host requests another frame only while the tween is unsettled. Reduced
motion jumps to the target value and does not keep an idle repaint loop alive.

Purposeful motion is the default for state changes that users read spatially:
menus, selection, drag/reorder, docking, panel creation/removal, and structural
layout changes should preserve continuity with a restrained shared tween. The
host owns the transient target and timing; the surface only describes the
visual state. Do not add decorative perpetual animation. A transition is not
complete until its GPU/CPU frame time, allocations, texture/atlas work, and
idle repaint behavior have been measured, including reduced-motion behavior.

### Component recipes

Use `raf_ui::components` for repeated editor vocabulary:

- `icon_button`: focusable command target with accessibility and tooltip keys.
- `panel_header`: consistent inspector/rail heading geometry.
- `tree_row`: stable hierarchy row contract.
- `tooltip_node`: compact neutral tooltip with the locked RafUI palette.

Recipes return ordinary nodes. Extend them with stable IDs, classes, layout
constraints, and typed actions rather than duplicating their visual constants
inside a panel.

### Diagnostics

Every host frame can expose `UiSurfaceDiagnostics`. A surface with unexpected
zero-sized boxes, excessive clipping, or a changed interactive count should be
investigated before screenshot-based styling. Diagnostics are also valid for
CPU recovery because they are derived from the shared retained frame.

For a fast, repeatable visual gate before opening a native window, use the
ApiGraphicBasic CPU matrix:

```rust
let samples = raf_render::api_graphic_basic::ui_surface::cpu_quality_matrix(
    &surface,
    [1280, 720],
    [14, 16, 20, 255],
    |key| localization.resolve(key),
);
assert!(samples.iter().all(|sample| !sample.diagnostics.has_layout_warnings()));
```

It renders 100%, 125%, 150%, and 200% into the same recovery compositor and
returns physical size, atlas revision, diagnostics, and a stable pixel hash.
Use the hash as a change signal, then inspect the actual image when the signal
changes; it is not a substitute for visual review.

## AuraRafi quality charter

The visual heart of AuraRafi is precision before decoration. Read
`.ulpi/design/aurarafi-ui-quality.md` before approving a surface. In
particular, classify sparkling or crawling icons as pixel shimmer, subpixel
jitter, temporal aliasing, texture bleeding, or resampling blur. Correct the
physical-density and sampling contract first; do not compensate by enlarging
buttons, adding glow, or painting a duplicate widget from Egui.

Review retained surfaces at 100%, 125%, 150%, and 200% DPI in GPU and CPU
paths. A hover or resize may change state styling, but must not make a small
icon change shape, move a one-pixel border, or soften unrelated text.
