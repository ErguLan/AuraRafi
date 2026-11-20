//! World plan, recipe, script and pipeline tests.

use super::super::execute;
use super::super::query::parsed_command_from_params;
use super::super::test_support::parsed_command;
use super::*;
use crate::commands::output::CommandLevel;

    fn temp_project(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "raf-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn authoring_script_emits_reviewable_plan_and_rejects_sandbox_escape() {
        let root = temp_project("world-script");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let command = parsed_command_from_params(
            "world.script",
            serde_json::json!({
                "script": "world::group(\"Scripted\"); world::group(\"Lamps\"); world::parent(\"Lamps\"); world::grid(\"street_lamp\", 3, 1, 6.0, 6.0); world::parent_clear(); world::place(\"crate\", 0.0, 0.0, 4.0);"
            }),
        );
        let output = execute_world_script(&command, &mut ctx);
        assert!(
            !matches!(output.level, CommandLevel::Error),
            "{:?}",
            output.lines
        );
        assert_eq!(output.json["script"]["sandbox"], "rhai-authoring-v1");
        assert!(output.json["script"]["operations"].as_u64().unwrap() >= 2);
        assert_eq!(output.json["script"]["groups"], 2);
        let script_path = output.json["script"]["path"].as_str().unwrap();
        assert!(root.join(script_path).exists());
        let plan_id = output.json["plan_id"].as_str().unwrap();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        assert!(plan_path.exists());
        let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        let groups = plan["groups"].as_array().unwrap();
        assert!(groups.iter().any(|group| group["name"] == "Lamps"));
        assert!(plan["entities"].as_array().unwrap().len() >= 4);
        let gates = output.json["pipeline"]["gates"].as_array().unwrap();
        assert!(gates.iter().any(|gate| gate["gate"] == "navigation"));
        assert!(gates.iter().any(|gate| gate["gate"] == "budget"));
        assert!(gates.iter().any(|gate| gate["gate"] == "design"));

        let escape = parsed_command_from_params(
            "world.script",
            serde_json::json!({"script": "eval(\"1 + 1\");"}),
        );
        let escaped = execute_world_script(&escape, &mut ctx);
        assert!(matches!(escaped.level, CommandLevel::Error));
        assert!(
            escaped.lines.join(" ").to_lowercase().contains("script"),
            "{:?}",
            escaped.lines
        );

        let over_budget = parsed_command_from_params(
            "world.script",
            serde_json::json!({
                "script": "world::assert_max_entities(1); world::grid(\"crate\", 3, 1, 2.0, 2.0);"
            }),
        );
        let over_budget = execute_world_script(&over_budget, &mut ctx);
        assert!(matches!(over_budget.level, CommandLevel::Error));
        assert!(
            over_budget.lines.join(" ").contains("max_entities"),
            "{:?}",
            over_budget.lines
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recipes_capture_list_apply_and_delete_compiled_plans() {
        let root = temp_project("world-recipe");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "group", "name": "RecipeRoot", "key": "recipe.root"},
                            {"op": "place", "archetype": "crate", "parent": "RecipeRoot", "position": [1, 0, 1], "key": "recipe.crate"}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(
            !matches!(plan.level, CommandLevel::Error),
            "{:?}",
            plan.lines
        );
        let plan_id = plan.json["plan_id"].as_str().unwrap().to_string();

        let save = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({
                    "action": "save",
                    "name": "demo_recipe",
                    "plan_id": plan_id,
                    "description": "A captured crate."
                }),
            ),
            &mut ctx,
        );
        assert!(save.changed, "{:?}", save.lines);
        assert!(root
            .join(".aura_rafi/world/recipes/demo_recipe.json")
            .exists());

        let list = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({"action": "list"}),
            ),
            &mut ctx,
        );
        assert_eq!(list.json["count"], 1);
        assert_eq!(list.json["recipes"][0]["name"], "demo_recipe");

        let apply = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({"action": "apply", "name": "demo_recipe"}),
            ),
            &mut ctx,
        );
        assert!(
            !matches!(apply.level, CommandLevel::Error),
            "{:?}",
            apply.lines
        );
        assert_ne!(apply.json["plan_id"], plan_id);
        assert_eq!(apply.json["recipe"]["name"], "demo_recipe");
        assert_eq!(apply.json["entities"], 1);

        let delete = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({"action": "delete", "name": "demo_recipe"}),
            ),
            &mut ctx,
        );
        assert!(delete.changed);
        assert!(!root
            .join(".aura_rafi/world/recipes/demo_recipe.json")
            .exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pipeline_reports_budget_failures_before_commit() {
        let root = temp_project("world-pipeline");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "budget": {"entities": 1, "ops": 8},
                        "operations": [
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]},
                            {"op": "place", "archetype": "crate", "position": [2, 0, 0]}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(
            !matches!(output.level, CommandLevel::Error),
            "{:?}",
            output.lines
        );
        assert_eq!(output.json["pipeline"]["status"], "failed");
        let gates = output.json["pipeline"]["gates"].as_array().unwrap();
        let budget = gates
            .iter()
            .find(|gate| gate["gate"] == "budget")
            .unwrap();
        assert_eq!(budget["status"], "failed");
        assert_eq!(budget["detail"]["entities"], 2);
        assert_eq!(budget["detail"]["max_entities"], 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn authoring_script_can_emit_camera_and_script_operations() {
        let root = temp_project("world-script-camera");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_world_script(
            &parsed_command_from_params(
                "world.script",
                serde_json::json!({
                    "script": "world::group(\"Rig\"); world::camera(\"Cam\", 0.0, 2.0, 0.0, 55.0); world::script(\"Cam\", \"scripts/probe.rhai\");"
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(output.level, CommandLevel::Error), "{:?}", output.lines);
        let plan_id = output.json["plan_id"].as_str().unwrap();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        assert!(plan["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entity| entity["camera"].is_object()));
        assert_eq!(plan["updates"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plan_supports_mirror_variant_update_and_delete_atomically() {
        let root = temp_project("world-ops");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let keep = scene.add_root("Keep");
        scene.get_mut(keep).unwrap().stable_key = Some("keep.crate".to_string());
        let remove = scene.add_root("DeleteMe");
        scene.get_mut(remove).unwrap().stable_key = Some("delete.me".to_string());
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let planned = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "group", "name": "World", "key": "world"},
                            {"op": "place", "archetype": "crate", "parent": "World", "name": "Base", "key": "world.base", "position": [2, 0, 0]},
                            {"op": "mirror", "source": "Base", "axis": "x", "offset": 0},
                            {"op": "variant", "archetypes": ["crate", "bench"], "count": 3, "parent": "World", "name": "Mix", "origin": [0, 0, 4], "spacing": [2, 0, 0]},
                            {"op": "update", "key": "keep.crate", "transform": {"position": [5, 0, 5]}},
                            {"op": "delete", "key": "delete.me"}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(planned.level, CommandLevel::Error), "{:?}", planned.lines);
        assert_eq!(planned.json["updates"], 1);
        assert_eq!(planned.json["deletions"], 1);
        let plan_id = planned.json["plan_id"].as_str().unwrap().to_string();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let persisted: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        assert_eq!(persisted["updates"].as_array().unwrap().len(), 1);
        assert_eq!(persisted["deletions"].as_array().unwrap().len(), 1);
        let committed = execute_commit(
            &parsed_command_from_params(
                "game.commit",
                serde_json::json!({"plan_id": plan_id}),
            ),
            &mut ctx,
        );
        assert!(!matches!(committed.level, CommandLevel::Error), "{:?}", committed.lines);
        assert_eq!(committed.json["deleted"], 1);
        assert_eq!(committed.json["updated"], 1);
        let keep_position = ctx.scene.get(keep).unwrap().position;
        assert!((keep_position - glam::Vec3::new(5.0, 0.0, 5.0)).length() < 0.001);
        assert!(!ctx.scene.is_valid_node(remove));
        let names = ctx
            .scene
            .iter()
            .map(|(_, node)| node.name.clone())
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| name.ends_with("_mirror")));
        assert!(names.iter().any(|name| name.starts_with("Mix_0")));
        assert!(names.iter().any(|name| name.starts_with("Mix_1")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn authoring_script_can_query_existing_scene_tags() {
        let root = temp_project("world-script-query");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let lamp = scene.add_root("ExistingLamp");
        scene
            .get_mut(lamp)
            .unwrap()
            .tags
            .push("street.light".to_string());
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_world_script(
            &parsed_command_from_params(
                "world.script",
                serde_json::json!({
                    "script": "world::group(\"Start\"); if world::count(\"street.light\") > 0 { world::group(\"Found\"); } world::place(\"crate\", 0.0, 0.0, 0.0);"
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(output.level, CommandLevel::Error), "{:?}", output.lines);
        let plan_id = output.json["plan_id"].as_str().unwrap();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        assert!(plan["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["name"] == "Found"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn asserted_overlap_constraint_fails_the_pipeline_gate() {
        let root = temp_project("world-assert");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]},
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]},
                            {"op": "assert", "no_overlaps": true}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(output.level, CommandLevel::Error), "{:?}", output.lines);
        let gates = output.json["pipeline"]["gates"].as_array().unwrap();
        let assertions = gates
            .iter()
            .find(|gate| gate["gate"] == "assertions")
            .unwrap();
        assert_eq!(assertions["status"], "failed");
        assert_eq!(output.json["pipeline"]["status"], "failed");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cancelled_plan_cannot_be_committed() {
        let root = temp_project("world-cancel");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(plan.level, CommandLevel::Error), "{:?}", plan.lines);
        let plan_id = plan.json["plan_id"].as_str().unwrap().to_string();
        let cancel = execute_cancel(&parsed_command_from_params(
            "game.cancel",
            serde_json::json!({"plan_id": plan_id.clone()}),
        ));
        assert!(!matches!(cancel.level, CommandLevel::Error), "{:?}", cancel.lines);
        assert_eq!(cancel.json["cancelled"], true);
        let commit = execute_commit(
            &parsed_command_from_params(
                "game.commit",
                serde_json::json!({"plan_id": plan_id}),
            ),
            &mut ctx,
        );
        assert!(matches!(commit.level, CommandLevel::Error));
        assert!(
            commit.lines.join(" ").contains("cancelled"),
            "{:?}",
            commit.lines
        );
        assert!(ctx.scene.all_live_ids().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn world_program_compiles_kits_without_mutation_and_commit_applies_once() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = parsed_command(
            "game.plan",
            serde_json::json!({
                "mode": "build",
                "operations": [
                    {"op": "group", "name": "District", "key": "world.district"},
                    {"op": "grid", "archetype": "street_lamp", "parent": "District",
                     "count": [2, 1], "spacing": [6.0, 0.0], "key": "lamp.{i}.{j}"},
                    {"op": "along", "archetype": "crate", "parent": "District",
                     "points": [[0, 0, 0], [8, 0, 0]], "spacing": 4.0, "key": "crate.{i}"},
                    {"op": "place", "archetype": "checkpoint", "parent": "District",
                     "name": "Checkpoint", "position": [0, 0, 3], "key": "checkpoint.main"}
                ]
            }),
        );
        let planned = execute("game.plan", &plan, &mut ctx);
        assert_ne!(planned.level, CommandLevel::Error, "{:?}", planned.lines);
        let plan_id = planned
            .json
            .get("plan_id")
            .and_then(Value::as_str)
            .expect("plan id")
            .to_string();
        assert!(planned
            .json
            .get("plan_hash")
            .and_then(Value::as_str)
            .is_some());
        assert!(
            ctx.scene.all_live_ids().is_empty(),
            "plan must not mutate the scene"
        );
        assert!(planned.json.get("preview").is_some());

        let commit = parsed_command("game.commit", serde_json::json!({"plan_id": plan_id}));
        let committed = execute("game.commit", &commit, &mut ctx);
        assert!(committed.changed, "{:?}", committed.lines);
        assert!(ctx.scene.all_live_ids().len() > 6);
        let names = ctx
            .scene
            .iter()
            .map(|(_, node)| node.name.clone())
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| name.contains("lamp")));
        assert!(names.iter().any(|name| name.contains("crate")));
        assert!(names.iter().any(|name| name.contains("Checkpoint")));

        let replay = execute("game.commit", &commit, &mut ctx);
        assert_eq!(
            replay.json.get("replayed").and_then(Value::as_bool),
            Some(true),
            "{:?}",
            replay.lines
        );

        let live_after_build = ctx.scene.all_live_ids().len();
        let patch = parsed_command(
            "game.plan",
            serde_json::json!({
                "mode": "reconcile",
                "operations": [
                    {"op": "group", "name": "District", "key": "world.district"},
                    {"op": "grid", "archetype": "street_lamp", "parent": "District",
                     "count": [2, 1], "spacing": [6.0, 0.0], "key": "lamp.{i}.{j}"},
                    {"op": "along", "archetype": "crate", "parent": "District",
                     "points": [[0, 0, 0], [8, 0, 0]], "spacing": 4.0, "key": "crate.{i}"},
                    {"op": "place", "archetype": "checkpoint", "parent": "District",
                     "name": "Checkpoint", "position": [0, 0, 3], "key": "checkpoint.main"}
                ]
            }),
        );
        let patched = execute("game.plan", &patch, &mut ctx);
        assert_ne!(patched.level, CommandLevel::Error, "{:?}", patched.lines);
        let patch_id = patched.json["plan_id"].as_str().unwrap().to_string();
        let applied = execute(
            "game.commit",
            &parsed_command("game.commit", serde_json::json!({"plan_id": patch_id})),
            &mut ctx,
        );
        assert!(applied.changed, "{:?}", applied.lines);
        assert_eq!(
            ctx.scene.all_live_ids().len(),
            live_after_build,
            "reconcile must not duplicate keyed entities"
        );
    }

    #[test]
    fn simple_supermarket_prompt_can_compile_and_commit_a_valid_world_plan() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = parsed_command(
            "game.plan",
            serde_json::json!({
                "mode": "build",
                "design_profile": "supermarket",
                "operations": [
                    {"op": "group", "name": "Store", "key": "store.main"},
                    {"op": "place", "archetype": "floor", "parent": "Store", "name": "Floor", "key": "store.floor"},
                    {"op": "grid", "archetype": "wall", "parent": "Store", "count": [2, 1], "spacing": [8.0, 0.0], "key": "store.wall.{i}"},
                    {"op": "place", "archetype": "door", "parent": "Store", "name": "Entrance", "position": [0.0, 0.0, 4.8], "key": "store.entrance"},
                    {"op": "place", "archetype": "aisle", "parent": "Store", "name": "Main_Aisle", "key": "store.aisle.main"},
                    {"op": "grid", "archetype": "shelf", "parent": "Store", "count": [2, 1], "spacing": [4.0, 0.0], "origin": [-2.0, 0.0, 0.0], "key": "store.shelf.{i}"},
                    {"op": "place", "archetype": "checkout", "parent": "Store", "name": "Checkout", "position": [3.0, 0.0, 3.0], "key": "store.checkout"}
                ]
            }),
        );
        let planned = execute("game.plan", &plan, &mut ctx);
        assert_ne!(planned.level, CommandLevel::Error, "{:?}", planned.lines);
        let audit = &planned.json["design_audit"];
        assert_eq!(audit["status"], "passed", "{audit}");
        let plan_id = planned.json["plan_id"].as_str().unwrap().to_string();
        assert!(ctx.scene.all_live_ids().is_empty());

        let committed = execute(
            "game.commit",
            &parsed_command("game.commit", serde_json::json!({"plan_id": plan_id})),
            &mut ctx,
        );
        assert!(committed.changed, "{:?}", committed.lines);
        let roles = ctx
            .scene
            .iter()
            .filter_map(|(_, node)| node.semantic_role.clone())
            .collect::<Vec<_>>();
        assert!(roles.iter().any(|role| role.starts_with("kit.shelf")));
        assert!(roles.iter().any(|role| role.starts_with("kit.checkout")));
        assert!(roles.iter().any(|role| role.starts_with("kit.wall")));
        assert!(ctx.scene.all_live_ids().len() > 12);
    }

    #[test]
    fn world_program_commit_exceeds_the_legacy_128_entity_build_limit() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let planned = execute(
            "game.plan",
            &parsed_command(
                "game.plan",
                serde_json::json!({
                    "mode": "build",
                    "operations": [
                        {"op": "group", "name": "Field", "key": "field"},
                        {"op": "grid", "archetype": "crate", "parent": "Field",
                         "count": [200], "spacing": [1.2], "key": "field.crate.{i}"}
                    ]
                }),
            ),
            &mut ctx,
        );
        assert_ne!(planned.level, CommandLevel::Error, "{:?}", planned.lines);
        assert!(
            ctx.scene.all_live_ids().is_empty(),
            "plan must not mutate the scene"
        );
        assert!(
            planned.json["preview"]["chunks"].as_u64().unwrap_or(0) > 1,
            "the preview must report multiple chunks: {:?}",
            planned.json["preview"]
        );
        let plan_id = planned.json["plan_id"].as_str().unwrap().to_string();
        let committed = execute(
            "game.commit",
            &parsed_command("game.commit", serde_json::json!({"plan_id": plan_id})),
            &mut ctx,
        );
        assert!(committed.changed, "{:?}", committed.lines);
        assert_eq!(committed.json["ok"], true, "{:?}", committed.json);
        assert!(
            committed.json["chunks"].as_u64().unwrap_or(0) > 1,
            "the plan must be applied in more than one chunk: {:?}",
            committed.json
        );
        assert_eq!(committed.json["undo_scope"], "plan");
        assert_eq!(
            committed.json["groups"].as_u64().unwrap_or(0),
            planned.json["groups"].as_u64().unwrap_or(0)
        );
        assert_eq!(
            committed.json["entities"].as_u64().unwrap_or(0),
            planned.json["entities"].as_u64().unwrap_or(0)
        );
        let live = ctx.scene.all_live_ids().len();
        assert!(
            live > 128,
            "expected more than 128 live entities, got {live}"
        );
        assert_eq!(
            live as u64,
            planned.json["groups"].as_u64().unwrap_or(0)
                + planned.json["entities"].as_u64().unwrap_or(0)
        );
    }

    #[test]
    fn chunked_plan_application_rolls_back_every_chunk_on_failure() {
        let mut scene = SceneGraph::new();
        scene.add_root("ExistingMarker");
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let before = scene.all_live_ids();
        let mut entities = (0..129)
            .map(|index| {
                serde_json::json!({
                    "kind": "cube",
                    "name": format!("ChunkCrate_{index}"),
                    "parent": if index == 128 { "MissingParent" } else { "Field" }
                })
            })
            .collect::<Vec<_>>();
        entities.truncate(129);
        let plan = PlannedWorld {
            id: "plan_rollback_test".to_string(),
            hash: "test".to_string(),
            status: "planned".to_string(),
            mode: "build".to_string(),
            parent: None,
            design_profile: None,
            groups: vec![serde_json::json!({"name":"Field","stable_key":"field"})],
            entities,
            updates: Vec::new(),
            deletions: Vec::new(),
            fingerprint: std::collections::BTreeMap::new(),
            persisted_path: None,
            project_root: None,
        };
        let error = apply_plan_to_scene(&plan, &mut scene, &mut selection, &mut viewport)
            .expect_err("the invalid parent must fail the plan");
        assert!(
            error.contains("MissingParent") || error.contains("Parent"),
            "{error}"
        );
        assert_eq!(
            scene.all_live_ids(),
            before,
            "all chunks must be rolled back"
        );
        assert!(scene
            .iter()
            .all(|(_, node)| !node.name.starts_with("ChunkCrate_")));
    }

    #[test]
    fn world_plan_persists_a_project_scoped_plan_and_journal() {
        let root =
            std::env::temp_dir().join(format!("raf-world-plan-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let planned = execute(
            "game.plan",
            &parsed_command(
                "game.plan",
                serde_json::json!({
                    "operations": [
                        {"op": "group", "name": "World", "key": "world"},
                        {"op": "place", "archetype": "crate", "parent": "World", "name": "Crate", "key": "world.crate"}
                    ]
                }),
            ),
            &mut ctx,
        );
        assert_ne!(planned.level, CommandLevel::Error, "{:?}", planned.lines);
        let plan_id = planned.json["plan_id"].as_str().unwrap();
        let plan_file = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let journal = root.join(".aura_rafi/world/journal.jsonl");
        assert!(plan_file.exists());
        assert!(journal.exists());
        let persisted: Value = serde_json::from_slice(&std::fs::read(&plan_file).unwrap()).unwrap();
        assert_eq!(persisted["id"], plan_id);
        assert!(persisted["project_root"].as_str().is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn world_brief_round_trips_inside_the_project_root() {
        let root =
            std::env::temp_dir().join(format!("raf-world-brief-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let _guard = bind_project_root(Some(root.clone()));
        let write = parsed_command(
            "world.brief",
            serde_json::json!({
                "action": "write",
                "brief": {
                    "genre": "retail",
                    "mood": "night",
                    "metrics": {"aisle_width": 2.4}
                }
            }),
        );
        let output = execute_world_brief(&write);
        assert!(output.changed, "{:?}", output.lines);
        let path = root.join(".aura_rafi/world/brief.json");
        assert!(path.exists());
        let read = execute_world_brief(&parsed_command(
            "world.brief",
            serde_json::json!({"action": "read"}),
        ));
        assert_eq!(read.json["found"], true);
        assert_eq!(read.json["brief"]["mood"], "night");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn world_program_rejects_unknown_kits_with_actionable_error() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = parsed_command(
            "game.plan",
            serde_json::json!({
                "operations": [
                    {"op": "place", "archetype": "not_a_kit", "name": "Nope"}
                ]
            }),
        );
        let output = execute("game.plan", &plan, &mut ctx);
        assert_eq!(output.level, CommandLevel::Error);
        assert!(output.lines.join(" ").contains("Unknown archetype"));
        assert!(ctx.scene.all_live_ids().is_empty());
    }
