
use super::audit::navigation_text;
use super::scene::scene_item;
use super::*;
use crate::scene::{Primitive, SceneGraph};
use glam::Vec3;
use serde_json::json;

#[test]
fn camera_components_are_observable_and_lens_changes_produce_semantic_diffs() {
    let mut scene = SceneGraph::new();
    let part = scene.add_root_with_primitive("CameraPart", Primitive::Cube);
    let before = scene_fingerprint(&scene);
    scene.get_mut(part).unwrap().game_camera = Some(crate::runtime_config::GameCamera::default());
    let added = scene_fingerprint(&scene);
    let uuid = scene.get(part).unwrap().uuid.to_string();
    assert_eq!(
        scene_diff(&before, &added).unwrap()["updated"],
        json!([uuid])
    );
    let item = scene_item(&scene, part, true).unwrap();
    assert_eq!(item["has_camera"], true);
    assert_eq!(item["primitive"], "Cube");
    assert_eq!(item["game_camera"]["fov_degrees"], 60.0);
    scene
        .get_mut(part)
        .unwrap()
        .game_camera
        .as_mut()
        .unwrap()
        .fov_degrees = 75.0;
    assert!(scene_diff(&added, &scene_fingerprint(&scene)).is_some());
    scene.get_mut(part).unwrap().game_camera = None;
    assert_eq!(scene_fingerprint(&scene), before);
}

#[test]
fn scene_context_uses_one_live_count_and_sanitizes_display_names() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_with_primitive("Store", Primitive::Empty);
    let child = scene.add_child_with_primitive(root, "Shelf\u{0008}", Primitive::Cube);
    let context = scene_context(&scene, &[child]);
    assert_eq!(context["entity_count"], 2);
    assert_eq!(context["outline_count"], 2);
    assert_eq!(context["selected"][0]["name"], "Shelf");
    assert_eq!(context["outline"][1]["path"], "/Store/Shelf");
}

#[test]
fn broken_names_remain_visible_and_addressable_without_mutating_scene() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_folder("Store");
    let unnamed = scene.add_child_with_primitive(root, "\u{0008}", Primitive::Cube);
    let child = scene.add_child_with_primitive(unnamed, "Shelf", Primitive::Cube);

    let context = scene_context(&scene, &[]);
    assert_eq!(context["entity_count"], 3);
    assert_eq!(context["outline_count"], 3);
    assert_eq!(
        context["outline"][1]["name"],
        json!(format!("Entity_{}", unnamed.0))
    );
    assert_eq!(
        context["outline"][2]["path"],
        json!(format!("/Store/Entity_{}/Shelf", unnamed.0))
    );
    assert_eq!(
        resolve_target(
            &scene,
            &format!("entity:{}", scene.get(unnamed).unwrap().uuid)
        ),
        Some(unnamed)
    );
    assert!(world_bounds(&scene, unnamed).is_some());
    assert_eq!(scene.get(unnamed).unwrap().name, "\u{0008}");
    assert_eq!(scene.get(child).unwrap().name, "Shelf");
}

#[test]
fn scene_query_is_bounded_and_resolves_clean_names() {
    let mut scene = SceneGraph::new();
    scene.add_root_with_primitive("Shelf_Left", Primitive::Cube);
    scene.add_root_with_primitive("Shelf_Right", Primitive::Cube);
    let result = scene_query(&scene, &json!({"query": "shelf", "limit": 1}), &[]);
    assert_eq!(result.data["total"], 2);
    assert_eq!(result.data["returned"], 1);
    assert_eq!(result.data["has_more"], true);
}

#[test]
fn scene_query_filters_semantic_roles_and_exact_tags() {
    let mut scene = SceneGraph::new();
    let shelf = scene.add_root_with_primitive("Shelf", Primitive::Cube);
    let product = scene.add_root_with_primitive("Product", Primitive::Cube);
    scene.get_mut(shelf).unwrap().semantic_role = Some("store.shelf".to_string());
    scene.get_mut(shelf).unwrap().tags = vec!["bodega".to_string(), "fixture".to_string()];
    scene.get_mut(product).unwrap().semantic_role = Some("store.product".to_string());
    scene.get_mut(product).unwrap().tags = vec!["bodega".to_string()];

    let result = scene_query(
        &scene,
        &json!({"semantic_role":"store.shelf", "tags":["bodega", "fixture"]}),
        &[],
    );

    assert_eq!(result.data["total"], 1);
    assert_eq!(result.data["items"][0]["name"], "Shelf");
}

#[test]
fn scene_outline_honors_requested_hierarchy_depth() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_folder("Store");
    let aisle = scene.add_child_folder(root, "Aisles");
    scene.add_child_with_primitive(aisle, "Shelf", Primitive::Cube);

    let result = scene_outline(&scene, &json!({"root": "Store", "depth": 1}), &[]);

    assert_eq!(result.data["total"], 2);
    assert_eq!(result.data["items"][0]["name"], "Store");
    assert_eq!(result.data["items"][1]["name"], "Aisles");
}

#[test]
fn spatial_map_reports_extents_and_renderable_overlaps() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_folder("Store");
    let floor = scene.add_child_with_primitive(root, "Floor", Primitive::Cube);
    let shelf = scene.add_child_with_primitive(root, "Shelf", Primitive::Cube);
    scene.get_mut(floor).unwrap().scale = Vec3::new(4.0, 0.2, 4.0);
    scene.get_mut(shelf).unwrap().position = Vec3::new(0.25, 0.0, 0.0);
    let result = scene_spatial_map(
        &scene,
        &json!({"root": "Store", "check_collisions": true}),
        &[],
    );
    assert!(result.is_success());
    assert_eq!(result.data["total"], 2);
    assert!(result.data["bounds"]["size"][0].as_f64().unwrap() > 0.0);
    assert_eq!(result.data["overlaps"].as_array().unwrap().len(), 1);
}

#[test]
fn spatial_map_does_not_report_parent_child_bounds_as_collisions() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_folder("Store");
    let container = scene.add_child_with_primitive(root, "ShelfAssembly", Primitive::Cube);
    scene.add_child_with_primitive(container, "ShelfBoard", Primitive::Cube);

    let result = scene_spatial_map(
        &scene,
        &json!({"root": "Store", "check_collisions": true}),
        &[],
    );

    assert!(result.is_success());
    assert_eq!(result.data["overlaps"].as_array().unwrap().len(), 0);
}

#[test]
fn overlap_check_returns_a_snap_repair_suggestion() {
    let mut scene = SceneGraph::new();
    scene.add_root_with_primitive("Shelf A", Primitive::Cube);
    let second = scene.add_root_with_primitive("Shelf B", Primitive::Cube);
    scene.get_mut(second).unwrap().position.x = 0.25;

    let result = scene_check_overlaps(&scene, &json!({"margin":0.1}), &[]);

    assert_eq!(result.data["overlap_count"], 1);
    assert_eq!(result.data["pairs"][0]["smallest_axis"], "x");
    assert_eq!(result.data["pairs"][0]["suggested_snap"]["axis"], "x");
}

#[test]
fn design_audit_reports_missing_envelope_features() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_folder("Store");
    scene.add_child_with_primitive(root, "Floor", Primitive::Cube);
    let result = scene_design_audit(&scene, &json!({"root": "Store"}), &[]);
    assert!(!result.is_success());
    assert_eq!(result.verification.unwrap().status, "failed");
    assert!(result.data["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|feature| feature["feature"] == "enclosure"));
}

#[test]
fn verification_count_uses_explicit_targets_when_present() {
    let mut scene = SceneGraph::new();
    scene.add_root_with_primitive("Floor", Primitive::Cube);
    scene.add_root_with_primitive("Shelf", Primitive::Cube);
    let result = scene_verify(
        &scene,
        &json!({"targets": ["Shelf"], "expected_count": 1}),
        &[],
    );
    assert!(result.is_success());
    assert_eq!(result.data["expected_scope_count"], 1);
}

#[test]
fn verification_compares_expected_state_in_the_shared_core() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root_folder("Store");
    let shelf = scene.add_child_with_primitive(root, "Shelf", Primitive::Cube);
    scene.get_mut(shelf).unwrap().position = Vec3::new(1.0, 2.0, 3.0);
    scene.get_mut(shelf).unwrap().color = crate::scene::NodeColor::rgba(20, 40, 60, 255);

    let expected = json!({
        "targets": ["Shelf"],
        "expected": {
            "primitive": "cube",
            "transform": {"position": [1.0, 2.0, 3.0]},
            "color_rgba": [20, 40, 60, 255],
            "parent": "Store"
        }
    });
    let result = scene_verify(&scene, &expected, &[]);
    assert!(result.is_success());

    let mismatch = scene_verify(
        &scene,
        &json!({
            "targets": ["Shelf"],
            "expected": {"transform": {"position": [9.0, 2.0, 3.0]}}
        }),
        &[],
    );
    assert!(!mismatch.is_success());
    assert_eq!(mismatch.verification.unwrap().status, "failed");
}

#[test]
fn scene_verification_reports_missing_targets() {
    let scene = SceneGraph::new();
    let result = scene_verify(&scene, &json!({"targets": ["Missing"]}), &[]);
    assert_eq!(result.verification.unwrap().status, "failed");
}

#[test]
fn asset_catalog_and_selection_are_bounded_observations() {
    let mut scene = SceneGraph::new();
    let id = scene.add_root_with_primitive("Shelf", Primitive::Cube);
    let selection = selection_info(&scene, &[id]);
    assert_eq!(selection.data["count"], 1);
    assert_eq!(selection.data["items"][0]["name"], "Shelf");

    let assets = assets_catalog(
        &scene,
        &[
            "models/Shelf.glb".to_string(),
            "scripts/store.rhai".to_string(),
        ],
        &json!({"usage":"unused", "kind":"model"}),
        false,
        None,
    );
    assert_eq!(assets.data["items"][0]["kind"], "model");
    assert_eq!(assets.data["items"][0]["used"], false);
    assert_eq!(assets.data["total"], 1);

    let scripts = scripts_catalog(&scene, &["scripts/store.rhai".to_string()], &json!({}));
    assert_eq!(scripts.data["items"][0]["path"], "scripts/store.rhai");
}

#[test]
fn asset_recommendations_explain_semantic_filename_matches() {
    let scene = SceneGraph::new();
    let result = assets_recommend(
        &scene,
        &[
            "models/supermarket_shelf.glb".to_string(),
            "audio/checkout.wav".to_string(),
        ],
        &json!({"intent":"estante de supermercado", "kind":"model"}),
        false,
        None,
    );

    assert_eq!(
        result.data["items"][0]["path"],
        "models/supermarket_shelf.glb"
    );
    assert!(result.data["items"][0]["reasons"]
        .as_array()
        .is_some_and(|reasons| !reasons.is_empty()));
}

#[test]
fn spatial_map_items_include_per_entity_world_bounds() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root("Store");
    let shelf = scene.add_child(root, "Shelf");
    if let Some(node) = scene.get_mut(shelf) {
        node.primitive = Primitive::Cube;
        node.scale = Vec3::new(2.0, 1.0, 3.0);
    }
    let result = scene_spatial_map(&scene, &json!({"root":"Store"}), &[]);
    assert_eq!(result.data["returned"], 1);
    let bounds = &result.data["items"][0]["world_bounds"];
    assert!(bounds.get("min").is_some(), "{:?}", result.data);
    let size = bounds["size"].as_array().expect("size array");
    assert!((size[0].as_f64().unwrap() - 2.0).abs() < 0.001);
    assert!((size[2].as_f64().unwrap() - 3.0).abs() < 0.001);
}

#[test]
fn scene_digest_reports_components_cameras_and_bounds() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root("World");
    let camera = scene.add_child(root, "Camera");
    if let Some(node) = scene.get_mut(camera) {
        node.primitive = Primitive::Empty;
        node.game_camera = Some(crate::runtime_config::GameCamera::default());
    }
    let crate_id = scene.add_child(root, "Crate");
    if let Some(node) = scene.get_mut(crate_id) {
        node.primitive = Primitive::Cube;
        node.scale = Vec3::splat(2.0);
        node.base_color_texture = Some("assets/generated/wood.png".to_string());
        node.collider.collider_type = crate::scene::ColliderType::Aabb;
        node.rigid_body.enabled = true;
        node.audio_source.enabled = true;
        node.tags = vec!["prop.crate".to_string()];
    }
    let result = scene_digest(
        &scene,
        &["assets/generated/wood.png".to_string()],
        &json!({}),
        &[],
    );
    assert_eq!(result.data["counts"]["live"], 3);
    assert_eq!(result.data["counts"]["cameras"], 1);
    assert_eq!(result.data["components"]["textured"], 1);
    assert_eq!(result.data["components"]["collider"], 1);
    assert_eq!(result.data["components"]["rigid_body"], 1);
    assert_eq!(result.data["components"]["audio"], 1);
    assert!(result.data["bounds"].get("min").is_some());
    assert_eq!(result.data["top_tags"][0]["tag"], "prop.crate");
}

#[test]
fn scene_inspect_reports_partial_targets_without_total_failure() {
    let mut scene = SceneGraph::new();
    let root = scene.add_root("A");
    let result = scene_inspect(&scene, &json!({"targets": ["A", "Missing"]}), &[]);
    assert_eq!(result.data["found"], true);
    assert_eq!(result.data["status"], "partial");
    assert_eq!(result.data["partial"], true);
    assert_eq!(result.data["unresolved_targets"][0], "Missing");
    assert_eq!(result.data["items"][0]["name"], "A");
    assert!(result.data["entity"].is_object());
    assert_eq!(result.data["items"].as_array().map(Vec::len), Some(1));
    let _ = root;
}

#[test]
fn navigation_audit_reports_when_a_goal_is_unreachable() {
    let mut open = SceneGraph::new();
    let floor = open.add_root("Floor");
    if let Some(node) = open.get_mut(floor) {
        node.primitive = Primitive::Plane;
        node.scale = Vec3::new(10.0, 1.0, 10.0);
    }
    let spawn = open.add_root("Spawn");
    if let Some(node) = open.get_mut(spawn) {
        node.position = Vec3::new(0.0, 0.0, -3.0);
    }
    let goal = open.add_root("Goal");
    if let Some(node) = open.get_mut(goal) {
        node.position = Vec3::new(0.0, 0.0, 3.0);
    }
    assert!(navigation_text(open.get(goal).unwrap()).contains("goal"));
    let open_result = scene_navigation_audit(&open, &json!({"resolution": 0.5}), &[]);
    assert_eq!(
        open_result.data["status"], "passed",
        "{:?}",
        open_result.data
    );

    let mut blocked = open.clone();
    let wall = blocked.add_root("Wall");
    if let Some(node) = blocked.get_mut(wall) {
        node.primitive = Primitive::Cube;
        node.position = Vec3::new(0.0, 1.0, 0.0);
        node.scale = Vec3::new(10.0, 2.0, 0.2);
        node.collider.collider_type = crate::scene::ColliderType::Aabb;
    }
    let blocked_result = scene_navigation_audit(&blocked, &json!({"resolution": 0.5}), &[]);
    assert_eq!(
        blocked_result.data["status"], "failed",
        "{:?}",
        blocked_result.data
    );
    assert_eq!(blocked_result.data["reachable_goals"], 0);
    assert_eq!(blocked_result.data["goals"][0]["reachable"], false);
}

#[test]
fn compact_result_lines_removes_renderer_dump_lines() {
    let lines = vec![
        "Command executed:".to_string(),
        "title: Created shelf".to_string(),
        "mesh_indices: 36".to_string(),
        "local_vertex_0: [0, 0, 0]".to_string(),
        "changed: true".to_string(),
    ];

    assert_eq!(
        compact_result_lines(&lines),
        vec!["title: Created shelf", "changed: true"]
    );
}
