//! Unit tests for the `raf` CLI.

use crate::cli::args::{build_command_request, parse_options, parse_params};
use crate::cli::commands::engine::HeadlessEngine;
use crate::cli::mcp::{
    handle_mcp_message, mcp_image_content, mcp_tool_command, mcp_tools, queue_mcp_notification,
    read_plan_resource, strip_mcp_controls, take_mcp_notifications,
};
use chrono::Utc;
use raf_core::project::{Project, ProjectType};
use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest};
use serde_json::json;
use std::fs;

#[test]
fn parser_keeps_json_and_boolean_options() {
    let options = parse_options(&[
        "command".to_string(),
        "project.info".to_string(),
        "--params".to_string(),
        "{}".to_string(),
        "--dry-run".to_string(),
    ]);
    assert_eq!(options.positionals, vec!["command", "project.info"]);
    assert_eq!(options.value("params"), Some("{}"));
    assert!(options.has("dry-run"));
}

#[test]
fn params_can_be_read_from_at_file_and_params_file() {
    let path = std::env::temp_dir().join(format!(
        "raf-cli-params-{}.json",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::write(&path, r#"{"name":"Demo","confirm":true}"#).unwrap();
    let at_file = parse_options(&[
        "command".to_string(),
        "project.info".to_string(),
        "--params".to_string(),
        format!("@{}", path.display()),
    ]);
    let params = parse_params(&at_file).unwrap();
    assert_eq!(params["name"], "Demo");
    assert_eq!(params["confirm"], true);
    let explicit = parse_options(&[
        "command".to_string(),
        "project.info".to_string(),
        "--params-file".to_string(),
        path.display().to_string(),
    ]);
    let params = parse_params(&explicit).unwrap();
    assert_eq!(params["name"], "Demo");
    let both = parse_options(&[
        "command".to_string(),
        "project.info".to_string(),
        "--params".to_string(),
        "{}".to_string(),
        "--params-file".to_string(),
        path.display().to_string(),
    ]);
    assert!(parse_params(&both).is_err());
    let _ = fs::remove_file(path);
}

#[test]
fn attached_command_builder_targets_the_command_after_attach_mode() {
    let options = parse_options(&[
        "attach".to_string(),
        "command".to_string(),
        "game.add".to_string(),
        "--confirm".to_string(),
    ]);
    let request = build_command_request(&options).unwrap();
    assert_eq!(request.name, "game.add");
    assert!(request.confirm);
    assert_eq!(request.source, CommandSource::Cli);
}

#[test]
fn headless_project_lifecycle_is_ui_independent() {
    let root = std::env::temp_dir().join(format!(
        "raf-cli-test-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir_all(&root).unwrap();
    let mut engine = HeadlessEngine::new(None).unwrap();
    let mut create = EngineCommandRequest::new(
        "project.create",
        json!({"name":"Demo","parent":root,"type":"game"}),
        CommandSource::Cli,
    );
    create.confirm = true;
    let response = engine.execute(create);
    assert!(response.ok, "{}", response.lines.join(" "));
    assert!(engine.project.is_some());
    let brief = engine.resource("raf://brief").unwrap();
    assert_eq!(brief["found"], false);
    let plans = engine.resource("raf://plans").unwrap();
    assert_eq!(plans["count"], 0);
    let info = engine.execute(EngineCommandRequest::new(
        "project.info",
        json!({}),
        CommandSource::Cli,
    ));
    assert!(info.ok);
    let sessions = engine.execute(EngineCommandRequest::new(
        "session.list",
        json!({}),
        CommandSource::Cli,
    ));
    assert!(sessions.ok);
    let use_session = engine.execute(EngineCommandRequest::new(
        "session.use",
        json!({"name": "Main"}),
        CommandSource::Cli,
    ));
    assert!(use_session.ok, "{}", use_session.lines.join(" "));
    let close = engine.execute(EngineCommandRequest::new(
        "project.close",
        json!({}),
        CommandSource::Cli,
    ));
    assert!(close.ok);
    assert!(engine.project.is_none());
    let after_close = engine.execute(EngineCommandRequest::new(
        "project.info",
        json!({}),
        CommandSource::Cli,
    ));
    assert!(!after_close.ok);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn project_create_dry_run_does_not_touch_disk() {
    let root = std::env::temp_dir().join(format!(
        "raf-cli-dry-run-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir_all(&root).unwrap();
    let mut engine = HeadlessEngine::new(None).unwrap();
    let mut request = EngineCommandRequest::new(
        "project.create",
        json!({"name":"PreviewOnly","parent":root,"type":"game"}),
        CommandSource::Cli,
    );
    request.dry_run = true;
    let response = engine.execute(request);
    assert!(response.ok);
    assert!(!root.join("PreviewOnly").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_modes_gate_mutations_until_active() {
    let mut engine = HeadlessEngine::new(None).unwrap();
    let set = handle_mcp_message(
        &mut engine,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "raf_set_mode",
                "arguments": {"mode": "inspect"}
            }
        }),
    )
    .unwrap();
    assert_eq!(set["result"]["structuredContent"]["mode"], "inspect");
    let blocked = handle_mcp_message(
        &mut engine,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "raf_scene_program",
                "arguments": {"operations": []}
            }
        }),
    )
    .unwrap();
    assert!(blocked["error"]["message"]
        .as_str()
        .unwrap()
        .contains("inspect"));
    let active = handle_mcp_message(
        &mut engine,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "raf_set_mode",
                "arguments": {"mode": "active"}
            }
        }),
    )
    .unwrap();
    assert_eq!(active["result"]["structuredContent"]["mode"], "active");
}

#[test]
fn mcp_plan_resource_reads_project_plan_json() {
    let root = std::env::temp_dir().join(format!(
        "raf-mcp-plan-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let directory = root.join(".aura_rafi/world/plans");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("plan_demo.json"),
        br#"{"id":"plan_demo","status":"planned","groups":[],"entities":[]}"#,
    )
    .unwrap();
    let value = read_plan_resource(&root, "plan_demo").unwrap();
    assert_eq!(value["id"], "plan_demo");
    assert_eq!(value["plan"]["status"], "planned");
    assert!(read_plan_resource(&root, "../escape").is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_notifications_are_bounded_and_drained() {
    let _ = take_mcp_notifications();
    queue_mcp_notification(json!({"jsonrpc":"2.0","method":"notifications/resourcesUpdated"}));
    let notifications = take_mcp_notifications();
    assert_eq!(notifications.len(), 1);
    assert!(take_mcp_notifications().is_empty());
}

#[test]
fn mcp_image_content_is_bounded_and_project_confined() {
    let root = std::env::temp_dir().join(format!(
        "raf-mcp-image-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let artifacts = root.join(".aura_rafi/agent_artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    fs::write(
        artifacts.join("front.png"),
        [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I',
            b'H', b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06,
            0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D',
            b'A', b'T', 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00,
            0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N',
            b'D', 0xAE, 0x42, 0x60, 0x82,
        ],
    )
    .unwrap();
    assert!(mcp_image_content(&root, ".aura_rafi/agent_artifacts/front.png").is_some());
    assert!(mcp_image_content(&root, ".aura_rafi/agent_artifacts/../../escape.png").is_none());
    assert!(mcp_image_content(&root, "project.ron").is_none());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_initialize_and_tool_catalog_are_json_rpc() {
    let mut engine = HeadlessEngine::new(None).unwrap();
    let initialize = handle_mcp_message(
        &mut engine,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .unwrap();
    assert_eq!(initialize["result"]["serverInfo"]["name"], "raf");
    let tools = handle_mcp_message(
        &mut engine,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .unwrap();
    assert!(tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "raf_status"));
    assert!(tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "raf_context"));
    assert!(tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "raf_capabilities"));
    let tools = tools["result"]["tools"].as_array().unwrap();
    for name in [
        "raf_set_mode",
        "raf_scene_query",
        "raf_scene_digest",
        "raf_scene_spatial_map",
        "raf_scene_navigation_audit",
        "raf_scene_inspect",
        "raf_asset_inspect",
        "raf_project_close",
        "raf_session_use",
        "raf_project_save",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing headless MCP tool {name}"
        );
    }
    for name in [
        "raf_scene_reconcile",
        "raf_viewport_capture",
        "raf_scene_top_down_map",
        "raf_task_list",
        "raf_script_write",
        "raf_scene_program",
        "raf_scene_commit",
        "raf_world_brief",
        "raf_scene_visual_diff",
        "raf_undo",
    ] {
        assert!(
            !tools.iter().any(|tool| tool["name"] == name),
            "headless MCP must not advertise {name}"
        );
    }
    let digest = tools
        .iter()
        .find(|tool| tool["name"] == "raf_scene_digest")
        .unwrap();
    assert_eq!(digest["annotations"]["readOnlyHint"], true);
    assert!(digest["outputSchema"]["properties"]["ok"].is_object());
    assert_eq!(
        mcp_tool_command("raf_scene_top_down_map"),
        Some("viewport.capture")
    );
    assert_eq!(mcp_tool_command("raf_scene_cancel"), Some("game.cancel"));
    assert_eq!(mcp_tool_command("raf_scene_patch"), Some("game.plan"));
    let attached = mcp_tools(true);
    assert!(attached
        .iter()
        .any(|tool| tool["name"] == "raf_scene_cancel"));
    assert!(attached
        .iter()
        .any(|tool| tool["name"] == "raf_scene_patch"));
    let top_down = attached
        .iter()
        .find(|tool| tool["name"] == "raf_scene_top_down_map")
        .unwrap();
    assert_eq!(top_down["annotations"]["readOnlyHint"], true);
    assert_eq!(
        top_down["inputSchema"]["properties"]["overlays"]["type"],
        "array"
    );
    let commit = attached
        .iter()
        .find(|tool| tool["name"] == "raf_scene_commit")
        .unwrap();
    assert_eq!(commit["annotations"]["destructiveHint"], true);
    assert_eq!(commit["annotations"]["idempotentHint"], true);
    let reconcile = attached
        .iter()
        .find(|tool| tool["name"] == "raf_scene_reconcile")
        .unwrap();
    assert_eq!(reconcile["inputSchema"]["additionalProperties"], false);
    assert_eq!(
        reconcile["inputSchema"]["properties"]["confirm"]["type"],
        "boolean"
    );
    assert_eq!(
        reconcile["inputSchema"]["properties"]["groups"]["items"]["required"],
        json!(["name", "stable_key"])
    );
    let stripped = strip_mcp_controls(&json!({
        "confirm": true,
        "dry_run": false,
        "name": "Shelf",
        "stable_key": "store.shelf"
    }));
    assert_eq!(stripped, json!({"name":"Shelf","stable_key":"store.shelf"}));
}

#[test]
fn headless_context_is_compact_and_does_not_scan_internal_metadata() {
    let root = std::env::temp_dir().join(format!(
        "raf-cli-context-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir_all(&root).unwrap();
    let project = Project::create("ContextDemo", ProjectType::Game, &root).unwrap();
    let mut engine = HeadlessEngine::new(Some(&project.path)).unwrap();
    let response = engine.execute(EngineCommandRequest::new(
        "engine.context",
        json!({}),
        CommandSource::Cli,
    ));
    assert!(response.ok, "{}", response.lines.join(" "));
    assert_eq!(response.data["project"]["name"], "ContextDemo");
    assert!(response.data["scene"]["outline"].is_array());
    assert_eq!(
        response.data["workspace"]["internal_metadata_skipped"],
        true
    );
    let _ = fs::remove_dir_all(root);
}
