use super::rows::{row_icon, row_node};
use super::*;

use crate::panels::hierarchy_surface::context::context_menu_rect;

use crate::panels::hierarchy_model::{HierarchyEntityKind, HierarchyRow, HierarchyView};
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
        false,
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
        kind: if is_folder {
            HierarchyEntityKind::Folder
        } else if primitive == Primitive::Empty {
            HierarchyEntityKind::Empty
        } else {
            HierarchyEntityKind::Primitive
        },
        visible: true,
        locked: false,
        has_children: false,
        expanded: false,
        script_link: None,
        has_scripts: false,
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
    camera_part.kind = HierarchyEntityKind::Camera;
    assert_eq!(row_icon(&camera_part), UiIconId::Camera);
}

#[test]
fn light_and_mesh_rows_use_the_kind_icon_registry() {
    let mut light = row(Primitive::Empty, false);
    light.kind = HierarchyEntityKind::Light;
    let mut mesh = row(Primitive::Empty, false);
    mesh.kind = HierarchyEntityKind::MeshAsset;
    let mut audio = row(Primitive::Empty, false);
    audio.kind = HierarchyEntityKind::Audio;
    let mut prefab = row(Primitive::Empty, false);
    prefab.kind = HierarchyEntityKind::Prefab;

    assert_eq!(row_icon(&light), UiIconId::Sun);
    assert_eq!(row_icon(&mesh), UiIconId::Assets);
    assert_eq!(row_icon(&audio), UiIconId::Play);
    assert_eq!(row_icon(&prefab), UiIconId::Entity);
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
        box_selection: None,
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
        false,
    );
    let serialized = format!("{surface:?}");
    assert!(!serialized.contains("Games"));
    assert!(!serialized.contains("Electronics"));
    assert!(!serialized.contains("hierarchy.toolbar.accent"));
    assert!(!serialized.contains("hierarchy-drop-zone"));

    for (id, key) in [("hierarchy", "app.scene"), ("lighting", "lighting.title")] {
        let tab = find_node(&surface.root, &format!("hierarchy.tab.{id}"))
            .expect("hierarchy tab should be retained");
        assert!(tab.interactive, "{id} must receive hover input");
        assert_eq!(tab.tooltip_key.as_deref(), Some(key));
    }
    for removed in ["assets", "world", "bookmarks", "search"] {
        assert!(
            find_node(&surface.root, &format!("hierarchy.tab.{removed}")).is_none(),
            "{removed} is no longer a hierarchy tab"
        );
    }
}

#[test]
fn regular_hierarchy_tabs_keep_scene_and_lighting_labels_inside_the_panel() {
    let row = tabs(StudioUiPalette::IndustrialDark, false, "hierarchy");
    let total_width: f32 = row.children.iter().map(|tab| tab.layout.basis[0]).sum();

    assert_eq!(row.children.len(), 2);
    assert_eq!(
        row.children
            .iter()
            .map(|tab| tab.tooltip_key.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("app.scene"), Some("lighting.title")]
    );
    assert!(row.children.iter().all(|tab| tab.text_key.is_some()));
    assert!(total_width + row.layout.padding.left + row.layout.padding.right <= 530.0);
}

#[test]
fn compact_tabs_keep_icons_and_drop_text_labels() {
    let row = tabs(StudioUiPalette::IndustrialDark, true, "lighting");
    assert_eq!(row.children.len(), 2);
    assert!(row.children.iter().all(|tab| tab.text_key.is_none()));
    assert_eq!(
        row.children[1].icon.as_ref().map(|icon| icon.id),
        Some(UiIconId::Shaded)
    );
    assert!(row.children[1]
        .classes
        .iter()
        .any(|class| class == "hierarchy-tab-active"));
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
        false,
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
fn hierarchy_row_target_exposes_tree_keyboard_navigation() {
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
        false,
        None,
    );
    let target = find_node(&row, "hierarchy.row.0.select").expect("row target");
    for direction in ["up", "down", "left", "right", "home", "end"] {
        let expected = format!("hierarchy.navigate:{direction}:0");
        assert!(
            target.event_handlers.iter().any(|binding| matches!(
                &binding.action,
                raf_ui::UiAction::Command { name } if name.as_str() == expected.as_str()
            )),
            "missing {expected}"
        );
    }
}

#[test]
fn hierarchy_context_menu_is_bounded_inside_small_viewports() {
    let rect = context_menu_rect(Some([12.0, 190.0]), true, [240.0, 200.0]);

    assert!(rect.height < 200.0);
    assert!(rect.bottom() <= 192.0);
}

#[test]
fn badge_overflow_shows_only_two_icons_and_a_plus_count() {
    use crate::panels::hierarchy_model::details::{HierarchyBadge, HierarchyBadgeKind};

    let mut source = row(Primitive::Cube, false);
    source.badges = [
        HierarchyBadgeKind::Warning,
        HierarchyBadgeKind::Light,
        HierarchyBadgeKind::Mesh,
        HierarchyBadgeKind::Audio,
    ]
    .into_iter()
    .map(|kind| HierarchyBadge {
        key: kind.key(),
        section: kind.section(),
        count: 1,
        diagnostics: Vec::new(),
    })
    .collect();
    let node = row_node(
        StudioUiPalette::IndustrialDark,
        &source,
        &[],
        None,
        26.0,
        14.0,
        true,
        false,
        false,
        false,
        None,
    );
    let badges = (0..4)
        .filter_map(|index| find_node(&node, &format!("hierarchy.badge.0.{index}")))
        .collect::<Vec<_>>();
    assert_eq!(badges.len(), 2);
    assert_eq!(
        badges[0].icon.as_ref().map(|icon| icon.id),
        Some(UiIconId::Warning)
    );
    assert_eq!(
        badges[1].icon.as_ref().map(|icon| icon.id),
        Some(UiIconId::Sun)
    );
    let more = find_node(&node, "hierarchy.badges.more.0").expect("overflow counter");
    assert_eq!(more.text_value.as_deref(), Some("+2"));
}

#[test]
fn box_selection_rect_is_painted_from_the_view_projection() {
    let mut view = HierarchyView {
        total_rows: 1,
        entity_rows: 1,
        visible_start: 0,
        top_spacer: 0.0,
        bottom_spacer: 0.0,
        rows: vec![row(Primitive::Cube, false)],
        box_selection: Some(raf_ui::UiRect::new(8.0, 12.0, 60.0, 30.0)),
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
        false,
    );
    let overlay = find_node(&surface.root, "hierarchy.box-selection").expect("box selection");
    assert_eq!(overlay.layout.rect.map(|rect| rect.width), Some(60.0));

    view.box_selection = None;
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
        false,
    );
    assert!(find_node(&surface.root, "hierarchy.box-selection").is_none());
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
        assert!(find_node(&surface.root, &format!("hierarchy.primitive-option.{slug}")).is_some());
    }
}
