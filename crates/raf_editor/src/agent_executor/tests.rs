use std::collections::HashMap;

use raf_ai::agent_runtime::{AgentToolKind, ToolExecutionMode};
use raf_ai::provider::AgentMode;
use raf_core::project::ProjectType;
use raf_core::scene::SceneGraph;
use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest, Language, TransactionLedger};

use crate::commands::catalog::CommandCatalog;
use crate::commands::game::{HeadlessGameViewportPort, SceneSelectionState};
use crate::commands::gateway::CommandGateway;

use super::evidence::{compact_command_data, verify_entity_postcondition};
use super::execution::AgentParsedExecutor;
use super::native_agent_budget;
use super::verify_postconditions;
use super::world_build::{
    normalize_scene_arguments, translate_arguments, translate_batch, translate_build,
};
use super::*;

#[test]
fn simple_noun_prompts_still_expose_the_authoring_core_and_program_tools() {
    let catalog = CommandCatalog::builtin();
    for prompt in ["un supermercado", "una estacion abandonada", "un nivel"] {
        let pack = AgentToolExecutor::build_tool_pack(
            &catalog,
            Language::English,
            Some(ProjectType::Game),
            AgentMode::Active,
            prompt,
        );
        for name in [
            "scene_build",
            "scene_reconcile",
            "scene_batch",
            "scene_digest",
            "scene_program",
            "scene_commit",
            "scene_cancel",
            "scene_patch",
            "scene_camera",
            "scene_component",
            "capabilities_call",
            "world_brief",
            "game_completion_audit",
            "viewport_capture",
            "scene_visual_review",
            "script_write",
            "script_validate",
        ] {
            assert!(
                pack.tools.iter().any(|tool| tool.function.name == name),
                "prompt '{prompt}' is missing {name}"
            );
        }
        // Perception, the authoring core, World Program tools and scripting
        // share one turn. This is the same 68-tool hard cap used by the
        // contextual pack builder.
        assert!(
            pack.tools.len() <= 76,
            "prompt '{prompt}' exceeded the pack budget with {} tools",
            pack.tools.len()
        );
    }
}

#[test]
fn program_tools_are_not_advertised_in_inspect_mode() {
    let catalog = CommandCatalog::builtin();
    let pack = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Inspect,
        "un supermercado",
    );
    for name in [
        "scene_program",
        "scene_commit",
        "scene_cancel",
        "scene_camera",
        "scene_component",
    ] {
        assert!(
            !pack.tools.iter().any(|tool| tool.function.name == name),
            "{name} must not be mutable in Inspect"
        );
    }
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_digest"));
}

#[test]
fn viewport_capture_top_down_map_returns_a_structured_artifact_with_obstacles() {
    let root = std::env::temp_dir().join(format!(
        "raf-tool-top-down-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let project = raf_core::project::Project::create(
        "MapToolDemo",
        raf_core::project::ProjectType::Game,
        &root,
    )
    .unwrap();
    let mut scene = SceneGraph::new();
    let floor = scene.add_root("Floor");
    if let Some(node) = scene.get_mut(floor) {
        node.primitive = raf_core::scene::Primitive::Plane;
        node.scale = glam::Vec3::new(10.0, 1.0, 10.0);
    }
    let wall = scene.add_root("Wall");
    if let Some(node) = scene.get_mut(wall) {
        node.primitive = raf_core::scene::Primitive::Cube;
        node.position = glam::Vec3::new(0.0, 1.0, 0.0);
        node.scale = glam::Vec3::new(6.0, 2.0, 0.3);
        node.collider.collider_type = raf_core::scene::ColliderType::Aabb;
    }
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut asset_tools = crate::agent_asset_tools::AgentAssetTools::default();
    let studio = raf_core::ai::ExternalAssetStudio::default();
    let mut ledger = TransactionLedger::default();
    let mut editor_actions = Vec::new();
    let mut canvas_changed = false;
    let mut routes = HashMap::new();
    routes.insert(
        "viewport_capture".to_string(),
        AgentToolRoute {
            command_name: None,
            kind: AgentToolKind::Read,
        },
    );
    let catalog = CommandCatalog::builtin();
    let agent_project = AgentProjectContext {
        root: project.path.clone(),
        project_type: raf_core::project::ProjectType::Game,
    };
    let mut executor = AgentToolExecutor {
        asset_tools: &mut asset_tools,
        asset_studio: &studio,
        image_provider: None,
        image_model: String::new(),
        vision_reviewer: None,
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
        graphics: None,
        electronics: None,
        project: Some(agent_project),
        project_info: Some(&project),
        project_assets: &[],
        active_session: "test",
        catalog_pending: false,
        catalog_error: None,
        catalog: &catalog,
        language: Language::English,
        routes,
        ledger: &mut ledger,
        editor_actions: &mut editor_actions,
        canvas_changed: &mut canvas_changed,
    };
    let result = executor
        .execute(
            "map-call",
            "viewport_capture",
            serde_json::json!({"preset":"top_down_map","width":256,"height":256}),
            ToolExecutionMode::Inspect,
        )
        .expect("map capture");
    assert!(result.ok, "{:?}", result.details);
    assert_eq!(result.data["mode"], "top_down_map");
    assert!(result.data["colliders"].as_array().unwrap().len() >= 1);
    assert!(result.data["labels"].as_array().unwrap().len() >= 1);
    let uri = result.data["artifact"]["uri"].as_str().expect("artifact");
    assert!(project.path.join(uri).exists());
    assert!(!result.artifacts.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn scene_visual_review_without_a_vision_model_returns_a_structured_fallback() {
    let catalog = CommandCatalog::builtin();
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut asset_tools = crate::agent_asset_tools::AgentAssetTools::default();
    let studio = raf_core::ai::ExternalAssetStudio::default();
    let mut ledger = TransactionLedger::default();
    let mut editor_actions = Vec::new();
    let mut canvas_changed = false;
    let mut routes = HashMap::new();
    routes.insert(
        "scene_visual_review".to_string(),
        AgentToolRoute {
            command_name: None,
            kind: AgentToolKind::Read,
        },
    );
    let mut executor = AgentToolExecutor {
        asset_tools: &mut asset_tools,
        asset_studio: &studio,
        image_provider: None,
        image_model: String::new(),
        vision_reviewer: None,
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
        graphics: None,
        electronics: None,
        project: None,
        project_info: None,
        project_assets: &[],
        active_session: "",
        catalog_pending: false,
        catalog_error: None,
        catalog: &catalog,
        language: Language::English,
        routes,
        ledger: &mut ledger,
        editor_actions: &mut editor_actions,
        canvas_changed: &mut canvas_changed,
    };
    let result = executor
        .execute(
            "vision-call",
            "scene_visual_review",
            serde_json::json!({}),
            ToolExecutionMode::Inspect,
        )
        .expect("the fallback must be a tool result, not an adapter error");
    assert!(result.ok);
    assert!(result.partial);
    assert_eq!(result.data["status"], "unavailable");
    assert_eq!(result.data["fallback"], "vision_unavailable");
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning.contains("vision-capable")));
    assert_eq!(
        result
            .verification
            .as_ref()
            .and_then(|value| value.get("status")),
        Some(&serde_json::json!("not_checked"))
    );
}

#[test]
fn image_intent_exposes_real_generation_choices_without_separate_model_roles() {
    let catalog = CommandCatalog::builtin();
    let active = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Active,
        "Genera una imagen en el estudio externo",
    );
    for name in [
        "asset_generate_local_png",
        "asset_generate_image",
        "external_studio_install",
        "external_studio_generate",
    ] {
        assert!(
            active.tools.iter().any(|tool| tool.function.name == name),
            "missing {name}"
        );
    }
    assert!(active.tools.len() <= 78);
    let inspect = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Inspect,
        "Genera una imagen en el estudio externo",
    );
    assert!(!inspect
        .tools
        .iter()
        .any(|tool| tool.function.name.starts_with("asset_generate")
            || tool.function.name.starts_with("external_studio")));
}

#[test]
fn game_tool_pack_is_contextual_and_never_exposes_electronics() {
    let catalog = CommandCatalog::builtin();
    let pack = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Active,
        "Create a detailed store shelf",
    );

    assert!(pack.tools.len() <= 76);
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_batch"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_build"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_create_group"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_reparent"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_repair"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_inspect"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "project_health"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "viewport_capture"));
    assert!(!pack
        .tools
        .iter()
        .any(|tool| tool.function.name.starts_with("electronics_")));
}

#[test]
fn gameplay_prompt_exposes_lighting_scripting_and_visual_completion_tools() {
    let catalog = CommandCatalog::builtin();
    let pack = AgentToolExecutor::build_tool_pack(
            &catalog,
            Language::English,
            Some(ProjectType::Game),
            AgentMode::Active,
            "Crea un mapa enorme de un desierto con un muro que se debe saltar y si se salta te da puntos",
        );
    for name in [
        "script_write",
        "script_attach",
        "script_validate",
        "lighting_inspect",
        "lighting_set",
        "viewport_capture",
        "game_completion_audit",
    ] {
        assert!(
            pack.tools.iter().any(|tool| tool.function.name == name),
            "missing {name}: {:?}",
            pack.tools
                .iter()
                .map(|tool| tool.function.name.as_str())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn continuation_prompt_keeps_game_mutation_tools_available() {
    let catalog = CommandCatalog::builtin();
    let pack = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Active,
        "Continúa y termina la tienda con la estructura modular pendiente",
    );

    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_build"));
    assert!(pack
        .tools
        .iter()
        .any(|tool| tool.function.name == "scene_create"));
    assert!(pack.routes.contains_key("scene_build"));
    assert!(pack.routes.contains_key("scene_create"));
}

#[test]
fn batch_schema_exposes_nested_scene_values() {
    let catalog = CommandCatalog::builtin();
    let pack = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Active,
        "Create a detailed store shelf",
    );
    let batch = pack
        .tools
        .iter()
        .find(|tool| tool.function.name == "scene_batch")
        .expect("scene_batch");
    let params =
        &batch.function.parameters["properties"]["operations"]["items"]["properties"]["params"];
    assert_eq!(params["type"], "object");
    assert_eq!(params["properties"]["transform"]["type"], "object");
    assert_eq!(
        params["properties"]["transform"]["properties"]["position"]["type"],
        "array"
    );
    assert_eq!(params["properties"]["color_rgba"]["type"], "array");
}

#[test]
fn inspect_mode_never_exposes_mutations() {
    let catalog = CommandCatalog::builtin();
    let pack = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Inspect,
        "Create a detailed store shelf",
    );

    assert!(pack
        .routes
        .values()
        .all(|route| route.kind == AgentToolKind::Read));
}

#[test]
fn semantic_scene_arguments_reach_the_game_kernel_without_lossy_flattening() {
    let mut arguments = serde_json::json!({
        "kind": "cube",
        "name": "Shelf",
        "transform": {
            "position": [1, 2, 3],
            "rotation_deg": [0, 90, 0],
            "scale": [2, 3, 4]
        },
        "color_rgba": [10, 20, 30, 255]
    });
    normalize_scene_arguments(&mut arguments).unwrap();

    assert_eq!(arguments["primitive"], "cube");
    assert_eq!(arguments["transform"]["position"][0], 1);
    assert_eq!(arguments["transform"]["rotation_deg"][1], 90);
    assert_eq!(arguments["transform"]["scale"][2], 4);
    assert_eq!(arguments["color_rgba"][2], 30);
    assert!(arguments.get("kind").is_none());
}

#[test]
fn batch_translation_accepts_semantic_and_canonical_scene_operations() {
    let mut arguments = serde_json::json!({
        "operations": [
            {
                "name": "scene_create",
                "params": {
                    "kind": "sphere",
                    "name": "Display",
                    "transform": {"position": [1, 2, 3]}
                }
            },
            {
                "name": "/game.update",
                "params": {"target": "Display", "color_rgba": [10, 20, 30, 255]}
            }
        ]
    });

    translate_batch(&mut arguments).unwrap();

    assert_eq!(arguments["operations"][0]["name"], "game.add");
    assert_eq!(arguments["operations"][0]["params"]["primitive"], "sphere");
    assert_eq!(arguments["operations"][1]["name"], "game.update");
    assert_eq!(arguments["operations"][1]["params"]["color_rgba"][2], 30);
}

#[test]
fn batch_translation_rejects_missing_or_non_object_params() {
    let mut missing = serde_json::json!({
        "operations": [{"name": "scene_create"}]
    });
    assert!(translate_batch(&mut missing).is_err());

    let mut invalid = serde_json::json!({
        "operations": [{"name": "scene_create", "params": []}]
    });
    assert!(translate_batch(&mut invalid).is_err());
}

#[test]
fn build_translation_keeps_groups_and_nested_entity_values() {
    let mut arguments = serde_json::json!({
        "groups": [{"name": "Structure"}],
        "entities": [{
            "kind": "cube",
            "name": "Floor",
            "transform": {"position": [0, 1, 2], "scale": [4, 1, 2]},
            "color_rgba": [60, 150, 90, 255]
        }]
    });

    translate_build(&mut arguments).unwrap();

    assert_eq!(arguments["groups"][0]["name"], "Structure");
    assert_eq!(arguments["entities"][0]["primitive"], "cube");
    assert_eq!(arguments["entities"][0]["transform"]["scale"][0], 4);
    assert_eq!(arguments["entities"][0]["color_rgba"][1], 150);
}

#[test]
fn postconditions_detect_a_transform_or_color_mismatch() {
    let scene = SceneGraph::new();
    let failures = verify_entity_postcondition(
        &serde_json::json!({
            "primitive": "cube",
            "name": "Shelf",
            "transform": {"position": [1, 2, 3]},
            "color_rgba": [10, 20, 30, 255]
        }),
        &serde_json::json!({
            "id": 0,
            "name": "Shelf",
            "primitive": "Cube",
            "position": [0, 0, 0],
            "rotation_deg": [0, 0, 0],
            "scale": [1, 1, 1],
            "color_rgba": [224, 116, 24, 255],
            "parent_id": null
        }),
        &scene,
    );

    assert_eq!(failures.len(), 2);
    assert!(failures.iter().any(|failure| failure.contains("position")));
    assert!(failures
        .iter()
        .any(|failure| failure.contains("color_rgba")));
}

#[test]
fn harness_parent_verification_reads_live_graph_for_semantic_inspection() {
    let mut scene = SceneGraph::new();
    let parent = scene.add_root_folder("Building");
    let child = scene.add_child(parent, "Door");
    let inspection = raf_core::agent_context::scene_inspect(
        &scene,
        &serde_json::json!({"target": child.0.to_string()}),
        &[],
    );
    let expected = serde_json::json!({"parent": "Building"});
    let entity = &inspection.data["entity"];
    assert!(!entity.is_null());
    assert!(verify_entity_postcondition(&expected, entity, &scene).is_empty());
    scene.reparent_node(child, None);
    assert!(!verify_entity_postcondition(&expected, entity, &scene).is_empty());
}

#[test]
fn harness_scripting_survives_a_mixed_world_build_request() {
    let pack = AgentToolExecutor::build_tool_pack(
        &CommandCatalog::builtin(),
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Active,
        "Create a world with scripts, lighting, image assets, prefabs and grid layout",
    );
    assert!(pack.tools.len() <= 76);
    for name in [
        "script_create",
        "script_attach",
        "script_validate",
        "assembly_revert",
        "assembly_update",
        "assembly_make_unique",
        "scene_build",
        "scene_verify",
        "capabilities_search",
    ] {
        assert!(pack.routes.contains_key(name), "missing {name}");
    }
}

#[test]
fn harness_large_batch_is_verified_before_model_compaction() {
    use crate::commands::game::HeadlessGameViewportPort;
    let mut scene = SceneGraph::new();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let mut actions = Vec::new();
    let params = serde_json::json!({"operations": (0..38).map(|i|
            serde_json::json!({"name":"game.add", "params":{
                "primitive":"sphere", "name":format!("Object_{i}"), "position":[i,0,0]
            }})).collect::<Vec<_>>()});
    let executor = AgentParsedExecutor {
        scene: &mut scene,
        selection: &mut selection,
        viewport: &mut viewport,
        electronics: None,
        project: None,
        editor_actions: &mut actions,
    };
    let mut gateway = CommandGateway::new(executor);
    let mut request = EngineCommandRequest::new("game.batch", params.clone(), CommandSource::Agent);
    request.confirm = true;
    request.budget = Some(native_agent_budget());
    let response = gateway.execute(request);
    assert!(response.ok, "{:?}", response);
    assert!(response.data.get("truncated").is_none());
    assert!(native_agent_budget().max_result_bytes.is_none());
    let data = response.data;
    drop(gateway);
    assert!(verify_postconditions("game.batch", &params, &data, &scene).is_empty());
    assert_eq!(scene.all_live_ids().len(), 38);
}

#[test]
fn mesh_edit_is_a_game_authoring_tool_routed_to_game_mesh() {
    let catalog = CommandCatalog::builtin();
    let mesh_command = catalog.find("game.mesh").expect("game.mesh catalog entry");
    assert_eq!(mesh_command.name, "game.mesh");
    assert_eq!(mesh_command.description_key, "commands.game_mesh.desc");
    assert!(mesh_command
        .aliases
        .iter()
        .any(|alias| alias == "mesh.edit"));
    for prompt in ["Create a detailed store shelf", "Crea un nivel de juego"] {
        let pack = AgentToolExecutor::build_tool_pack(
            &catalog,
            Language::English,
            Some(ProjectType::Game),
            AgentMode::Active,
            prompt,
        );
        let tool = pack
            .tools
            .iter()
            .find(|tool| tool.function.name == "mesh_edit")
            .unwrap_or_else(|| panic!("missing mesh_edit for prompt '{prompt}'"));
        assert_eq!(
            pack.routes["mesh_edit"].command_name.as_deref(),
            Some("game.mesh")
        );
        assert_eq!(pack.routes["mesh_edit"].kind, AgentToolKind::Mutation);
        assert!(tool
            .function
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("mesh_edit action=enter"));
        assert_eq!(
            tool.function.parameters["properties"]["action"]["enum"],
            serde_json::json!([
                "inspect",
                "enter",
                "set",
                "move",
                "scale",
                "extrude",
                "inset",
                "subdivide",
                "delete",
                "revert",
                "apply"
            ])
        );
        assert_eq!(
            tool.function.parameters["required"],
            serde_json::json!(["action"])
        );
    }
    let inspect = AgentToolExecutor::build_tool_pack(
        &catalog,
        Language::English,
        Some(ProjectType::Game),
        AgentMode::Inspect,
        "Create a detailed store shelf",
    );
    assert!(!inspect.routes.contains_key("mesh_edit"));

    let arguments = translate_arguments(
        "mesh_edit",
        "game.mesh",
        serde_json::json!({"target": "Cube"}),
    )
    .unwrap();
    assert_eq!(arguments["action"], "inspect");
}

#[test]
fn mesh_topology_survives_agent_result_compaction() {
    let compact = compact_command_data(serde_json::json!({
        "has_editable_mesh": true,
        "editable_mesh": {
            "revision": 2,
            "vertex_count": 3,
            "face_count": 1,
            "bounds_min": [0.0, 0.0, 0.0],
            "bounds_max": [1.0, 1.0, 0.0],
            "positions": [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            "indices": [0, 1, 2]
        }
    }));

    assert_eq!(compact["editable_mesh"]["vertex_count"], 3);
    assert_eq!(
        compact["editable_mesh"]["positions"][1],
        serde_json::json!([1.0, 0.0, 0.0])
    );
    assert_eq!(
        compact["editable_mesh"]["indices"],
        serde_json::json!([0, 1, 2])
    );

    let entity = compact_command_data(serde_json::json!({
        "ok": true,
        "entity": {
            "id": 1,
            "name": "Cube",
            "has_editable_mesh": false,
            "editable_mesh": {"revision": 1},
            "uncompacted_debug_field": "drop me"
        }
    }));
    assert!(entity["entity"].get("uncompacted_debug_field").is_none());
}

#[test]
fn batch_result_compaction_keeps_effective_scene_values_without_mesh_noise() {
    let compact = compact_command_data(serde_json::json!({
        "ok": true,
        "entity_count": 1,
        "operations": [{
            "name": "game.add",
            "ok": true,
            "changed": true,
            "summary": "Created Shelf",
            "entity": {
                "id": 4,
                "name": "Shelf",
                "primitive": "Cube",
                "position": [1, 2, 3],
                "scale": [2, 1, 1],
                "color_rgba": [20, 30, 40, 255],
                "mesh": {
                    "vertex_count": 24,
                    "index_count": 36,
                    "bounds_min": [-1, -1, -1]
                }
            }
        }]
    }));

    assert_eq!(
        compact["operations"][0]["entity"]["position"],
        serde_json::json!([1, 2, 3])
    );
    assert_eq!(
        compact["operations"][0]["entity"]["color_rgba"],
        serde_json::json!([20, 30, 40, 255])
    );
    assert_eq!(
        compact["operations"][0]["entity"]["mesh"],
        serde_json::json!({"vertex_count": 24, "index_count": 36})
    );
    assert!(compact["operations"][0]["entity"]["mesh"]["bounds_min"].is_null());
}
