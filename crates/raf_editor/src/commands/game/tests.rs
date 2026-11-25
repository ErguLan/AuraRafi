//! Command-surface tests for the game command kernel.

use super::test_support::parsed_command;
use super::*;
use crate::commands::output::CommandLevel;
use crate::commands::parser::{parse_console_input, ParsedInput};

#[test]
fn shadows_entity_authoring_validates_both_flags_before_mutation() {
    let mut scene = SceneGraph::new();
    let id = scene.add_root_with_primitive("Caster", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let invalid = parsed_command(
        "game.update",
        serde_json::json!({"target":"Caster","casts_shadows":false,"receives_shadows":"invalid"}),
    );
    assert!(!execute("game.update", &invalid, &mut ctx).changed);
    assert!(ctx.scene.get(id).unwrap().casts_shadows);
    let command = parsed_command(
        "game.update",
        serde_json::json!({"target":"Caster","casts_shadows":false,"receives_shadows":false}),
    );
    let output = execute("game.update", &command, &mut ctx);
    assert!(output.changed);
    assert_eq!(output.json["entity"]["casts_shadows"], false);
    let node = ctx.scene.get(id).unwrap();
    assert!(!node.casts_shadows && !node.receives_shadows);
}

#[test]
fn add_primitive_uses_builtin_asset_manifest() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let ParsedInput::Command(command) =
        parse_console_input("/game.add primitive=cube name=Block x=1 y=2 z=3").unwrap()
    else {
        panic!("expected command");
    };
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.add", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert!(output.changed);
    let selected = ctx.selection.selected_node.expect("selected node");
    let node = ctx.scene.get(selected).expect("created node");
    assert_eq!(node.name, "Block");
    assert_eq!(node.primitive, Primitive::Cube);
    assert_eq!(node.position, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(
        node.source_asset.as_deref(),
        Some("builtin://primitive/cube")
    );
    assert_eq!(
        output.json["entity"]["source_asset"],
        "builtin://primitive/cube"
    );
}

#[test]
fn semantic_add_preserves_primitive_transform_and_color() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = parsed_command(
        "game.add",
        serde_json::json!({
            "kind": "sphere",
            "name": "Round_Product",
            "transform": {
                "position": [2.5, 1.25, -3.0],
                "rotation_deg": [0.0, 45.0, 10.0],
                "scale": [0.5, 2.0, 1.5]
            },
            "color_rgba": [12, 34, 56, 200]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.add", &command, &mut ctx);
    let id = ctx.selection.selected_node.expect("created entity");
    let node = ctx.scene.get(id).expect("created node");

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(node.primitive, Primitive::Sphere);
    assert_eq!(node.position, Vec3::new(2.5, 1.25, -3.0));
    assert_eq!(node.rotation, Vec3::new(0.0, 45.0, 10.0));
    assert_eq!(node.scale, Vec3::new(0.5, 2.0, 1.5));
    assert_eq!(node.color, NodeColor::rgba(12, 34, 56, 200));
    assert_eq!(
        output.json["entity"]["color_rgba"],
        serde_json::json!([12, 34, 56, 200])
    );
}

#[test]
fn structured_update_accepts_object_vectors_and_color() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let add = parsed_command(
        "game.add",
        serde_json::json!({"primitive": "cube", "name": "Shelf"}),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    execute("game.add", &add, &mut ctx);
    let id = ctx.selection.selected_node.expect("created entity");
    let uuid = ctx.scene.get(id).expect("created node").uuid;

    let update = parsed_command(
        "game.update",
        serde_json::json!({
            "target": uuid.to_string(),
            "transform": {
                "position": {"x": -1.0, "y": 2.0, "z": 3.0},
                "scale": {"x": 2.0, "y": 0.5, "z": 4.0}
            },
            "color_rgba": [220, 180, 40]
        }),
    );
    let output = execute("game.update", &update, &mut ctx);
    let node = ctx.scene.get(id).expect("updated node");

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(node.position, Vec3::new(-1.0, 2.0, 3.0));
    assert_eq!(node.scale, Vec3::new(2.0, 0.5, 4.0));
    assert_eq!(node.color, NodeColor::rgb(220, 180, 40));
}

#[test]
fn semantic_batch_keeps_different_shapes_separate() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = parsed_command(
        "game.batch",
        serde_json::json!({
            "operations": [
                {"name": "scene_create", "params": {"kind": "cube", "name": "Body", "transform": {"position": [0, 0, 0], "scale": [4, 1, 2]}, "color_rgba": [90, 90, 90, 255]}},
                {"name": "scene_create", "params": {"kind": "sphere", "name": "Display", "transform": {"position": [0, 1, 0], "scale": [1, 1, 1]}, "color_rgba": [30, 140, 220, 255]}},
                {"name": "scene_create", "params": {"kind": "cylinder", "name": "Post", "transform": {"position": [1, 2, 0], "scale": [0.2, 2, 0.2]}, "color_rgba": [200, 50, 30, 255]}}
            ]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.batch", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(ctx.scene.len(), 3);
    let nodes = ctx.scene.iter().map(|(_, node)| node).collect::<Vec<_>>();
    assert_eq!(nodes[0].primitive, Primitive::Cube);
    assert_eq!(nodes[1].primitive, Primitive::Sphere);
    assert_eq!(nodes[2].primitive, Primitive::Cylinder);
    assert_eq!(nodes[0].scale, Vec3::new(4.0, 1.0, 2.0));
    assert_eq!(nodes[1].position, Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(nodes[2].color, NodeColor::rgb(200, 50, 30));
}

#[test]
fn structured_build_creates_modular_groups_and_effective_values() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = parsed_command(
        "game.build",
        serde_json::json!({
            "groups": [
                {"name": "Structure"},
                {"name": "Products", "parent": "Structure"}
            ],
            "entities": [
                {
                    "kind": "cube",
                    "name": "Floor",
                    "parent": "Structure",
                    "transform": {
                        "position": [0.0, 0.5, -1.0],
                        "scale": [4.0, 0.2, 2.0]
                    },
                    "color_rgba": [60, 150, 90, 255]
                }
            ]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.build", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(output.json["build"]["groups"], 2);
    assert_eq!(output.json["build"]["entities"], 1);
    let structure = ctx.scene.find_node_by_name("Structure").expect("structure");
    let products = ctx.scene.find_node_by_name("Products").expect("products");
    let floor = ctx.scene.find_node_by_name("Floor").expect("floor");
    assert!(ctx.scene.get(structure).unwrap().is_folder);
    assert_eq!(ctx.scene.get(products).unwrap().parent, Some(structure));
    assert_eq!(ctx.scene.get(floor).unwrap().parent, Some(structure));
    assert_eq!(
        ctx.scene.get(floor).unwrap().position,
        Vec3::new(0.0, 0.5, -1.0)
    );
    assert_eq!(
        ctx.scene.get(floor).unwrap().scale,
        Vec3::new(4.0, 0.2, 2.0)
    );
    assert_eq!(
        ctx.scene.get(floor).unwrap().color,
        NodeColor::rgba(60, 150, 90, 255)
    );
}

#[test]
fn structured_build_orders_child_groups_after_their_parent() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = parsed_command(
        "game.build",
        serde_json::json!({
            "groups": [
                {"name": "Products", "parent": "Structure"},
                {"name": "Structure"}
            ],
            "entities": []
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.build", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    let structure = ctx.scene.find_node_by_name("Structure").expect("structure");
    let products = ctx.scene.find_node_by_name("Products").expect("products");
    assert_eq!(ctx.scene.get(products).unwrap().parent, Some(structure));
}

#[test]
fn real_world_design_profile_rejects_incomplete_structure_before_mutation() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = parsed_command(
        "game.build",
        serde_json::json!({
            "design_profile": "supermarket",
            "groups": [{"name": "Store"}],
            "entities": [
                {"kind": "cube", "name": "Floor", "parent": "Store"},
                {"kind": "cube", "name": "Entrance", "parent": "Store"},
                {"kind": "cube", "name": "Aisle_Main", "parent": "Store"},
                {"kind": "cube", "name": "Shelf_Main", "parent": "Store"}
            ]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.build", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Error);
    assert!(output.lines.join(" ").contains("enclosure"));
    assert_eq!(ctx.scene.len(), 0);
}

#[test]
fn repair_is_explicit_scoped_and_atomic() {
    let mut scene = SceneGraph::new();
    let store = scene.add_root_folder("Store");
    let shelf = scene.add_child_with_primitive(store, "Shelf", Primitive::Cube);
    let outside = scene.add_root_folder("Outside");
    scene.get_mut(shelf).unwrap().position = Vec3::ZERO;
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let successful = parsed_command(
        "game.repair",
        serde_json::json!({
            "root": "Store",
            "operations": [{
                "name": "scene_update",
                "params": {
                    "target": "Shelf",
                    "transform": {"position": [2.0, 0.0, 0.0]}
                }
            }]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.repair", &successful, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(ctx.scene.get(shelf).unwrap().position.x, 2.0);

    let outside_repair = parsed_command(
        "game.repair",
        serde_json::json!({
            "root": "Store",
            "operations": [{
                "name": "scene_update",
                "params": {
                    "target": "Outside",
                    "transform": {"position": [9.0, 0.0, 0.0]}
                }
            }]
        }),
    );
    let output = execute("game.repair", &outside_repair, &mut ctx);

    assert_eq!(output.level, CommandLevel::Error);
    assert_eq!(ctx.scene.get(outside).unwrap().position.x, 0.0);

    let atomic_failure = parsed_command(
        "game.repair",
        serde_json::json!({
            "operations": [
                {
                    "name": "scene_update",
                    "params": {
                        "target": "Shelf",
                        "transform": {"position": [4.0, 0.0, 0.0]}
                    }
                },
                {
                    "name": "scene_reparent",
                    "params": {"target": "Shelf", "parent": "Missing"}
                }
            ]
        }),
    );
    let output = execute("game.repair", &atomic_failure, &mut ctx);

    assert_eq!(output.level, CommandLevel::Error);
    assert_eq!(ctx.scene.get(shelf).unwrap().position.x, 2.0);
}

#[test]
fn reconcile_reuses_stable_keys_instead_of_duplicating_scene_nodes() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let first = parsed_command(
        "game.reconcile",
        serde_json::json!({
            "groups": [{"name": "Store", "stable_key": "store"}],
            "entities": [{
                "kind": "cube",
                "name": "Shelf_Left",
                "stable_key": "store.shelf.left",
                "parent": "key:store",
                "transform": {"position": [0.0, 1.0, 0.0]}
            }]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.reconcile", &first, &mut ctx);
    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(ctx.scene.len(), 2);
    let store = ctx
        .scene
        .iter()
        .find(|(_, node)| node.stable_key.as_deref() == Some("store"))
        .map(|(id, _)| id)
        .expect("store group");
    let shelf = ctx
        .scene
        .iter()
        .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.left"))
        .map(|(id, _)| id)
        .expect("shelf entity");

    let second = parsed_command(
        "game.reconcile",
        serde_json::json!({
            "groups": [{"name": "Storefront", "stable_key": "store"}],
            "entities": [{
                "kind": "cube",
                "name": "Shelf_Left_Wide",
                "stable_key": "store.shelf.left",
                "parent": "key:store",
                "transform": {"position": [2.0, 1.0, 0.0]}
            }]
        }),
    );
    let output = execute("game.reconcile", &second, &mut ctx);
    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(ctx.scene.len(), 2);
    assert_eq!(
        ctx.scene
            .iter()
            .find(|(_, node)| node.stable_key.as_deref() == Some("store"))
            .map(|(id, _)| id),
        Some(store)
    );
    let updated_shelf = ctx
        .scene
        .iter()
        .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.left"))
        .map(|(id, node)| (id, node));
    assert_eq!(updated_shelf.map(|(id, _)| id), Some(shelf));
    assert_eq!(updated_shelf.unwrap().1.name, "Shelf_Left_Wide");
    assert_eq!(updated_shelf.unwrap().1.position.x, 2.0);
}

#[test]
fn reconcile_profile_uses_existing_envelope_for_partial_deltas() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let initial = parsed_command(
        "game.build",
        serde_json::json!({
            "design_profile": "supermarket",
            "groups": [{"name": "Store", "stable_key": "store"}],
            "entities": [
                {"kind": "cube", "name": "Floor", "stable_key": "store.floor"},
                {"kind": "cube", "name": "Wall_North", "stable_key": "store.wall.north"},
                {"kind": "cube", "name": "Wall_South", "stable_key": "store.wall.south"},
                {"kind": "cube", "name": "Entrance", "stable_key": "store.entrance"},
                {"kind": "cube", "name": "Aisle_Main", "stable_key": "store.aisle.main"},
                {"kind": "cube", "name": "Shelf_Main", "stable_key": "store.shelf.main"}
            ]
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.build", &initial, &mut ctx);
    assert_eq!(output.level, CommandLevel::Info);
    let before_count = ctx.scene.len();
    let shelf = ctx
        .scene
        .iter()
        .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.main"))
        .map(|(id, _)| id)
        .expect("shelf entity");

    let partial = parsed_command(
        "game.reconcile",
        serde_json::json!({
            "design_profile": "supermarket",
            "entities": [{
                "kind": "cube",
                "name": "Shelf_Main_Wide",
                "stable_key": "store.shelf.main",
                "transform": {"scale": [2.0, 1.0, 1.0]}
            }]
        }),
    );
    let output = execute("game.reconcile", &partial, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(ctx.scene.len(), before_count);
    assert_eq!(
        ctx.scene
            .iter()
            .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.main"))
            .map(|(id, node)| (id, node.name.as_str())),
        Some((shelf, "Shelf_Main_Wide"))
    );
}

#[test]
fn reparent_command_preserves_world_transform_by_default() {
    let mut scene = SceneGraph::new();
    let source = scene.add_root_folder("Source");
    let target = scene.add_root_folder("Target");
    let child = scene.add_child_with_primitive(source, "Shelf", Primitive::Cube);
    scene.get_mut(source).unwrap().position = Vec3::new(4.0, 1.0, 0.0);
    scene.get_mut(target).unwrap().position = Vec3::new(-2.0, 0.0, 3.0);
    scene.get_mut(child).unwrap().position = Vec3::new(1.0, 2.0, 3.0);
    let before = scene.world_matrix(child).to_cols_array();
    let command = parsed_command(
        "game.reparent",
        serde_json::json!({"target": "Shelf", "parent": "Target"}),
    );
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.reparent", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(ctx.scene.get(child).unwrap().parent, Some(target));
    let after = ctx.scene.world_matrix(child).to_cols_array();
    for (before, after) in before.iter().zip(after) {
        assert!((*before - after).abs() < 0.001);
    }
}

#[test]
fn duplicate_supports_parent_count_and_spacing() {
    let mut scene = SceneGraph::new();
    let source_parent = scene.add_root_folder("Source");
    let target_parent = scene.add_root_folder("Products");
    scene.add_child_with_primitive(source_parent, "Can", Primitive::Cylinder);
    let command = parsed_command(
        "game.duplicate",
        serde_json::json!({
            "target": "Can",
            "parent": "Products",
            "name": "Can Copy",
            "offset": [0.0, 0.0, 0.0],
            "count": 3,
            "axis": "x",
            "spacing": 0.5,
            "preserve_world": false
        }),
    );
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.duplicate", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert_eq!(output.json["count"], 3);
    assert_eq!(ctx.selection.selected_nodes.len(), 3);
    for (index, id) in ctx.selection.selected_nodes.iter().enumerate() {
        let node = ctx.scene.get(*id).unwrap();
        assert_eq!(node.parent, Some(target_parent));
        assert_eq!(node.position.x, index as f32 * 0.5);
    }
}

#[test]
fn snap_places_entity_on_top_of_surface_bounds() {
    let mut scene = SceneGraph::new();
    let shelf = scene.add_root_with_primitive("Shelf", Primitive::Cube);
    let product = scene.add_root_with_primitive("Product", Primitive::Cube);
    scene.get_mut(shelf).unwrap().scale = Vec3::new(4.0, 0.2, 2.0);
    scene.get_mut(product).unwrap().scale = Vec3::new(0.5, 1.0, 0.5);
    let command = parsed_command(
        "game.snap",
        serde_json::json!({
            "target": "Product",
            "mode": "surface",
            "snap_to": "Shelf",
            "axis": "y",
            "placement": "after",
            "gap": 0.05
        }),
    );
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.snap", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    let shelf_max = world_bounds(ctx.scene, shelf).unwrap().1.y;
    let product_min = world_bounds(ctx.scene, product).unwrap().0.y;
    assert!((product_min - shelf_max - 0.05).abs() < 0.001);
}

#[test]
fn malformed_structured_transform_does_not_create_default_entity() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = parsed_command(
        "game.add",
        serde_json::json!({
            "kind": "cube",
            "name": "Invalid",
            "transform": {"position": [1, 2]}
        }),
    );
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.add", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Error);
    assert_eq!(ctx.scene.len(), 0);
    assert_eq!(output.json["error"]["code"], "invalid_vector_shape");
}

#[test]
fn batch_is_atomic_when_a_later_operation_fails() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let command = ParsedCommand {
        raw: "/game.batch".to_string(),
        name: "game.batch".to_string(),
        args: std::collections::BTreeMap::from([(
            "operations".to_string(),
            serde_json::to_string(&serde_json::json!([
                {"name": "game.add", "params": {"primitive": "cube", "name": "Shelf"}},
                {"name": "game.delete", "params": {"target": "does-not-exist"}}
            ]))
            .unwrap(),
        )]),
        positional: Vec::new(),
        structured_args: None,
    };
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };

    let output = execute("game.batch", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Error);
    assert_eq!(ctx.scene.len(), 0);
}

#[test]
fn semantic_parent_and_update_preserve_stable_entity_identity() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let parent = scene.add_root_folder("Store");
    let add = ParsedCommand {
        raw: "/game.add".to_string(),
        name: "game.add".to_string(),
        args: std::collections::BTreeMap::from([
            ("primitive".to_string(), "cube".to_string()),
            ("name".to_string(), "Shelf".to_string()),
            ("parent".to_string(), parent.0.to_string()),
        ]),
        positional: Vec::new(),
        structured_args: None,
    };
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    execute("game.add", &add, &mut ctx);
    let id = ctx.selection.selected_node.expect("created node");
    let uuid = ctx.scene.get(id).unwrap().uuid;
    let update = ParsedCommand {
        raw: "/game.update".to_string(),
        name: "game.update".to_string(),
        args: std::collections::BTreeMap::from([
            ("target".to_string(), uuid.to_string()),
            ("name".to_string(), "Shelf_Left".to_string()),
            ("x".to_string(), "2".to_string()),
        ]),
        positional: Vec::new(),
        structured_args: None,
    };
    execute("game.update", &update, &mut ctx);

    assert_eq!(
        ctx.scene.node_path(id).as_deref(),
        Some("/Store/Shelf_Left")
    );
    assert_eq!(ctx.scene.get(id).unwrap().uuid, uuid);
    assert_eq!(ctx.scene.get(id).unwrap().position.x, 2.0);
}

#[test]
fn camera_command_is_atomic_preserves_parts_and_reports_real_changes() {
    let mut scene = SceneGraph::new();
    let part = scene.add_root_with_primitive("Part", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let add = parsed_command(
        "game.camera",
        serde_json::json!({"action":"add","target":"Part","fov":70}),
    );
    assert!(execute("game.camera", &add, &mut ctx).changed);
    assert!(!execute("game.camera", &add, &mut ctx).changed);
    assert_eq!(ctx.scene.get(part).unwrap().primitive, Primitive::Cube);
    let invalid = parsed_command(
        "game.camera",
        serde_json::json!({"target":"Part","near":2000}),
    );
    assert_eq!(
        execute("game.camera", &invalid, &mut ctx).level,
        CommandLevel::Error
    );
    assert_eq!(
        ctx.scene
            .get(part)
            .unwrap()
            .game_camera
            .as_ref()
            .unwrap()
            .near,
        0.1
    );
    let create = parsed_command(
        "game.camera",
        serde_json::json!({"action":"create","near":-1}),
    );
    assert_eq!(
        execute("game.camera", &create, &mut ctx).level,
        CommandLevel::Error
    );
    assert_eq!(ctx.scene.len(), 1);
    let fuzzy = parsed_command(
        "game.camera",
        serde_json::json!({"target":"Par","action":"remove"}),
    );
    assert_eq!(
        execute("game.camera", &fuzzy, &mut ctx).level,
        CommandLevel::Error
    );
}

#[test]
fn authoring_capsule_dimensions_validate_and_round_trip_without_duplicate_size() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let add = parsed_command(
        "game.add",
        serde_json::json!({"primitive":"capsule","name":"Capsule","size":[2,6,2]}),
    );
    let output = execute("game.add", &add, &mut ctx);
    assert!(output.changed, "{:?}", output.lines);
    let id = ctx.selection.selected_node.unwrap();
    assert_eq!(ctx.scene.get(id).unwrap().scale, Vec3::new(2.0, 3.0, 2.0));
    let before = ctx.scene.clone();
    let bad = parsed_command(
        "game.update",
        serde_json::json!({"target":"Capsule","size":[1,2,1],"transform":{"scale":[1,1,1]}}),
    );
    assert_eq!(
        execute("game.update", &bad, &mut ctx).level,
        CommandLevel::Error
    );
    assert_eq!(ctx.scene.render_fingerprint(), before.render_fingerprint());
    let serialized = ron::to_string(ctx.scene).unwrap();
    let restored: SceneGraph = ron::from_str(&serialized).unwrap();
    assert_eq!(restored.get(id).unwrap().primitive, Primitive::Capsule);
    let prefab = raf_core::scene::prefab::ScenePrefab::capture(&restored, id).unwrap();
    let mut copy = SceneGraph::new();
    let copied = prefab.instantiate(&mut copy, None).unwrap();
    assert_eq!(copy.get(copied).unwrap().scale, Vec3::new(2.0, 3.0, 2.0));
}

#[test]
fn game_add_components_cover_inspector_fields_and_reject_escape_paths() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let add = parsed_command(
        "game.add",
        serde_json::json!({
            "primitive": "cube",
            "name": "Crate",
            "texture": "assets/generated/wood.png",
            "collider": {"type": "aabb", "offset": [0.0, 0.5, 0.0]},
            "rigid_body": {"enabled": true, "body_type": "dynamic", "mass": 2.0},
            "audio": {"enabled": true, "clip": "assets/audio/hit.wav", "volume": 0.5},
            "camera": {"fov_degrees": 75.0},
            "scripts": ["scripts/test.rhai"]
        }),
    );
    let output = execute("game.add", &add, &mut ctx);
    assert!(output.changed, "{:?}", output.lines);
    let id = ctx.selection.selected_node.expect("created entity");
    let node = ctx.scene.get(id).expect("live entity");
    assert_eq!(
        node.base_color_texture.as_deref(),
        Some("assets/generated/wood.png")
    );
    assert_eq!(node.collider.collider_type, ColliderType::Aabb);
    assert_eq!(node.collider.offset, Vec3::new(0.0, 0.5, 0.0));
    assert!(node.rigid_body.enabled);
    assert_eq!(
        node.rigid_body.body_type,
        raf_core::scene::RigidBodyType::Dynamic
    );
    assert_eq!(node.rigid_body.mass, 2.0);
    assert_eq!(node.audio_source.clip, "audio/hit.wav");
    assert_eq!(node.audio_source.volume, 0.5);
    assert_eq!(
        node.game_camera.as_ref().map(|camera| camera.fov_degrees),
        Some(75.0)
    );
    assert_eq!(node.scripts, vec!["assets/scripts/test.rhai"]);

    let before = ctx.scene.all_live_ids();
    let bad = parsed_command(
        "game.add",
        serde_json::json!({
            "primitive": "cube",
            "name": "Escape",
            "texture": "../escape.png"
        }),
    );
    assert_eq!(
        execute("game.add", &bad, &mut ctx).level,
        CommandLevel::Error
    );
    assert_eq!(ctx.scene.all_live_ids(), before);
    assert!(ctx.scene.iter().all(|(_, node)| node.name != "Escape"));
}

#[test]
fn mesh_asset_place_accepts_cached_reference_without_host_file() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let command = parsed_command(
        "game.mesh_asset",
        serde_json::json!({
            "action": "place",
            "mesh_asset": {
                "path": "models/store.glb",
                "content_hash": "fnv1a64:0123456789abcdef",
                "import_scale": 1.0
            },
            "name": "Store",
            "position": [1.0, 2.0, 3.0],
            "scale": [2.0, 2.0, 2.0]
        }),
    );
    let output = execute("game.mesh_asset", &command, &mut ctx);
    assert_eq!(output.level, CommandLevel::Info);
    assert!(output.changed);
    let id = ctx.selection.selected_node.expect("placed model selected");
    assert_eq!(ctx.selection.selected_nodes, vec![id]);
    let node = ctx.scene.get(id).expect("placed node");
    assert_eq!(node.name, "Store");
    assert_eq!(node.primitive, Primitive::Empty);
    assert_eq!(node.position, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(node.scale, Vec3::splat(2.0));
    let mesh_asset = node.mesh_asset.as_ref().expect("mesh asset assigned");
    assert_eq!(mesh_asset.path, "models/store.glb");
    assert_eq!(
        mesh_asset.content_hash.as_deref(),
        Some("fnv1a64:0123456789abcdef")
    );
    assert_eq!(output.json["mesh_asset"]["path"], "models/store.glb");
}

#[test]
fn mesh_asset_place_rejects_traversal_paths() {
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let before = ctx.scene.all_live_ids();
    let command = parsed_command(
        "game.mesh_asset",
        serde_json::json!({"action": "place", "path": "../escape.glb"}),
    );
    let output = execute("game.mesh_asset", &command, &mut ctx);
    assert_eq!(output.level, CommandLevel::Error);
    assert_eq!(ctx.scene.all_live_ids(), before);
}

#[test]
fn mesh_asset_remove_clears_the_reference() {
    let mut scene = SceneGraph::new();
    let id = scene.add_root_with_primitive("Mesh", Primitive::Empty);
    scene.get_mut(id).unwrap().mesh_asset =
        Some(raf_core::scene::MeshAssetRef::new("models/store.glb"));
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let command = parsed_command(
        "game.mesh_asset",
        serde_json::json!({"action": "remove", "target": "Mesh"}),
    );
    let output = execute("game.mesh_asset", &command, &mut ctx);
    assert!(output.changed);
    assert!(ctx.scene.get(id).unwrap().mesh_asset.is_none());
    assert!(output.json["mesh_asset"].is_null());
    assert_eq!(
        output.json["removed_mesh_asset"]["path"],
        "models/store.glb"
    );
}

#[test]
fn mesh_enter_converts_a_primitive_into_editable_topology() {
    let mut scene = SceneGraph::new();
    let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let command = parsed_command(
        "game.mesh",
        serde_json::json!({"action": "enter", "target": "Cube"}),
    );

    let output = execute("game.mesh", &command, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert!(output.changed, "{:?}", output.lines);
    assert_eq!(output.json["has_editable_mesh"], true);
    let mesh = ctx
        .scene
        .get(cube)
        .and_then(|node| node.editable_mesh.as_ref())
        .expect("editable mesh");
    assert!(mesh.face_count() > 0);
    assert_eq!(
        output.json["editable_mesh"]["face_count"],
        mesh.face_count()
    );
    assert_eq!(ctx.selection.selected_node, Some(cube));
}

#[test]
fn mesh_set_replaces_topology_and_rejects_out_of_range_indices() {
    let mut scene = SceneGraph::new();
    let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let set = parsed_command(
        "game.mesh",
        serde_json::json!({
            "action": "set",
            "target": "Cube",
            "vertices": [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            "indices": [0, 1, 2]
        }),
    );

    let output = execute("game.mesh", &set, &mut ctx);

    assert!(output.changed, "{:?}", output.lines);
    assert_eq!(output.json["editable_mesh"]["vertex_count"], 3);
    assert_eq!(output.json["editable_mesh"]["face_count"], 1);
    let mesh = ctx.scene.get(cube).unwrap().editable_mesh.clone().unwrap();
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.indices, vec![0, 1, 2]);

    let invalid = parsed_command(
        "game.mesh",
        serde_json::json!({
            "action": "set",
            "target": "Cube",
            "vertices": [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            "indices": [0, 1, 3]
        }),
    );
    let output = execute("game.mesh", &invalid, &mut ctx);

    assert_eq!(output.level, CommandLevel::Error);
    assert!(!output.changed);
    let mesh = ctx.scene.get(cube).unwrap().editable_mesh.clone().unwrap();
    assert_eq!(mesh.indices, vec![0, 1, 2]);
}

#[test]
fn mesh_extrude_on_a_box_face_creates_side_faces() {
    let mut scene = SceneGraph::new();
    let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let enter = parsed_command(
        "game.mesh",
        serde_json::json!({"action": "enter", "target": "Cube"}),
    );
    execute("game.mesh", &enter, &mut ctx);
    let before = ctx
        .scene
        .get(cube)
        .and_then(|node| node.editable_mesh.as_ref())
        .map(|mesh| mesh.face_count())
        .expect("editable mesh");
    let extrude = parsed_command(
        "game.mesh",
        serde_json::json!({
            "action": "extrude",
            "target": "Cube",
            "faces": [0],
            "distance": 0.25
        }),
    );

    let output = execute("game.mesh", &extrude, &mut ctx);

    assert!(output.changed, "{:?}", output.lines);
    let after = ctx
        .scene
        .get(cube)
        .and_then(|node| node.editable_mesh.as_ref())
        .map(|mesh| mesh.face_count())
        .expect("editable mesh");
    let created = output.json["created_faces"].as_array().unwrap().len();
    assert!(after > before);
    assert!(created > 0);
    assert_eq!(after, before + created);
    assert_eq!(output.json["editable_mesh"]["face_count"], after);
}

#[test]
fn mesh_inspect_topology_returns_positions_and_indices() {
    let mut scene = SceneGraph::new();
    let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let enter = parsed_command(
        "game.mesh",
        serde_json::json!({"action": "enter", "target": "Cube"}),
    );
    execute("game.mesh", &enter, &mut ctx);
    let (vertex_count, index_count) = ctx
        .scene
        .get(cube)
        .and_then(|node| node.editable_mesh.as_ref())
        .map(|mesh| (mesh.vertices.len(), mesh.indices.len()))
        .expect("editable mesh");
    let inspect = parsed_command(
        "game.mesh",
        serde_json::json!({"action": "inspect", "target": "Cube", "topology": true}),
    );

    let output = execute("game.mesh", &inspect, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert!(!output.changed);
    assert_eq!(output.json["has_editable_mesh"], true);
    assert!(vertex_count >= 3);
    assert!(index_count >= 3);
    assert_eq!(
        output.json["editable_mesh"]["positions"]
            .as_array()
            .unwrap()
            .len(),
        vertex_count
    );
    assert_eq!(
        output.json["editable_mesh"]["indices"]
            .as_array()
            .unwrap()
            .len(),
        index_count
    );
}

#[test]
fn mesh_revert_clears_editable_mesh_and_reports_the_change() {
    let mut scene = SceneGraph::new();
    let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut ctx = GameCommandContext {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
    };
    let enter = parsed_command(
        "game.mesh",
        serde_json::json!({"action": "enter", "target": "Cube"}),
    );
    execute("game.mesh", &enter, &mut ctx);
    let revert = parsed_command(
        "game.mesh",
        serde_json::json!({"action": "revert", "target": "Cube"}),
    );

    let output = execute("game.mesh", &revert, &mut ctx);

    assert_eq!(output.level, CommandLevel::Info);
    assert!(output.changed, "{:?}", output.lines);
    assert_eq!(output.json["has_editable_mesh"], false);
    assert!(output.json["editable_mesh"].is_null());
    assert!(ctx.scene.get(cube).unwrap().editable_mesh.is_none());

    let output = execute("game.mesh", &revert, &mut ctx);
    assert!(!output.changed);
}
