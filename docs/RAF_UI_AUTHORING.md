# Authoring RafUI in Rust

Status: current API guide. Source contracts reviewed on 2026-11-08. The examples
below are contributor recipes, not an installed panel or automatically executed
game logic. No application build or native-window acceptance is implied by this
documentation review.

Use [RafUI](RAF_UI.md) for internals and the source map,
[Editor RafUI](EDITOR_RAFUI.md) for editor integration rules, and
[Game interfaces](GAME_UI.md) for creating a player's interface. Historical
authoring material under `archive/` is not the active guide.

## 1. Choose the owner before creating nodes

| You are creating | Authoritative data | Presentation owner |
| --- | --- | --- |
| An editor panel | Existing application/domain view model | A focused surface and its host |
| Reusable game UI | `UiDocument`, serialized in the project | Interface authoring host; independent player instances in Play |
| Scene or CAD graphics | Existing scene/CAD document | ApiGraphicBasic canvas, with RafUI chrome around it |

Put declarative nodes in a focused `*_surface.rs`. Put focus, scroll, menus,
drafts, gesture capture and action translation in its host. Domain commands own
validation, mutation, history and durable persistence. A button must not write
project files from its surface builder.

`UiSurface::new` defaults to the editor theme. `UiDocument::blank` defaults to
the neutral game UI theme. When presenting a saved document, use
`UiSurface::from_document`; reconstructing only its root loses the document's
theme and stylesheet unless they are explicitly carried across.

## 2. Build a small, complete surface

This panel displays a real project name and emits the existing Save action. It
is intended to live inside `raf_editor`, where the application command constant
is available. It uses an existing localized label, measured text sizing and
theme metrics; it does not create a second save implementation.

```rust
use crate::application_menu::command::PROJECT_SAVE;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::*;

pub struct SavePanelView<'a> {
    pub project_name: &'a str,
    pub can_save: bool,
}

pub fn build_save_panel(
    palette: StudioUiPalette,
    view: SavePanelView<'_>,
) -> UiSurface {
    let kit = UiComponents::for_palette(palette);

    let name = UiNode::new("save-panel.project", UiNodeKind::Label)
        .with_text_value(view.project_name)
        .with_text_style(UiTextStyle::body(kit.tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .fit_content().fill_width()
        .map_layout(|layout| layout.grow(1.0));

    let save = kit.button(
        "save-panel.save", "app.save_menu",
        UiAction::Command { name: PROJECT_SAVE.into() },
        UiButtonVariant::Standard,
    ).disabled(!view.can_save);

    let root = UiNode::row("save-panel.root", [name, save])
        .fit_content().fill_width()
        .with_gap(kit.metrics.spacing)
        .with_padding(UiSpacing::same(kit.metrics.spacing))
        .with_alignment(UiAlign::Center)
        .with_style(palette.panel_style());

    UiSurface::new("editor.save-panel", palette, root)
        .with_style_sheet(kit.style_sheet())
}
```

`project_name` is domain data, so it uses `with_text_value`. Interface copy uses
`with_text_key` and synchronized entries in both locale catalogs. Repeated
component instances need scoped node IDs; do not mount this exact ID set twice
without namespacing it.

`UiComponents` resolves theme metrics, measured icon/text composition, focus,
accessibility and state classes. Its button variants are `Standard`, `Quiet`,
`Accent` and `Danger`; it also provides text inputs, Select triggers, property
fields and adaptive toolbars. Mount its stylesheet once and append explicit
author overrides with `with_layer`. Hosts still own Select popup composition
and commands. Custom palettes use `UiComponents::new(tokens, metrics)`; game
content does not need to inherit the editor's accent colors.

Reusable recipes already exist in `raf_ui::components`: form fields, numeric
fields, tabs, tree rows, segmented groups, thumbnails, badges and toolbars.
They return normal nodes. Inspect their parameters, classes and style needs
before introducing another local helper. A recipe is not evidence that every
consumer already uses the same metrics or has all interaction states styled.

## 3. Preserve the container's layout contract

`UiNode::with_layout` replaces the entire layout. It does not merge a patch.
`UiLayout::fit_content()` starts from defaults, including `flow: None`.
Consequently this expression removes the row flow:

```rust
let unsafe_composition = UiNode::row("example.row", children)
    .with_layout(UiLayout::fit_content());
```

Prefer `row.fit_content().fill_width().with_gap(...)`: these node builders
retain flow and anchoring. `map_layout` makes other partial changes explicit.
When a full replacement is intentional, keep `flow: UiFlow::Row` in the supplied
layout. The same distinction applies to columns and `UiLayout::fixed`.

Fit-content tracks retain their natural main-axis size even when a grow weight
is used on the other axis. Flexible compression preserves minimum sizes; an
impossible set of minima overflows instead of creating undersized hit targets.
Intrinsic height follows compact Row/Column/RowWrap behavior, including the
actual distributed width of aspect-ratio images inside rows.

- Use `FitContent` for content-driven text and controls, `Fill` for an allocated
  remainder, and `Fixed` for a deliberate structural or design constraint.
- A fill dimension needs an ancestor that supplies that extent. Intrinsic and
  fill dimensions are not interchangeable.
- Give wrapping text a containing width. Wrapping cannot rescue an unconstrained
  line that is allowed to measure wider than its panel.
- Keep ordinary rows and columns in flow. Absolute rectangles are for canvas
  geometry and overlays; they are not a repair for a broken parent measurement.
- For icon plus text, use the node's icon/content contract so measurement and
  painting share a group. Do not add a guessed text offset per panel.
- `plan_action_bar` takes actual localized text measurements. Declare action
  priorities and overflow policy rather than dropping commands when space ends.

Styles are ordered rules; `UiStyleLayers` can make their base/component/variant/
state/override ordering explicit. Geometry and borders do not inherit by
default. Changing a node's text color does not grant full text-style inheritance.

## 4. Mount the surface and route its actions

The active editor composes panel surfaces in
[`native_workbench_surface.rs`](../crates/raf_editor/src/native_workbench_surface.rs).
Its wrapper provides the logical rectangle and merges the child surface's
stylesheet. Preserve both; mounting only `surface.root` can discard its styles.

The native host and input bridge dispatch `UiDispatchedAction` values through
[`native_workbench_input.rs`](../crates/raf_editor/src/native_workbench_input.rs).
The Save command above already belongs to that shared application dispatcher.
Reuse it; do not add direct filesystem persistence to this panel.

For a genuinely new domain operation, add a typed intent at the established
application/command boundary, including its validation, history and persistence
semantics. Keep identifiers stable and labels localized. Document new core
commands in the shared command catalog and [Commands](COMMANDS.md).

When using a focused host, create its retained UI host from the existing native
graphics context and compose it through `EditorUiLayer`. Reuse the existing
window, input router and graphics owner. A host is not another renderer.

## 5. Keep editing state separate from saved data

`UiSurfaceSession` owns interaction, including `UiControlState`. Seed a text
field from the authoritative value when the field/selection scope changes,
not on every frame. `UiTextInput::submit_command` supplies the commit action.
Validate that draft at the host/domain boundary before changing saved data.

Use `UiNumericEditBank` when its shared numeric editing contract fits the
field. Preserve incomplete drafts, commit on Enter, cancel on Escape and keep
the latest authoritative value separate from the typed string. Specialized
scene and CAD fields retain their domain validation.

For ordinary text properties, `UiTextDraftBank::sync(scope, bindings, controls)`
reconciles `UiTextBinding` values with `UiControlState`. Unedited fields follow
the latest model value; dirty drafts, selection and IME survive refreshes. A new
scope resets owned fields. `cancel` restores the latest model value, while a
rejected domain commit leaves the draft intact. Bind only fields owned by that
bank. The interface inspector uses this contract: committing one property does
not erase an unfinished sibling, and Escape restores the focused property.

Update presentation according to what changed:

- `set_surface` publishes a changed declarative surface.
- `update_node` accepts typed `UiDocumentUpdate` changes and selects the
  corresponding paint/layout invalidation path.
- `DirectUiSurfaceHost::patch_text_value` is available for suitable fixed-size
  readouts. It is not a domain-state write or a general guarantee that changed
  text can always avoid layout.

The host decides what requires reconstruction. Pointer movement alone must not
rebuild every application panel. A completed animation stops requesting frames.

## 6. Insert images and icons through their resource contracts

An image node references a key. It does not decode a file or own a GPU texture:

```rust
use raf_ui::*;

pub fn artwork_node(source: UiImageSource) -> UiNode {
    UiNode::image("artwork.preview", UiImage {
        source,
        fit: UiImageFit::Contain,
        tint: None,
        options: UiImageOptions::default(),
    })
    .with_layout(UiLayout::fill(UiFlow::None))
}
```

Its parent must supply the image slot's extent. Project keys use
`assets://ui/logo.png` or `project://assets/ui/logo.png`.

The shorter image recipe reserves responsive geometry independently of loading:

```rust
let preview = UiImageView::new("assets://ui/cover.png")
    .fit(UiImageFit::Cover)
    .alt("project.cover_description")
    .shape(UiImageViewShape::Rounded(kit.metrics.corner_radius))
    .build("cover", UiLayout::default().fill_width().with_aspect_ratio(16.0 / 9.0));
```

The caller registers `project.cover_description` in its locale catalogs.
Aspect ratio is outer width divided by height; explicit fixed/fill height wins.
Minimum and maximum height can intentionally constrain the ratio. Pending,
missing and ready states share the same reserved box. `UiImageView::apply_to`
updates an existing thumbnail while retaining its identity and handlers.
For a monochrome resource, optional `tint(color)` supplies its presentation ink;
ordinary images retain their source colors unless the caller declares a tint.

At the host boundary, register the document's image references with
`images_mut().register_project_images(...)`, or the wakeup variant, when their
resource set/project changes. Pump a bounded number of completions with
`pump_project_images`. Re-registering the whole set every frame cancels work
and can restart decoding. Retain pending/error/absent states and preserve an
image's intended aspect ratio while it loads.

`UiImageOptions` provides source crop, focal point, per-use sampling and
nine-slice metadata. Use `UiIconId`/`UiIcon` for registered semantic icons.
The SVG master in that icon registry is a development asset; it does not mean
that every arbitrary SVG file is automatically a runtime image source.

External render targets go through ApiGraphicBasic's owned view and the
`register_external_image` contract. CPU recovery needs an actual raster source;
do not silently substitute an unrelated picture.

## 7. Menus, focus and visual states

A focusable trigger emits `UiAction::OpenMenu`. The menu is an elevated
`UiNodeKind::Menu`, placed in window coordinates with `raf_ui::overlays`.
The host owns open state, target validity and closing on acceptance, Escape or
outside click. It must also release capture/focus correctly. Do not enlarge
the owner's clip rectangle to make an overlay escape its panel.

Define normal, hover, focus, active, disabled and error treatments where they
apply. Focus is distinct from selection. Use the shared semantic typography,
icon catalog, density metrics and `UiTween` motion contract. Text shaping,
font fallback and physical-pixel rasterization belong to the text/presentation
owner, not per-panel drawing code.

## 8. Inspect a real render before calling the panel polished

The following helper uses the actual CPU compositor with the surface above:

```rust
use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;
use raf_ui::StudioUiPalette;

pub fn preview_save_panel(
    project_name: &str,
    palette: StudioUiPalette,
    size: [u32; 2],
) -> Vec<u8> {
    let surface = build_save_panel(palette, SavePanelView {
        project_name,
        can_save: true,
    });
    let mut host = CpuUiSurfaceHost::new(surface, palette.tokens().background);
    let frame = host.render(size, |key| {
        raf_core::i18n::t(key, raf_core::Language::Spanish)
    });
    frame.pixels.to_vec()
}
```

This checks real layout/paint, not Save execution. Input and dispatch still
need their host/application boundary. For an existing reproducible example,
see [`game_ui_designer.rs`](../crates/raf_render/tests/game_ui_designer.rs), which
imports the real interface surfaces and controller and supports optional PNG
output. No application package needs to be built to use that focused fixture.

`component_gallery(palette)` composes the actual shared recipes, including
disabled controls, localized fields, adaptive rows and ready/missing image uses.
`raf_render/tests/rafui_authoring.rs` exercises it and exports PNG files when
`RAF_INTERFACE_QA_DIR` is set. The gallery is a component fixture, not a new
installed editor panel; emitted sample commands are not domain execution.

`inspect_surface(surface, frame, images, max_nodes)` inspects an already compiled
frame on demand. It reports parent/classes, declared constraints, resolved boxes,
clips, styles, candidate state rules and image residency. Issues identify
ambiguous flow, conflicting constraints, invalid ratios, undersized boxes and
ordinary flow overlaps. The report says when traversal was truncated; absence
of a resource store leaves residency unknown. It never reads edit drafts or
adds a polling timer. There is no new native inspector window in this pass.

Inspect the resolved boxes, hit regions, clipping and diagnostics. Review narrow
and wide widths, translations, themes and fractional density. A geometry test
can pass while hierarchy, contrast, legibility or affordances still look poor.
Native pointer/keyboard behavior, window DPI and GPU appearance remain separate
acceptance checks. The snippets in this guide were checked against current
source signatures; they were not compiled or launched during the documentation
review.

## 9. Game UI uses a different authoring and scripting boundary

Game assets are version-4 `UiDocument` resources. The editor can present their
screen UI over the scene and can open an isolated content canvas. Player
instances use `Screen`, `CameraOverlay`, `CameraPlane`, `Surface` or `Billboard`.
Local/Client scripting addresses those instances, not editor node IDs.

Follow [Game interfaces](GAME_UI.md) to create and name elements, assign an
event, insert the resource into the session, and update it from Rhai or Rust.
Editing an asset does not automatically update independent inserted copies.
Physical-plane placement in the editor viewport is not implemented by this
guide; those existing presentations are inspected in Play.
