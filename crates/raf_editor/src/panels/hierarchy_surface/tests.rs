use super::rows::{row_icon, row_node};
use super::*;

use crate::panels::hierarchy_model::{HierarchyRow, HierarchyView};
use raf_core::scene::Primitive;
use raf_render::api_graphic_basic::ui_surface::UiIconId;

#[test]
fn script_row_has_script_icon_and_open_action_but_no_scene_actions() {
    let mut source = row(Primitive::Cube, false);
    source.script_link = Some((
        0,
        crate::script_relationships::ScriptLink {
            path: "scripts/power.rhai".into(),
            kind: crate::script_relationships::ScriptLinkKind::Reference,
        },
    ));
    source.name = "power.rhai".into();
    let node = row_node(
        StudioUiPalette::IndustrialDark,
        &source,
        &[source.id],
        None,
        26.0,
        18.0,
        true,
        true,
        true,
        None,
    );
    let open = find_node(&node, "hierarchy.script.0.0.open").unwrap();
    assert_eq!(
        open.icon.as_ref().map(|icon| icon.id),
        Some(UiIconId::Script)
    );
    let serialized = format!("{node:?}");
    assert!(serialized.contains("hierarchy.script.open:0:0"));
    for command in [
        "hierarchy.select:",
        "hierarchy.drag.",
        "hierarchy.rename.",
        "hierarchy.menu:",
        "hierarchy.visibility",
    ] {
        assert!(!serialized.contains(command));
    }
}

fn row(primitive: Primitive, is_folder: bool) -> HierarchyRow {
    HierarchyRow {
        id: raf_core::scene::SceneNodeId(0),
        depth: 0,
        name: "node".to_string(),
        primitive,
        is_folder,
        is_camera: false,
        visible: true,
        locked: false,
        has_children: false,
        expanded: false,
        script_link: None,
        badges: Vec::new(),
        insertion_parent: false,
    }
}

fn find_node<'a>(node: &'a raf_ui::UiNode, id: &str) -> Option<&'a raf_ui::UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}

#[test]
fn hierarchy_rows_use_semantic_primitive_icons() {
    assert_eq!(row_icon(&row(Primitive::Cube, false)), UiIconId::Cube);
    assert_eq!(row_icon(&row(Primitive::Sphere, false)), UiIconId::Sphere);
    assert_eq!(row_icon(&row(Primitive::Plane, false)), UiIconId::Plane);
    assert_eq!(
        row_icon(&row(Primitive::Cylinder, false)),
        UiIconId::Cylinder
    );
    assert_eq!(row_icon(&row(Primitive::Empty, false)), UiIconId::Node);
    assert_eq!(row_icon(&row(Primitive::Cube, true)), UiIconId::Folder);
    let mut camera_part = row(Primitive::Cube, false);
    camera_part.is_camera = true;
    assert_eq!(row_icon(&camera_part), UiIconId::Camera);
}

#[test]
fn hierarchy_chrome_does_not_introduce_project_type_tabs_or_painted_drag_slots() {
    let view = HierarchyView {
        total_rows: 1,
        entity_rows: 1,
        visible_start: 0,
        top_spacer: 0.0,
        bottom_spacer: 0.0,
        rows: vec![row(Primitive::Cube, false)],
    };
    let surface = build_hierarchy_surface(
        StudioUiPalette::IndustrialDark,
        &view,
        &[],
        None,
        None,
        false,
        false,
        None,
        None,
        [460.0, 400.0],
        26.0,
        14.0,
        true,
        true,
        true,
        1.0,
        None,
        None,
        None,
        false,
        false,
        "hierarchy",
        [false; 3],
        None,
    );
    let serialized = format!("{surface:?}");
    assert!(!serialized.contains("Games"));
    assert!(!serialized.contains("Electronics"));
    assert!(!serialized.contains("hierarchy.toolbar.accent"));
    assert!(!serialized.contains("hierarchy-drop-zone"));

    for (id, key) in [
        ("hierarchy", "app.hierarchy"),
        ("assets", "app.assets"),
        ("world", "app.world"),
        ("bookmarks", "app.bookmarks"),
        ("search", "app.search"),
    ] {
        let tab = find_node(&surface.root, &format!("hierarchy.tab.{id}"))
            .expect("hierarchy tab should be retained");
        assert!(tab.interactive, "{id} must receive hover input");
        assert_eq!(tab.tooltip_key.as_deref(), Some(key));
    }
}

#[test]
fn regular_hierarchy_tabs_keep_all_labels_inside_the_panel() {
    let row = tabs(StudioUiPalette::IndustrialDark, false, "hierarchy");
    let total_width: f32 = row.children.iter().map(|tab| tab.layout.basis[0]).sum();

    assert_eq!(row.children.len(), 6);
    assert!(row.children.iter().all(|tab| tab.text_key.is_some()));
    assert!(total_width + row.layout.padding.left + row.layout.padding.right <= 530.0);
}

#[test]
fn hierarchy_row_primary_target_opens_its_context_menu() {
    let row = row_node(
        StudioUiPalette::IndustrialDark,
        &row(Primitive::Cube, false),
        &[],
        None,
        26.0,
        14.0,
        true,
        true,
        true,
        None,
    );
    let target = find_node(&row, "hierarchy.row.0.select").expect("row target");

    assert!(target.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        raf_ui::UiAction::OpenMenu { id } if id == "hierarchy.menu:0"
    )));
    assert_eq!(target.tooltip_value.as_deref(), Some("node"));
}

#[test]
fn hierarchy_context_menu_is_bounded_inside_small_viewports() {
    let rect = context_menu_rect(Some([12.0, 190.0]), true, [240.0, 200.0]);

    assert!(rect.height < 200.0);
    assert!(rect.bottom() <= 192.0);
}

#[test]
fn empty_context_menu_exposes_primitive_disclosure_and_options() {
    let surface = build_hierarchy_context_overlay_surface(
        StudioUiPalette::IndustrialDark,
        None,
        true,
        true,
        None,
        Some([80.0, 80.0]),
        [900.0, 600.0],
        false,
    );
    let trigger = find_node(
        &surface.root,
        "hierarchy.empty-context-menu.create-primitive",
    )
    .expect("primitive trigger");
    assert!(trigger.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        raf_ui::UiAction::Command { name }
            if name == "hierarchy.empty.create-primitive-menu"
    )));
    let arrow = find_node(
        &surface.root,
        "hierarchy.empty-context-menu.create-primitive.arrow",
    )
    .expect("primitive disclosure arrow");
    assert_eq!(
        arrow.icon.as_ref().map(|icon| icon.id),
        Some(UiIconId::ChevronRight)
    );

    for slug in ["cube", "sphere", "cylinder", "plane"] {
        assert!(
            find_node(&surface.root, &format!("hierarchy.primitive-option.{slug}")).is_some()
        );
    }
}
