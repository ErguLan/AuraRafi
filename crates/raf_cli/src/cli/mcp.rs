//! Model Context Protocol stdio server, tool catalog and resources.

mod schemas;

use crate::attached::AttachedClient;
use crate::cli::commands::engine::HeadlessEngine;
use crate::cli::output::clean_path_string;
use crate::cli::{CliResult, PROTOCOL_VERSION};
use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest, MAX_COMMAND_FRAME_BYTES};
use self::schemas::{budget_schema, mcp_semantic_tools, mcp_tool, schema};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

static MCP_MODE: AtomicU8 = AtomicU8::new(2);
static MCP_NOTIFICATIONS: Mutex<Vec<Value>> = Mutex::new(Vec::new());

pub(super) fn queue_mcp_notification(value: Value) {
    if let Ok(mut pending) = MCP_NOTIFICATIONS.lock() {
        if pending.len() < 64 {
            pending.push(value);
        }
    }
}

pub(super) fn take_mcp_notifications() -> Vec<Value> {
    MCP_NOTIFICATIONS
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default()
}

pub(super) trait McpCommandHost: CommandEndpoint {
    fn resource(&self, uri: &str) -> CliResult<Value>;
    fn project_root(&self) -> Option<&Path>;
    fn attached(&self) -> bool {
        false
    }
}

impl McpCommandHost for HeadlessEngine {
    fn resource(&self, uri: &str) -> CliResult<Value> {
        HeadlessEngine::resource(self, uri)
    }

    fn project_root(&self) -> Option<&Path> {
        self.project_path()
    }
}

impl McpCommandHost for AttachedClient {
    fn resource(&self, uri: &str) -> CliResult<Value> {
        AttachedClient::resource(self, uri)
    }

    fn project_root(&self) -> Option<&Path> {
        Some(self.project_path())
    }

    fn attached(&self) -> bool {
        true
    }
}

fn mcp_mode() -> &'static str {
    match MCP_MODE.load(Ordering::Relaxed) {
        0 => "inspect",
        1 => "plan",
        _ => "active",
    }
}

fn set_mcp_mode(value: &str) -> Result<&'static str, String> {
    let mode = match value.trim().to_ascii_lowercase().as_str() {
        "inspect" | "read" | "readonly" => 0,
        "plan" | "preview" => 1,
        "active" | "apply" => 2,
        other => {
            return Err(format!(
                "Unknown MCP mode '{other}'. Use inspect, plan or active."
            ))
        }
    };
    MCP_MODE.store(mode, Ordering::Relaxed);
    Ok(mcp_mode())
}

fn mcp_command_is_mutation(name: &str) -> bool {
    matches!(
        name,
        "game.add"
            | "game.update"
            | "game.delete"
            | "game.duplicate"
            | "game.reparent"
            | "game.snap"
            | "game.transform"
            | "game.move"
            | "game.rotate"
            | "game.scale"
            | "game.color"
            | "game.batch"
            | "game.build"
            | "game.reconcile"
            | "game.repair"
            | "game.plan"
            | "game.commit"
            | "game.cancel"
            | "game.create_group"
            | "game.camera"
            | "game.generate_prefab"
            | "world.recipe"
            | "world.script"
            | "world.brief"
            | "scene.visual_diff"
            | "script.create"
            | "script.write"
            | "script.attach"
            | "script.detach"
            | "script.run"
            | "behavior.attach"
            | "behavior.detach"
            | "behavior.parameter"
            | "prefab.capture"
            | "prefab.instantiate"
            | "lighting.set"
            | "lighting.reset"
            | "project.create"
            | "project.close"
            | "project.save"
            | "undo"
            | "redo"
            | "__generic__"
    )
}

fn mcp_plan_mode_allowed(name: &str) -> bool {
    matches!(name, "game.plan" | "world.script" | "world.brief")
}

pub(crate) fn read_plan_resource(root: &Path, id: &str) -> CliResult<Value> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("Plan resource id contains unsupported characters.".to_string());
    }
    let path = root
        .join(".aura_rafi/world/plans")
        .join(format!("{id}.json"));
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("Plan '{id}' was not found: {error}"))?;
    let plan = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("Plan '{id}' is not valid JSON: {error}"))?;
    Ok(json!({
        "id": id,
        "path": format!(".aura_rafi/world/plans/{id}.json"),
        "plan": plan,
    }))
}

pub(super) fn run_mcp_stdio<H: McpCommandHost>(mut engine: H) -> CliResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    for line in reader.lines() {
        let line = line.map_err(|error| format!("MCP read: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_COMMAND_FRAME_BYTES {
            let response = jsonrpc_error(
                &Value::Null,
                -32600,
                "MCP frame exceeds the 1 MiB safety limit.",
            );
            write_mcp_response(&mut writer, &response)?;
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                let response = jsonrpc_error(&Value::Null, -32700, format!("MCP JSON: {error}"));
                write_mcp_response(&mut writer, &response)?;
                continue;
            }
        };
        if let Some(response) = handle_mcp_message(&mut engine, &message) {
            write_mcp_response(&mut writer, &response)?;
        }
        for notification in take_mcp_notifications() {
            write_mcp_response(&mut writer, &notification)?;
        }
    }
    Ok(())
}

fn write_mcp_response(writer: &mut impl Write, response: &Value) -> CliResult<()> {
    let encoded = serde_json::to_string(response).map_err(|error| error.to_string())?;
    writer
        .write_all(encoded.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("MCP write: {error}"))
}

fn jsonrpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn jsonrpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

pub(super) fn handle_mcp_message<H: McpCommandHost>(engine: &mut H, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message.get("method")?.as_str()?;
    if !message.get("id").is_some() {
        return None;
    }
    let instructions = if engine.attached() {
        "AuraRafi attached editor surface. Inspect, preview, confirm and verify. Mutations use the live document and its real undo history. Never expose Play or Runtime."
    } else {
        "AuraRafi headless command surface. Only headless-supported inspection and project lifecycle tools are advertised. Mutations require an attached editor and never expose Play or Runtime."
    };
    match method {
        "initialize" => Some(jsonrpc_result(
            &id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {"listChanged": false},
                    "resources": {"subscribe": true, "listChanged": true},
                    "prompts": {"listChanged": false},
                },
                "serverInfo": {"name": "raf", "version": env!("CARGO_PKG_VERSION")},
                "instructions": instructions,
            }),
        )),
        "ping" => Some(jsonrpc_result(&id, json!({}))),
        "tools/list" => Some(jsonrpc_result(
            &id,
            json!({"tools": mcp_tools(engine.attached())}),
        )),
        "resources/list" => Some(jsonrpc_result(
            &id,
            json!({"resources": [
                {"uri": "raf://status", "name": "Rafi status", "mimeType": "application/json"},
                {"uri": "raf://project", "name": "Open project", "mimeType": "application/json"},
                {"uri": "raf://capabilities", "name": "Engine capabilities", "mimeType": "application/json"},
                {"uri": "raf://brief", "name": "World Brief", "mimeType": "application/json"},
                {"uri": "raf://plans", "name": "World Plan index", "mimeType": "application/json"},
                {"uri": "raf://digest", "name": "Scene digest", "mimeType": "application/json"},
                {"uri": "raf://context", "name": "Compact project context", "mimeType": "application/json"},
                {"uri": "raf://workspace", "name": "Workspace summary", "mimeType": "application/json"},
            ]}),
        )),
        "resources/read" => {
            let uri = message
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or("");
            match engine.resource(uri) {
                Ok(value) => Some(jsonrpc_result(
                    &id,
                    json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": value.to_string()}]}),
                )),
                Err(error) => Some(jsonrpc_error(&id, -32602, error)),
            }
        }
        "prompts/list" => Some(jsonrpc_result(
            &id,
            json!({"prompts": [{"name": "raf_plan_scene_change", "description": "Create a dry-run plan for a scene change before committing."}]}),
        )),
        "prompts/get" => Some(jsonrpc_result(
            &id,
            json!({"description": "Rafi scene planning prompt", "messages": [{"role": "user", "content": {"type": "text", "text": "Plan the requested AuraRafi scene change. Inspect capabilities first, preview mutations, then verify the result. Do not use Play or Runtime."}}]}),
        )),
        "tools/call" => handle_mcp_tool_call(engine, &id, message),
        _ => Some(jsonrpc_error(
            &id,
            -32601,
            format!("Unknown MCP method: {method}"),
        )),
    }
}

fn handle_mcp_tool_call<H: McpCommandHost>(
    engine: &mut H,
    id: &Value,
    message: &Value,
) -> Option<Value> {
    let name = message
        .pointer("/params/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = message
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if name == "raf_project_path" {
        let Some(root) = engine.project_root() else {
            return Some(jsonrpc_result(
                id,
                json!({
                    "isError": true,
                    "content": [{"type": "text", "text": "No project is mounted or attached."}],
                    "structuredContent": {"ok": false, "error": "project_not_found"}
                }),
            ));
        };
        let clean = clean_path_string(root);
        let project = engine.resource("raf://project").unwrap_or_else(|_| json!({}));
        return Some(jsonrpc_result(
            id,
            json!({
                "isError": false,
                "content": [{"type": "text", "text": clean.clone()}],
                "structuredContent": {
                    "ok": true,
                    "path": clean,
                    "raw_path": root.display().to_string(),
                    "project": project,
                    "summary": clean,
                }
            }),
        ));
    }
    let confirm = arguments
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let dry_run = arguments
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let expected_revision = match arguments.get("expected_revision") {
        Some(value) => match value.as_u64() {
            Some(revision) => Some(revision),
            None => {
                return Some(jsonrpc_error(
                    id,
                    -32602,
                    "expected_revision must be an unsigned integer.",
                ))
            }
        },
        None => None,
    };
    let idempotency_key = arguments
        .get("idempotency_key")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session = arguments
        .get("session")
        .and_then(Value::as_str)
        .map(str::to_string);
    let budget = match arguments.get("budget") {
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(budget) => Some(budget),
            Err(error) => {
                return Some(jsonrpc_error(
                    id,
                    -32602,
                    format!("budget is invalid: {error}"),
                ))
            }
        },
        None => None,
    };
    if name == "raf_set_mode" {
        let Some(mode) = arguments.get("mode").and_then(Value::as_str) else {
            return Some(jsonrpc_error(id, -32602, "raf_set_mode requires mode."));
        };
        return Some(match set_mcp_mode(mode) {
            Ok(mode) => jsonrpc_result(
                id,
                json!({
                    "isError": false,
                    "content": [{"type": "text", "text": format!("MCP mode: {mode}")}],
                    "structuredContent": {"ok": true, "mode": mode}
                }),
            ),
            Err(error) => jsonrpc_error(id, -32602, error),
        });
    }
    let Some(command_name) = mcp_tool_command(name) else {
        return Some(jsonrpc_error(
            id,
            -32602,
            format!("Unknown Rafi MCP tool: {name}"),
        ));
    };
    let mut arguments = arguments;
    if name == "raf_scene_top_down_map" {
        if let Some(object) = arguments.as_object_mut() {
            object
                .entry("preset".to_string())
                .or_insert_with(|| json!("top_down_map"));
        }
    }
    if name == "raf_scene_patch" {
        if let Some(object) = arguments.as_object_mut() {
            object
                .entry("mode".to_string())
                .or_insert_with(|| json!("reconcile"));
        }
    }
    let command_name = if command_name == "__generic__" {
        let Some(generic_name) = arguments
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return Some(jsonrpc_error(id, -32602, "raf_command requires a name."));
        };
        let params = arguments
            .get("params")
            .cloned()
            .unwrap_or_else(|| json!({}));
        arguments = strip_mcp_controls(&params);
        generic_name
    } else {
        arguments = strip_mcp_controls(&arguments);
        command_name.to_string()
    };
    let mode = mcp_mode();
    if mode == "inspect" && mcp_command_is_mutation(&command_name) {
        return Some(jsonrpc_error(
            id,
            -32000,
            format!(
                "MCP mode is inspect; '{command_name}' is a mutation. Call raf_set_mode with plan or active first."
            ),
        ));
    }
    if mode == "plan"
        && mcp_command_is_mutation(&command_name)
        && !mcp_plan_mode_allowed(&command_name)
    {
        return Some(jsonrpc_error(
            id,
            -32000,
            format!(
                "MCP mode is plan; '{command_name}' applies changes. Use raf_scene_program / raf_world_script to compile a plan, then switch to active to commit."
            ),
        ));
    }
    if let Some(session_id) = session.as_deref() {
        if command_name != "session.use" && command_name != "session.activate" {
            let mut use_request = EngineCommandRequest::new(
                "session.use",
                json!({"session": session_id}),
                CommandSource::Mcp,
            );
            use_request.confirm = true;
            let use_response = engine.execute(use_request);
            if !use_response.ok {
                return Some(jsonrpc_error(
                    id,
                    -32000,
                    format!(
                        "The requested MCP session could not be activated: {}",
                        use_response.lines.join(" ")
                    ),
                ));
            }
        }
    }
    let mut request = EngineCommandRequest::new(command_name, arguments, CommandSource::Mcp);
    request.confirm = confirm;
    request.dry_run = dry_run || name == "raf_scene_plan";
    request.expected_revision = expected_revision;
    request.idempotency_key = idempotency_key;
    request.session = session;
    request.budget = budget;
    let response = engine.execute(request);
    if response.ok && response.changed {
        for uri in ["raf://status", "raf://plans", "raf://brief", "raf://digest"] {
            queue_mcp_notification(json!({
                "jsonrpc": "2.0",
                "method": "notifications/resourcesUpdated",
                "params": {"uri": uri, "revision": response.revision}
            }));
        }
    }
    let compact_lines = raf_core::agent_context::compact_result_lines(&response.lines);
    let text = compact_lines
        .first()
        .map(|line| format!("{}\n{}", response.title, line))
        .unwrap_or_else(|| response.title.clone());
    let mut structured = serde_json::to_value(&response).unwrap_or_else(|_| json!({"ok": false}));
    if let Some(object) = structured.as_object_mut() {
        object.insert(
            "data".to_string(),
            raf_core::agent_context::compact_result_data(&response.data),
        );
        object.insert("lines".to_string(), json!(compact_lines.clone()));
        object.insert(
            "summary".to_string(),
            json!(compact_lines
                .first()
                .cloned()
                .unwrap_or_else(|| response.title.clone())),
        );
    }
    let mut content = vec![json!({"type": "text", "text": text})];
    let mut image_uris = Vec::new();
    if let Some(uri) = response
        .data
        .get("artifact")
        .and_then(Value::as_object)
        .and_then(|artifact| artifact.get("uri"))
        .and_then(Value::as_str)
    {
        image_uris.push(uri.to_string());
    }
    for artifact in &response.artifacts {
        if artifact.kind.starts_with("image/") {
            image_uris.push(artifact.uri.clone());
        }
    }
    image_uris.sort();
    image_uris.dedup();
    for uri in image_uris.iter().take(4) {
        if let Some(image) = engine
            .project_root()
            .and_then(|root| mcp_image_content(root, uri))
        {
            content.push(image);
        }
    }
    Some(jsonrpc_result(
        id,
        json!({
            "isError": !response.ok,
            "content": content,
            "structuredContent": structured,
        }),
    ))
}

pub(super) fn mcp_tool_command(name: &str) -> Option<&'static str> {
    match name {
        "raf_status" => Some("engine.status"),
        "raf_context" => Some("engine.context"),
        "raf_project_snapshot" => Some("engine.context"),
        "raf_scene_outline" => Some("scene.outline"),
        "raf_scene_digest" => Some("scene.digest"),
        "raf_scene_query" => Some("scene.query"),
        "raf_scene_spatial_map" => Some("scene.spatial_map"),
        "raf_scene_overlaps" => Some("scene.overlaps"),
        "raf_scene_navigation_audit" => Some("scene.navigation_audit"),
        "raf_scene_diff" => Some("scene.diff"),
        "raf_scene_design_audit" => Some("scene.design_audit"),
        "raf_scene_inspect" => Some("scene.inspect"),
        "raf_selection_get" => Some("selection.get"),
        "raf_viewport_capture" => Some("viewport.capture"),
        "raf_scene_top_down_map" => Some("viewport.capture"),
        "raf_scene_visual_review" => Some("scene.visual_review"),
        "raf_task_list" => Some("task.list"),
        "raf_task_get" => Some("task.get"),
        "raf_task_events" => Some("task.events"),
        "raf_task_cancel" => Some("task.cancel"),
        "raf_assets_search" => Some("assets.catalog"),
        "raf_assets_recommend" => Some("assets.recommend"),
        "raf_asset_inspect" => Some("assets.inspect"),
        "raf_scripts_search" => Some("scripts.catalog"),
        "raf_script_list" => Some("script.list"),
        "raf_script_create" => Some("script.create"),
        "raf_script_write" => Some("script.write"),
        "raf_script_attach" => Some("script.attach"),
        "raf_script_validate" => Some("script.validate"),
        "raf_script_run" => Some("script.run"),
        "raf_behavior_attach" => Some("behavior.attach"),
        "raf_lighting_inspect" => Some("lighting.inspect"),
        "raf_lighting_set" => Some("lighting.set"),
        "raf_lighting_reset" => Some("lighting.reset"),
        "raf_game_completion_audit" => Some("project.gameplay_audit"),
        "raf_project_health" => Some("project.health"),
        "raf_scene_verify" => Some("scene.verify"),
        "raf_transaction_undo" => Some("transaction.undo"),
        "raf_scene_create" => Some("game.add"),
        "raf_scene_update" => Some("game.update"),
        "raf_scene_reparent" => Some("game.reparent"),
        "raf_scene_duplicate" => Some("game.duplicate"),
        "raf_scene_snap" => Some("game.snap"),
        "raf_scene_instantiate_template" => Some("game.generate_prefab"),
        "raf_scene_build" | "raf_scene_apply" | "raf_scene_plan" => Some("game.build"),
        "raf_scene_program" => Some("game.plan"),
        "raf_scene_commit" => Some("game.commit"),
        "raf_scene_patch" => Some("game.plan"),
        "raf_scene_cancel" => Some("game.cancel"),
        "raf_world_brief" => Some("world.brief"),
        "raf_world_recipe" => Some("world.recipe"),
        "raf_world_script" => Some("world.script"),
        "raf_scene_visual_diff" => Some("scene.visual_diff"),
        "raf_scene_reconcile" => Some("game.reconcile"),
        "raf_scene_repair" => Some("game.repair"),
        "raf_scene_batch" => Some("game.batch"),
        "raf_diagnostics" => Some("project.health"),
        "raf_doctor" => Some("engine.doctor"),
        "raf_capabilities" => Some("capabilities.list"),
        "raf_capabilities_search" => Some("capabilities.search"),
        "raf_project_info" => Some("project.info"),
        "raf_project_open" => Some("project.open"),
        "raf_project_close" => Some("project.close"),
        "raf_project_create" => Some("project.create"),
        "raf_project_save" => Some("project.save"),
        "raf_session_list" => Some("session.list"),
        "raf_session_use" => Some("session.use"),
        "raf_undo" => Some("undo"),
        "raf_redo" => Some("redo"),
        "raf_workspace_describe" => Some("workspace.describe"),
        "raf_command" => Some("__generic__"),
        _ => None,
    }
}

pub(super) fn mcp_image_content(root: &Path, uri: &str) -> Option<Value> {
    const MAX_BYTES: u64 = 4 * 1024 * 1024;
    let relative = Path::new(uri);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        || relative.extension().and_then(|value| value.to_str()) != Some("png")
    {
        return None;
    }
    let project = root.canonicalize().ok()?;
    let file = project.join(relative).canonicalize().ok()?;
    if !file.starts_with(&project) {
        return None;
    }
    let metadata = file.metadata().ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_BYTES {
        return None;
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(&file)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    Some(json!({
        "type": "image",
        "data": base64_encode(&bytes),
        "mimeType": "image/png",
    }))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for part in bytes.chunks(3) {
        let bits = ((part[0] as u32) << 16)
            | ((part.get(1).copied().unwrap_or(0) as u32) << 8)
            | part.get(2).copied().unwrap_or(0) as u32;
        output.push(TABLE[((bits >> 18) & 63) as usize] as char);
        output.push(TABLE[((bits >> 12) & 63) as usize] as char);
        output.push(if part.len() > 1 {
            TABLE[((bits >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if part.len() > 2 {
            TABLE[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

pub(super) fn strip_mcp_controls(arguments: &Value) -> Value {
    let Some(object) = arguments.as_object() else {
        return arguments.clone();
    };
    let mut params = object.clone();
    for key in [
        "confirm",
        "dry_run",
        "expected_revision",
        "idempotency_key",
        "session",
        "budget",
    ] {
        params.remove(key);
    }
    Value::Object(params)
}

pub(super) fn mcp_tools(attached: bool) -> Vec<Value> {
    let mut tools: Vec<Value> = vec![
        mcp_tool(
            "raf_set_mode",
            "Set the MCP session mode: inspect (read-only), plan (compile plans only) or active (normal mutations).",
            schema(
                vec![("mode", json!({"type":"string","enum":["inspect","plan","active"]}))],
                &["mode"],
                false,
            ),
        ),
        mcp_tool(
            "raf_status",
            "Read headless engine and project status.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_context",
            "Read compact project, session, workspace, and scene context. The attached editor returns live hierarchy data.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_doctor",
            "Check the local Rafi CLI environment.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_capabilities",
            "List commands supported by the current headless engine or attached project domain.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_capabilities_search",
            "Search commands available in the current headless engine or attached project domain.",
            json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
        ),
        mcp_tool(
            "raf_project_info",
            "Read project metadata.",
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        ),
        mcp_tool(
            "raf_project_path",
            "Return the exact project root of the live attached editor, or of the explicitly selected project, as a clean filesystem path. Use this before reading or writing project files.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_project_open",
            "Open a project directory in this headless session.",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        ),
        mcp_tool(
            "raf_project_create",
            "Preview or create a project directory. Filesystem mutation requires confirm=true.",
            json!({"type":"object","properties":{"name":{"type":"string"},"parent":{"type":"string"},"type":{"type":"string","enum":["game","electronics"]},"confirm":{"type":"boolean"},"dry_run":{"type":"boolean"}},"required":["name"]}),
        ),
        mcp_tool(
            "raf_project_save",
            "Save the mounted project document and its active editor domain. Use confirm=true to commit; dry_run previews the write.",
            schema(Vec::new(), &[], true),
        ),
        mcp_tool(
            "raf_project_close",
            "Close the mounted headless project session or detach the current project context. In an attached editor this requires confirm=true and never kills the application.",
            schema(Vec::new(), &[], true),
        ),
        mcp_tool(
            "raf_session_use",
            "Activate a project session by id or exact name. This updates the session registry; it does not mutate the live scene.",
            schema(vec![("id", json!({"type":"string","minLength":1}))], &["id"], true),
        ),
        mcp_tool(
            "raf_undo",
            "Request one editor undo step through the attached document history.",
            schema(Vec::new(), &[], true),
        ),
        mcp_tool(
            "raf_redo",
            "Request one editor redo step through the attached document history.",
            schema(Vec::new(), &[], true),
        ),
        mcp_tool(
            "raf_session_list",
            "List sessions in the open project.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_workspace_describe",
            "Return bounded workspace counts.",
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        ),
        mcp_tool(
            "raf_command",
            "Call a supported project-scoped command by name. In attached mode this reaches the live editor and its real undo history.",
            json!({"type":"object","properties":{"name":{"type":"string"},"params":{"type":"object"},"confirm":{"type":"boolean"},"dry_run":{"type":"boolean"},"expected_revision":{"type":"integer","minimum":0},"idempotency_key":{"type":"string"},"session":{"type":"string"},"budget":budget_schema()},"required":["name"]}),
        ),
    ]
    .into_iter()
    .chain(mcp_semantic_tools())
    .collect();
    if !attached {
        tools.retain(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(headless_mcp_tool_supported)
        });
    }
    tools
}

fn headless_mcp_tool_supported(name: &str) -> bool {
    matches!(
        name,
        "raf_set_mode"
            | "raf_status"
            | "raf_context"
            | "raf_doctor"
            | "raf_capabilities"
            | "raf_capabilities_search"
            | "raf_project_snapshot"
            | "raf_project_info"
            | "raf_project_open"
            | "raf_project_close"
            | "raf_project_create"
            | "raf_project_save"
            | "raf_session_list"
            | "raf_session_use"
            | "raf_workspace_describe"
            | "raf_scene_outline"
            | "raf_scene_digest"
            | "raf_scene_snapshot"
            | "raf_scene_query"
            | "raf_scene_spatial_map"
            | "raf_scene_overlaps"
            | "raf_scene_navigation_audit"
            | "raf_scene_diff"
            | "raf_scene_design_audit"
            | "raf_scene_inspect"
            | "raf_scene_verify"
            | "raf_assets_search"
            | "raf_assets_recommend"
            | "raf_asset_inspect"
            | "raf_scripts_search"
            | "raf_project_health"
            | "raf_diagnostics"
            | "raf_command"
    )
}
