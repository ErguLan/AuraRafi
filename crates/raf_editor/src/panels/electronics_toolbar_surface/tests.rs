use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiIconId, UiStylePatch, UiStyleRuleState, UiSurface,
    UiSurfaceFrame, UiTextOverflow,
};
use raf_ui::{UiFlow, UiNode, UiNodeKind, UiOverflow, UiRect, UiSizeMode, UiStyleSelector};

use super::actions::tooltip_key;
use super::*;
use crate::panels::electronics_surface::{
    assert_electronics_layout_gate, ELECTRONICS_BORDER_WIDTH,
};

#[test]
fn desktop_ribbon_keeps_tools_visible_and_usable_at_real_window_widths() {
    for language in [raf_core::Language::English, raf_core::Language::Spanish] {
        for width in [1100.0, 1280.0, 1600.0] {
            for kind in [CadSurfaceKind::Pcb, CadSurfaceKind::Schematic] {
                let surface = build_electronics_toolbar_surface(
                    StudioUiPalette::IndustrialDark,
                    kind,
                    ElectronicsTool::Select,
                    true,
                    true,
                    true,
                    true,
                    true,
                    true,
                    true,
                    width,
                );
                let mut host = raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost::new(
                    surface.clone(),
                    [0, 0, 0, 255],
                );
                let capture = host.render(
                    [
                        width as u32,
                        crate::editor_layout::ELECTRONICS_TOOLBAR_HEIGHT as u32,
                    ],
                    |key| raf_core::i18n::t(key, language),
                );
                for id in [
                    "electronics.mode.schematic.label",
                    "electronics.mode.pcb.label",
                ] {
                    let request = capture
                        .frame
                        .text_requests
                        .iter()
                        .find(|r| r.node_id == id)
                        .unwrap();
                    let text = raf_core::i18n::t(&request.text_key, language);
                    let mut natural = request.clone();
                    natural.max_width = 1024.0;
                    natural.overflow = UiTextOverflow::Clip;
                    let needed = raf_render::api_graphic_basic::ui_surface::UiTextAtlas::default()
                        .measure_prefix_width(&natural, &text, text.chars().count());
                    assert!(
                        needed <= request.max_width + 0.1,
                        "{id} needs {needed}, receives {} at {width}",
                        request.max_width
                    );
                }
                for command in [
                    "electronics.mode.schematic",
                    "electronics.mode.pcb",
                    "electronics.select",
                    "electronics.place",
                    "electronics.fit",
                    "electronics.zoom-in",
                    "edit.undo",
                    "edit.redo",
                ] {
                    let id = format!("electronics.toolbar.{command}");
                    let node = surface.root.find(&id).expect("live tool");
                    assert!(node.focusable && !node.disabled);
                    let rect = capture
                        .frame
                        .layout_boxes
                        .iter()
                        .find(|b| b.id == id)
                        .unwrap()
                        .rect;
                    assert!(
                        rect.x >= 0.0 && rect.x + rect.width <= width + 0.5,
                        "{id}: {rect:?} at {width}"
                    );
                    assert!(rect.width >= 24.0 && rect.height >= 24.0);
                    assert!(node.event_handlers.iter().any(
                        |e| matches!(&e.action,raf_ui::UiAction::Command{name} if name==command)
                    ));
                }
                let connect = if kind == CadSurfaceKind::Pcb {
                    "electronics.route"
                } else {
                    "electronics.wire"
                };
                assert!(surface
                    .root
                    .find(&format!("electronics.toolbar.{connect}.glyph"))
                    .is_some());
                assert!(surface
                    .root
                    .find(&format!("electronics.toolbar.{connect}.label"))
                    .is_some());
            }
        }
    }
}

/// Canvas width that resolves the labelled density.
const LABELLED_WIDTH: f32 = 1600.0;
/// Canvas width that resolves the single glyph-only row.
const ICON_WIDTH: f32 = 700.0;
/// Narrowest canvas the Electronics layout allows, and therefore the worst
/// case of the stacked density.
const NARROW_WIDTH: f32 = 360.0;

/// Advance of each labeled control in the Spanish catalog, in logical points,
/// already including the 8+8 points of padding the compositor forces on every
/// labelled control. Measured from the bundled Ubuntu Medium face at 11px
/// against `crates/raf_core/locales/es.json`, which is the longest catalog of
/// the two. The reused `Contorno` key reserves a conservative 64pt span.
const MEASURED_ES_LABELS: [(&str, f32); 11] = [
    ("electronics.toolbar.schematic", 81.5),
    ("electronics.toolbar.pcb", 37.1),
    ("electronics.toolbar.select", 74.5),
    ("electronics.toolbar.wire", 45.0),
    ("electronics.toolbar.place", 54.9),
    ("electronics.toolbar.route", 48.3),
    ("electronics.pcb.via_label", 34.0),
    ("electronics.toolbar.grid", 70.1),
    ("electronics.toolbar.snap", 48.9),
    ("electronics.toolbar.labels", 64.8),
    // Existing bilingual app label reused because the shared locale files
    // are outside this task's edit scope.
    ("app.pcb_outline_status", 64.0),
];

/// Mode labels stay visible in the compact row. These widths use the same
/// Ubuntu Medium face at 9px with 1px horizontal padding on each button.
const COMPACT_ES_MODE_LABELS: [(&str, f32); 2] = [
    ("electronics.toolbar.schematic", 55.6),
    ("electronics.toolbar.pcb", 19.3),
];

fn build(kind: CadSurfaceKind, width: f32) -> UiSurface {
    build_with(
        ToolbarState::available(kind, ElectronicsTool::Select),
        width,
    )
}

fn build_unavailable(kind: CadSurfaceKind, width: f32) -> UiSurface {
    build_with(
        ToolbarState::nothing_available(kind, ElectronicsTool::Select),
        width,
    )
}

fn build_with(state: ToolbarState, width: f32) -> UiSurface {
    build_compact_electronics_toolbar_surface(
        StudioUiPalette::IndustrialDark,
        state.surface,
        state.tool,
        state.grid_visible,
        state.labels_visible,
        state.snap_enabled,
        state.can_undo,
        state.can_redo,
        state.has_selection,
        state.can_rotate,
        width,
    )
}

fn frame(surface: &UiSurface, width: f32) -> UiSurfaceFrame {
    surface.build_frame(
        width.max(1.0) as u32,
        electronics_toolbar_height(width).max(1.0) as u32,
        [0, 0, 0, 255],
    )
}

fn rect_of(frame: &UiSurfaceFrame, id: &str) -> UiRect {
    frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == id)
        .unwrap_or_else(|| panic!("{id} is missing from the frame"))
        .rect
}

fn rule_for<'a>(surface: &'a UiSurface, class: &str, state: UiStyleRuleState) -> &'a UiStylePatch {
    surface
        .style_sheet
        .rules
        .iter()
        .find(|rule| {
            rule.selector == UiStyleSelector::Class(class.to_string()) && rule.state == state
        })
        .map(|rule| &rule.patch)
        .unwrap_or_else(|| panic!("{class} has no {state:?} rule"))
}

fn class_of(surface: &UiSurface, id: &str, class: &str) -> bool {
    surface
        .root
        .find(id)
        .unwrap_or_else(|| panic!("{id} is missing"))
        .classes
        .iter()
        .any(|owned| owned == class)
}

fn collect_glyphs(node: &UiNode, seen: &mut Vec<(String, UiIconId)>) {
    if let Some(icon) = node.icon {
        let command = node
            .id
            .strip_prefix("electronics.toolbar.")
            .unwrap_or(node.id.as_str())
            .to_string();
        seen.push((command, icon.id));
    }
    for child in &node.children {
        collect_glyphs(child, seen);
    }
}

/// Width of one control at the labelled density, from the measured table.
fn labelled_width(label: &str) -> f32 {
    MEASURED_ES_LABELS
        .iter()
        .find(|(key, _)| *key == label)
        .map(|(_, width)| *width)
        .unwrap_or(TOOLBAR_ICON_BUTTON_WIDTH)
}

fn compact_mode_width(label: &str) -> f32 {
    COMPACT_ES_MODE_LABELS
        .iter()
        .find(|(key, _)| *key == label)
        .map(|(_, width)| *width)
        .unwrap_or(TOOLBAR_ICON_BUTTON_WIDTH)
}

/// Width of one control at one density, from the measured table.
fn control_width(labelled: bool, label: Option<&str>) -> f32 {
    match (labelled, label) {
        (true, Some(label)) => labelled_width(label),
        _ => TOOLBAR_ICON_BUTTON_WIDTH,
    }
}

/// Span of the document group at one density: the surface segment, the tools
/// of the active surface and the two selection edits.
fn document_span(kind: CadSurfaceKind, labelled: bool) -> f32 {
    let mode_width = if labelled {
        labelled_width("electronics.toolbar.schematic") + labelled_width("electronics.toolbar.pcb")
    } else {
        compact_mode_width("electronics.toolbar.schematic")
            + compact_mode_width("electronics.toolbar.pcb")
    };
    let segment = mode_width + SEGMENT_GAP + 2.0 * SEGMENT_INSET;
    let mut widths = vec![
        segment,
        control_width(labelled, Some("electronics.toolbar.select")),
        // Pan stays a glyph: see `document_controls`.
        control_width(labelled, None),
    ];
    match kind {
        CadSurfaceKind::Schematic => {
            widths.push(control_width(labelled, Some("electronics.toolbar.wire")));
            widths.push(control_width(labelled, Some("electronics.toolbar.place")));
        }
        CadSurfaceKind::Pcb => {
            widths.push(control_width(labelled, Some("electronics.toolbar.route")));
            widths.push(control_width(labelled, Some("electronics.pcb.via_label")));
            widths.push(control_width(labelled, Some("app.pcb_outline_status")));
            widths.push(control_width(labelled, Some("electronics.toolbar.place")));
            widths.push(control_width(labelled, None));
            // Board rules is a glyph alongside the sync operation.
            widths.push(control_width(labelled, None));
        }
    }
    // Rotate and Delete are glyph-only in every density.
    widths.push(control_width(labelled, None));
    widths.push(control_width(labelled, None));
    let control_gaps = TOOLBAR_GAP * (widths.len() - 1) as f32;
    let island_separators = if labelled { 4.0 } else { 1.0 };
    widths.iter().sum::<f32>() + control_gaps + island_separators * (1.0 + TOOLBAR_GAP)
}

/// Span of the view group at one density: the three latched options plus the
/// fit and zoom commands.
fn view_span(labelled: bool) -> f32 {
    let widths = [
        control_width(labelled, Some("electronics.toolbar.grid")),
        control_width(labelled, Some("electronics.toolbar.snap")),
        control_width(labelled, Some("electronics.toolbar.labels")),
        control_width(labelled, None),
        control_width(labelled, None),
        control_width(labelled, None),
    ];
    widths.iter().sum::<f32>() + TOOLBAR_GAP * (widths.len() - 1) as f32
}

/// Span of the history group at one density: undo and redo, both glyph-only.
fn history_span() -> f32 {
    2.0 * TOOLBAR_ICON_BUTTON_WIDTH + TOOLBAR_GAP
}

/// Authored width of one hairline, read from the document instead of being
/// re-declared here.
fn separator_width(surface: &UiSurface, id: &str) -> f32 {
    surface
        .root
        .find(id)
        .unwrap_or_else(|| panic!("{id} is missing"))
        .layout
        .basis[0]
}

/// Band a single-row density needs: both groups, both hairlines, the four
/// gaps between the three row children and the band padding.
fn single_row_span(surface: &UiSurface, kind: CadSurfaceKind, labelled: bool) -> f32 {
    document_span(kind, labelled)
        + view_span(labelled)
        + history_span()
        + separator_width(surface, "electronics.toolbar.document-separator")
        + separator_width(surface, "electronics.toolbar.view-separator")
        + 4.0 * TOOLBAR_GAP
        + 2.0 * TOOLBAR_PADDING_X
}

/// Which row of the stacked density a span describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackedRow {
    /// The document group, alone on the first row.
    Document,
    /// The view and history groups, sharing the second row.
    View,
}

/// Band a stacked row needs: the row content, its own hairline and gaps and
/// the band padding.
fn stacked_row_span(
    surface: &UiSurface,
    kind: CadSurfaceKind,
    row: StackedRow,
    labelled: bool,
) -> f32 {
    let (content, hairline, gaps) = match row {
        // The document row is a single group.
        StackedRow::Document => (document_span(kind, labelled), 0.0, 0.0),
        // The view row holds the view group, the hairline and the history
        // group, so it pays for the hairline and two extra gaps.
        StackedRow::View => (
            view_span(labelled) + history_span(),
            separator_width(surface, "electronics.toolbar.view-separator"),
            2.0 * TOOLBAR_GAP,
        ),
    };
    content + hairline + gaps + 2.0 * TOOLBAR_PADDING_X
}

#[test]
fn every_control_declares_a_role_a_label_and_a_tooltip() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
            let surface = build(kind, width);
            assert_declared(&surface, kind, width);
        }
    }
}

fn assert_declared(surface: &UiSurface, kind: CadSurfaceKind, width: f32) {
    fn walk(node: &UiNode, kind: CadSurfaceKind, width: f32) {
        if node.kind == UiNodeKind::Button {
            assert!(
                node.tooltip_key.is_some(),
                "{} needs a tooltip at every density",
                node.id
            );
            assert!(
                node.accessibility_label_key.is_some(),
                "{} needs an accessible name",
                node.id
            );
            assert_ne!(
                node.accessibility_role,
                UiAccessibilityRole::Generic,
                "{} needs a role",
                node.id
            );
            assert!(
                node.text_key.is_some() || node.icon.is_some(),
                "{} must say what it does with a label or a glyph, {kind:?} at {width}",
                node.id
            );
            assert!(
                !(node.text_key.is_some() && node.icon.is_some()),
                "{} must not combine a glyph and a label",
                node.id
            );
        }
        for child in &node.children {
            walk(child, kind, width);
        }
    }
    walk(&surface.root, kind, width);
}

#[test]
fn the_labelled_density_prints_the_controls_whose_glyph_does_not_speak() {
    let surface = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
    for command in [
        "electronics.mode.schematic",
        "electronics.mode.pcb",
        "electronics.select",
        "electronics.route",
        "electronics.board-outline",
        "electronics.place",
        "electronics.grid.toggle",
        "electronics.snap.toggle",
        "electronics.labels.toggle",
    ] {
        let node = surface
            .root
            .find(&format!("electronics.toolbar.{command}"))
            .unwrap_or_else(|| panic!("{command} is missing"));
        assert!(node.text_key.is_some(), "{command} must print its label");
        assert!(node.icon.is_none(), "{command} must not carry a glyph");
        assert_eq!(
            node.layout.width_mode,
            UiSizeMode::FitContent,
            "{command} must measure its localized label"
        );
    }
    // A glyph-only control is deliberate, not an omission: each of these has a
    // universal glyph or a gesture equivalent that its tooltip states.
    for command in [
        "electronics.pan",
        "electronics.pcb.sync",
        "electronics.rotate",
        "edit.delete",
        "edit.undo",
        "edit.redo",
    ] {
        let node = surface
            .root
            .find(&format!("electronics.toolbar.{command}"))
            .unwrap_or_else(|| panic!("{command} is missing"));
        assert!(node.text_key.is_none(), "{command} stays a glyph");
        assert!(node.icon.is_some(), "{command} must keep its glyph");
        assert_eq!(node.layout.basis[0], TOOLBAR_ICON_BUTTON_WIDTH);
    }
}

#[test]
fn the_glyph_densities_keep_a_glyph_and_a_tooltip_for_every_control() {
    for width in [ICON_WIDTH, NARROW_WIDTH] {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let surface = build(kind, width);
            fn walk(node: &UiNode) {
                if node.kind == UiNodeKind::Button {
                    let is_mode = node.id == "electronics.toolbar.electronics.mode.schematic"
                        || node.id == "electronics.toolbar.electronics.mode.pcb";
                    if is_mode {
                        assert!(node.text_key.is_some(), "{} must name the mode", node.id);
                        assert!(node.icon.is_none(), "{} must stay text-first", node.id);
                    } else {
                        assert!(node.text_key.is_none(), "{} hid its glyph", node.id);
                        assert!(node.icon.is_some(), "{} lost its glyph", node.id);
                    }
                    assert!(node.tooltip_key.is_some());
                    assert_eq!(
                        node.layout.width_mode,
                        if is_mode {
                            UiSizeMode::FitContent
                        } else {
                            UiSizeMode::Fixed
                        }
                    );
                }
                for child in &node.children {
                    walk(child);
                }
            }
            walk(&surface.root);
        }
    }
}

#[test]
fn the_mode_selector_keeps_both_localized_names_at_every_density() {
    for width in [
        NARROW_WIDTH,
        crate::editor_layout::ELECTRONICS_TOOLBAR_STACKED_WIDTH,
        crate::editor_layout::ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
        1200.0,
    ] {
        for (surface_kind, active_mode) in [
            (CadSurfaceKind::Schematic, "schematic"),
            (CadSurfaceKind::Pcb, "pcb"),
        ] {
            let surface = build(surface_kind, width);
            for (command, key) in [
                ("schematic", "electronics.toolbar.schematic"),
                ("pcb", "electronics.toolbar.pcb"),
            ] {
                let mode = surface
                    .root
                    .find(&format!("electronics.toolbar.electronics.mode.{command}"))
                    .expect("both surface modes stay available");
                assert_eq!(mode.text_key.as_deref(), Some(key));
                assert!(mode.icon.is_none());
                let selected = command == active_mode;
                assert_eq!(mode.accessibility_selected, Some(selected));
                assert_eq!(
                    class_of(
                        &surface,
                        &format!("electronics.toolbar.electronics.mode.{command}"),
                        CLASS_OPTION_ON,
                    ),
                    selected,
                    "the active {command} mode has a visible selected treatment"
                );
                assert!(mode.focusable);
                assert!(mode.tooltip_key.is_some());
            }
            assert!(
                surface
                    .root
                    .find("electronics.toolbar.surface-separator")
                    .is_some(),
                "the mode selector stays distinct from the tool islands"
            );
        }
    }
}

#[test]
fn tool_islands_follow_the_active_surface_workflow() {
    let schematic = build(CadSurfaceKind::Schematic, LABELLED_WIDTH);
    for command in ["electronics.select", "electronics.pan"] {
        assert!(class_of(
            &schematic,
            &format!("electronics.toolbar.{command}"),
            CLASS_TOOL_ISLAND_SELECT
        ));
    }
    assert!(class_of(
        &schematic,
        "electronics.toolbar.electronics.wire",
        CLASS_TOOL_ISLAND_CONNECT
    ));
    assert!(class_of(
        &schematic,
        "electronics.toolbar.electronics.place",
        CLASS_TOOL_ISLAND_PLACE
    ));

    let pcb = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
    for command in ["electronics.route", "electronics.board-outline"] {
        assert!(class_of(
            &pcb,
            &format!("electronics.toolbar.{command}"),
            CLASS_TOOL_ISLAND_CONNECT
        ));
    }
    for id in [
        "electronics.toolbar.island-separator.connection",
        "electronics.toolbar.island-separator.placement",
        "electronics.toolbar.island-separator.operations",
    ] {
        assert!(pcb.root.find(id).is_some(), "missing boundary {id}");
    }
}

#[test]
fn the_band_never_scrolls_and_clips_instead() {
    // A horizontal scroll owner has no reachable thumb in this compositor, so
    // a scrollable band would hide controls instead of moving them.
    for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let surface = build(kind, width);
            assert!(surface.root.control.scroll_axis().is_none());
            assert_eq!(surface.root.layout.overflow, UiOverflow::Clip);
            assert!(!surface.root.interactive, "the band floor is not a target");
        }
    }
}

#[test]
fn the_stacked_density_puts_the_view_row_below_the_document_row() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        let surface = build(kind, NARROW_WIDTH);
        assert_eq!(surface.root.layout.flow, UiFlow::Column);
        assert_eq!(
            surface.root.layout.basis[1],
            crate::editor_layout::ELECTRONICS_TOOLBAR_STACKED_HEIGHT
        );
        assert_eq!(surface.root.children.len(), 2);
        let resolved = frame(&surface, NARROW_WIDTH);
        let document = rect_of(&resolved, "electronics.toolbar.cluster.document");
        let view = rect_of(&resolved, "electronics.toolbar.right");
        assert!(
            document.bottom() <= view.y + f32::EPSILON,
            "{kind:?}: the document row ends at {} and the view row starts at {}",
            document.bottom(),
            view.y
        );
        assert_eq!(document.height, TOOLBAR_BUTTON_HEIGHT);
        assert_eq!(view.height, TOOLBAR_BUTTON_HEIGHT);
    }
}

#[test]
fn the_single_row_densities_keep_both_groups_on_one_track() {
    for width in [ICON_WIDTH, LABELLED_WIDTH] {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let surface = build(kind, width);
            assert_eq!(surface.root.layout.flow, UiFlow::Row);
            assert_eq!(surface.root.children.len(), 3);
            assert_eq!(
                surface.root.layout.basis[1],
                crate::editor_layout::ELECTRONICS_TOOLBAR_HEIGHT
            );
            let resolved = frame(&surface, width);
            let document = rect_of(&resolved, "electronics.toolbar.cluster.document");
            let view = rect_of(&resolved, "electronics.toolbar.cluster.view");
            let history = rect_of(&resolved, "electronics.toolbar.cluster.history");
            assert!(
                document.right() <= view.x + f32::EPSILON,
                "{kind:?} at {width}: the groups overlap"
            );
            assert!(view.right() <= history.x + f32::EPSILON);
            assert!(
                (document.y - view.y).abs() < 0.01,
                "{kind:?} at {width}: the groups left the shared track"
            );
            assert!((document.y - history.y).abs() < 0.01);
        }
    }
}

#[test]
fn the_labelled_row_fits_its_breakpoint_with_the_measured_span() {
    let mut worst = 0.0_f32;
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        let span = single_row_span(&build(kind, LABELLED_WIDTH), kind, true);
        assert!(
            span <= ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
            "{kind:?} labelled row needs {span} points, the breakpoint is {ELECTRONICS_TOOLBAR_LABELLED_WIDTH}"
        );
        worst = worst.max(span);
    }
    // A breakpoint far above the worst measured span would push labelled
    // controls into the glyph-only row for no reason.
    assert!(
        ELECTRONICS_TOOLBAR_LABELLED_WIDTH - worst <= 60.0,
        "the worst labelled row needs {worst} points, the breakpoint leaves {} unused",
        ELECTRONICS_TOOLBAR_LABELLED_WIDTH - worst
    );
}

#[test]
fn the_glyph_row_fits_its_breakpoint_with_the_measured_span() {
    let mut worst = 0.0_f32;
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        let span = single_row_span(&build(kind, ICON_WIDTH), kind, false);
        assert!(
            span <= ELECTRONICS_TOOLBAR_STACKED_WIDTH,
            "{kind:?} glyph row needs {span} points, the breakpoint is {ELECTRONICS_TOOLBAR_STACKED_WIDTH}"
        );
        worst = worst.max(span);
    }
    assert!(
        ELECTRONICS_TOOLBAR_STACKED_WIDTH - worst <= 40.0,
        "the worst glyph row needs {worst} points, the breakpoint leaves {} unused",
        ELECTRONICS_TOOLBAR_STACKED_WIDTH - worst
    );
}

#[test]
fn every_stacked_row_fits_the_narrowest_canvas_the_editor_allows() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        let surface = build(kind, NARROW_WIDTH);
        for row in [StackedRow::Document, StackedRow::View] {
            let span = stacked_row_span(&surface, kind, row, false);
            assert!(
                span <= NARROW_WIDTH,
                "{kind:?} {row:?} row needs {span} of a {NARROW_WIDTH} point canvas"
            );
        }
    }
}

#[test]
fn the_reserved_band_height_matches_the_density() {
    for (width, density) in [
        (NARROW_WIDTH, ToolbarDensity::Stacked),
        (ICON_WIDTH, ToolbarDensity::Icon),
        (LABELLED_WIDTH, ToolbarDensity::Labelled),
    ] {
        let surface = build(CadSurfaceKind::Schematic, width);
        assert_eq!(ToolbarDensity::for_width(width), density, "at {width}");
        let band = electronics_toolbar_height(width);
        assert_eq!(surface.root.layout.basis[1], band, "at {width}");
        let rows = density.rows() as f32;
        let content = rows * TOOLBAR_BUTTON_HEIGHT + (rows - 1.0) * TOOLBAR_ROW_GAP;
        assert_eq!(
            density.padding_y(width) * 2.0 + content,
            band,
            "the rows and their padding must consume the whole band at {width}"
        );
        assert_eq!(TOOLBAR_BUTTON_HEIGHT, ELECTRONICS_CONTROL_HEIGHT);
        assert!(density.padding_y(width) >= 4.0, "at {width}");
    }
}

#[test]
fn the_density_breakpoints_are_ordered() {
    assert!(ELECTRONICS_TOOLBAR_STACKED_WIDTH < ELECTRONICS_TOOLBAR_LABELLED_WIDTH);
    assert_eq!(
        ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_STACKED_WIDTH - 1.0),
        ToolbarDensity::Stacked
    );
    assert_eq!(
        ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_STACKED_WIDTH),
        ToolbarDensity::Icon
    );
    assert_eq!(
        ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_LABELLED_WIDTH - 1.0),
        ToolbarDensity::Icon
    );
    assert_eq!(
        ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_LABELLED_WIDTH),
        ToolbarDensity::Labelled
    );
}

#[test]
fn the_engaged_tool_and_an_engaged_option_do_not_share_a_treatment() {
    for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
        let surface = build_compact_electronics_toolbar_surface(
            palette,
            CadSurfaceKind::Schematic,
            ElectronicsTool::Wire,
            true,
            true,
            true,
            true,
            true,
            true,
            true,
            LABELLED_WIDTH,
        );
        let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
        let tool = rule_for(&surface, CLASS_TOOL_ACTIVE, UiStyleRuleState::Always);
        let option = rule_for(&surface, CLASS_OPTION_ON, UiStyleRuleState::Always);
        assert_eq!(tool.fill, Some(tokens.accent));
        // The page color is the only token that contrasts with the accent in
        // both themes, so the tool glyph stays readable on its own fill.
        assert_eq!(tool.text, Some(tokens.background));
        assert_eq!(option.fill, Some(tokens.selection));
        assert_ne!(option.fill, tool.fill);
        assert_ne!(option.text, tool.text);
        // The engaged option keeps a neutral resting edge so the shared focus
        // ring stays visible on top of it.
        assert_eq!(option.border, Some(tokens.border));
        assert_ne!(option.border, tool.border);

        assert!(class_of(
            &surface,
            "electronics.toolbar.electronics.wire",
            CLASS_TOOL_ACTIVE
        ));
        assert!(!class_of(
            &surface,
            "electronics.toolbar.electronics.grid.toggle",
            CLASS_TOOL_ACTIVE
        ));
        assert!(class_of(
            &surface,
            "electronics.toolbar.electronics.grid.toggle",
            CLASS_OPTION_ON
        ));
        assert!(class_of(
            &surface,
            "electronics.toolbar.electronics.mode.schematic",
            CLASS_OPTION_ON
        ));
    }
}

#[test]
fn only_the_tool_in_hand_wears_the_saturated_accent() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
            let surface = build_with(ToolbarState::available(kind, ElectronicsTool::Place), width);
            let mut engaged = Vec::new();
            fn walk(node: &UiNode, engaged: &mut Vec<String>) {
                if node.classes.iter().any(|class| class == CLASS_TOOL_ACTIVE) {
                    engaged.push(node.id.clone());
                }
                for child in &node.children {
                    walk(child, engaged);
                }
            }
            walk(&surface.root, &mut engaged);
            assert_eq!(
                engaged,
                vec!["electronics.toolbar.electronics.place".to_string()],
                "{kind:?} at {width} must engage exactly one tool"
            );
        }
    }
}

#[test]
fn the_document_segment_is_the_only_bordered_container() {
    let surface = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
    let tokens = crate::panels::electronics_surface::electronics_tokens(surface.palette);
    let segment = rule_for(
        &surface,
        "electronics-toolbar-segment",
        UiStyleRuleState::Always,
    );
    assert_eq!(segment.border, Some(tokens.border));
    assert_eq!(segment.border_width, Some(ELECTRONICS_BORDER_WIDTH));
    let button = rule_for(&surface, CLASS_BUTTON, UiStyleRuleState::Always);
    assert_eq!(button.border, Some([0, 0, 0, 0]));
    assert_eq!(button.border_width, Some(0.0));
    // The segment holds both options side by side without overlapping.
    // The gap is intentional padding, so the invariant is ordering plus a
    // non-negative gap, not a hard "touching" edge.
    let resolved = frame(&surface, LABELLED_WIDTH);
    let schematic = rect_of(&resolved, "electronics.toolbar.electronics.mode.schematic");
    let pcb = rect_of(&resolved, "electronics.toolbar.electronics.mode.pcb");
    let shell = rect_of(&resolved, "electronics.toolbar.segment");
    assert!(
      schematic.right() <= pcb.x + f32::EPSILON,
      "segment options overlap at {LABELLED_WIDTH}px: schematic={schematic:?} pcb={pcb:?} shell={shell:?}"
  );
    assert!(shell.x <= schematic.x + f32::EPSILON);
    assert!(pcb.right() <= shell.right() + f32::EPSILON);
}

#[test]
fn an_unavailable_command_states_its_prerequisite_instead_of_disappearing() {
    for width in [NARROW_WIDTH, LABELLED_WIDTH] {
        let surface = build_unavailable(CadSurfaceKind::Schematic, width);
        for (command, reason) in [
            (
                "electronics.rotate",
                "electronics.tooltip.rotate.unavailable",
            ),
            ("edit.delete", "electronics.tooltip.delete.unavailable"),
            ("edit.undo", "electronics.tooltip.undo.unavailable"),
            ("edit.redo", "electronics.tooltip.redo.unavailable"),
        ] {
            let id = format!("electronics.toolbar.{command}");
            let node = surface
                .root
                .find(&id)
                .unwrap_or_else(|| panic!("{id} is missing"));
            assert!(
                !node.disabled,
                "{id} must stay reachable so its reason can be read"
            );
            assert!(node.focusable, "{id} must stay in the focus order");
            assert_eq!(
                node.tooltip_key.as_deref(),
                Some(reason),
                "{id} must explain why it cannot run"
            );
            assert_eq!(node.accessibility_description_key.as_deref(), Some(reason));
            assert_eq!(
                node.accessibility_label_key.as_deref(),
                Some(tooltip_key(command)),
                "{id} must keep naming its action"
            );
            assert!(node.classes.iter().any(|class| class == CLASS_UNAVAILABLE));
        }
        // An available destructive action keeps the danger edge; an
        // unavailable one must not, or it would read as armed.
        assert!(!class_of(
            &surface,
            "electronics.toolbar.edit.delete",
            CLASS_DANGER
        ));
        let available = build(CadSurfaceKind::Schematic, width);
        assert!(class_of(
            &available,
            "electronics.toolbar.edit.delete",
            CLASS_DANGER
        ));
    }
}

#[test]
fn every_band_class_exposes_hover_focus_and_pressed_states() {
    let surface = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
    for class in [
        CLASS_BUTTON,
        CLASS_TOOL_ACTIVE,
        CLASS_OPTION_ON,
        CLASS_UNAVAILABLE,
        CLASS_DANGER,
    ] {
        let states: Vec<UiStyleRuleState> = surface
            .style_sheet
            .rules
            .iter()
            .filter(|rule| rule.selector == UiStyleSelector::Class(class.to_string()))
            .map(|rule| rule.state)
            .collect();
        for state in [
            UiStyleRuleState::Hovered,
            UiStyleRuleState::Focused,
            UiStyleRuleState::Active,
        ] {
            assert!(
                states.contains(&state),
                "{class} needs a {state:?} rule, found {states:?}"
            );
        }
    }
}

#[test]
fn no_two_band_commands_share_one_glyph() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
            let surface = build(kind, width);
            let mut seen = Vec::new();
            collect_glyphs(&surface.root, &mut seen);
            for (index, (command, icon)) in seen.iter().enumerate() {
                assert!(
                    !seen[index + 1..].iter().any(|(_, other)| other == icon),
                    "{kind:?} at {width}: {command} shares the {icon:?} glyph with a later command"
                );
            }
        }
    }
}

#[test]
fn the_band_passes_the_retained_layout_gate_in_both_width_directions() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        for width in [
            NARROW_WIDTH,
            ELECTRONICS_TOOLBAR_STACKED_WIDTH - 1.0,
            ELECTRONICS_TOOLBAR_STACKED_WIDTH,
            ELECTRONICS_TOOLBAR_LABELLED_WIDTH - 1.0,
            ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
            1200.0,
            1280.0,
            LABELLED_WIDTH,
        ] {
            let surface = build(kind, width);
            let diagnostics = assert_electronics_layout_gate(
                &surface,
                width as u32,
                electronics_toolbar_height(width) as u32,
            );
            assert!(
                diagnostics.focusable_regions >= 10,
                "{kind:?} at {width} resolved {} focusable regions",
                diagnostics.focusable_regions
            );
        }
    }
}

#[test]
fn cpu_toolbar_layouts_at_real_breakpoints_and_can_save_optional_review_images() {
    use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;

    let qa_dir = std::env::var_os("RAF_ELECTRONICS_TOOLBAR_QA_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &qa_dir {
        std::fs::create_dir_all(dir).expect("create toolbar QA image directory");
    }

    for width in [360_u32, 530, 840, 1200] {
        let height = electronics_toolbar_height(width as f32) as u32;
        let surface = build(CadSurfaceKind::Pcb, width as f32);
        let mut host = CpuUiSurfaceHost::new(surface, [12, 15, 21, 255]);
        let rendered = host.render([width, height], |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });
        assert!(
            rendered.pixels.chunks_exact(4).any(|pixel| {
                pixel[0] != 12 || pixel[1] != 15 || pixel[2] != 21 || pixel[3] != 255
            }),
            "the toolbar should paint at {width}×{height}"
        );
        for layout in rendered
            .frame
            .layout_boxes
            .iter()
            .filter(|layout| layout.id.starts_with("electronics.toolbar.electronics."))
        {
            assert!(
                layout.rect.width > 0.0
                    && layout.rect.x >= 0.0
                    && layout.rect.right() <= width as f32 + f32::EPSILON,
                "{} escaped the {width}px toolbar: {:?}",
                layout.id,
                layout.rect
            );
        }
        if let Some(dir) = &qa_dir {
            image::save_buffer(
                dir.join(format!("electronics-toolbar-{width}.png")),
                rendered.pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .expect("save optional toolbar QA image");
        }
    }
}

#[test]
fn the_unavailable_band_also_passes_the_retained_layout_gate() {
    for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
        for width in [NARROW_WIDTH, LABELLED_WIDTH] {
            let surface = build_unavailable(kind, width);
            assert_electronics_layout_gate(
                &surface,
                width as u32,
                electronics_toolbar_height(width) as u32,
            );
        }
    }
}
