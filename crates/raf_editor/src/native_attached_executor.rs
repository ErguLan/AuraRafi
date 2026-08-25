//! CLI/MCP attached-command boundary for the native editor.
//!
//! Attached authoring commands enter through the same domain command gateway
//! used by the native editor. This module owns only transport adaptation and
//! revision responses; document mutation remains in the domain controllers.

use raf_core::project::Project;
use raf_core::scene::SceneGraph;
use raf_core::{
    CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse, TransactionLedger,
};
use serde_json::Value;

use crate::attached::AttachedCommandHost;
use crate::commands::game::{GameCommandContext, GameViewportPort, SceneSelectionState};
use crate::commands::gateway::{CommandGateway, ParsedCommandExecutor};
use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_editor_runtime::NativeEditorRuntime;

struct NativeGameCommandExecutor<'a> {
    scene: &'a mut SceneGraph,
    selection: &'a mut SceneSelectionState,
    viewport: &'a mut crate::panels::viewport_controller::NativeGameViewportController,
}

struct NativeElectronicsCommandExecutor<'a> {
    editor: &'a mut NativeElectronicsEditor,
}

struct NativeElectronicsUiCommandExecutor<'a> {
    editor: &'a mut NativeElectronicsEditor,
}

impl ParsedCommandExecutor for NativeElectronicsUiCommandExecutor<'_> {
    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        match command.name.as_str() {
            "electronics.analysis.drc" => {
                self.editor.start_drc_analysis();
                return CommandOutput::changed(
                    "Electronics DRC",
                    vec!["DRC started in the background".to_string()],
                    serde_json::json!({"ok": true, "command": command.name, "status": "running"}),
                );
            }
            "electronics.analysis.simulation" => {
                self.editor.start_simulation_analysis();
                return CommandOutput::changed(
                    "Electronics simulation",
                    vec!["DC simulation started in the background".to_string()],
                    serde_json::json!({"ok": true, "command": command.name, "status": "running"}),
                );
            }
            "electronics.analysis.cancel" => {
                self.editor.cancel_analysis();
                return CommandOutput::changed(
                    "Electronics analysis",
                    vec!["Analysis cancellation requested".to_string()],
                    serde_json::json!({"ok": true, "command": command.name, "status": "cancelled"}),
                );
            }
            _ => {}
        }
        if self.editor.apply_ui_command(&command.name) {
            CommandOutput::changed(
                "Electronics UI command",
                vec![format!("Applied {}", command.name)],
                serde_json::json!({"ok": true, "command": command.name}),
            )
        } else {
            CommandOutput::error(
                "Electronics UI command",
                format!("Unknown native Electronics UI command: {}", command.name),
            )
        }
    }
}

pub(crate) fn execute_native_ui_intent(
    editor: &mut NativeElectronicsEditor,
    ledger: &mut TransactionLedger,
    command_name: &str,
) -> EngineCommandResponse {
    let executor = NativeElectronicsUiCommandExecutor { editor };
    let mut gateway = CommandGateway {
        executor,
        ledger: std::mem::take(ledger),
    };
    let response = gateway.execute(EngineCommandRequest::new(
        command_name,
        Value::Null,
        CommandSource::RafUi,
    ));
    *ledger = gateway.ledger;
    response
}

impl ParsedCommandExecutor for NativeElectronicsCommandExecutor<'_> {
    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        self.editor.execute_catalog_command(&command.name, command)
    }
}

impl ParsedCommandExecutor for NativeGameCommandExecutor<'_> {
    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        let mut context = GameCommandContext {
            scene: self.scene,
            selection: self.selection,
            viewport: self.viewport as &mut dyn GameViewportPort,
        };
        crate::commands::game::execute(&command.name, command, &mut context)
    }
}

pub(crate) fn poll_attached_commands(
    attached_host: &mut AttachedCommandHost,
    attached_ledger: &mut TransactionLedger,
    runtime: Option<&mut NativeEditorRuntime>,
    scene: &mut SceneGraph,
    mut electronics: Option<&mut NativeElectronicsEditor>,
    project: Option<&Project>,
) -> bool {
    let pending = attached_host.drain();
    if pending.is_empty() {
        return false;
    }
    let mut any_changed = false;
    let Some(runtime) = runtime else {
        for command in pending {
            let request_id = command.request.id;
            let response = if is_status_probe(&command.request.name) {
                editor_status_response(
                    request_id,
                    project,
                    0,
                    &[
                        "project.info".to_string(),
                        "session.list".to_string(),
                        "workspace.describe".to_string(),
                    ],
                )
            } else {
                EngineCommandResponse::error(
                    request_id,
                    "Editor unavailable",
                    "The native editor surface is not ready yet.",
                )
            };
            attached_host.respond(command, response);
        }
        return false;
    };

    for command in pending {
        let request_id = command.request.id;
        if is_status_probe(&command.request.name) {
            let entity_count = if project
                .is_some_and(|project| project.project_type == raf_core::project::ProjectType::Game)
            {
                scene.len()
            } else {
                0
            };
            let capabilities = match project.map(|project| project.project_type) {
                Some(raf_core::project::ProjectType::Game) => {
                    crate::native_project_controller::game_capabilities(
                        raf_core::project::ProjectType::Game,
                    )
                }
                Some(raf_core::project::ProjectType::Electronics) => {
                    crate::native_project_controller::electronics_capabilities()
                }
                None => Vec::new(),
            };
            attached_host.respond(
                command,
                editor_status_response(request_id, project, entity_count, &capabilities),
            );
            continue;
        }
        let response = if project.is_none() {
            EngineCommandResponse::error(
                request_id,
                "No project is open",
                "Open a project before sending attached CLI/MCP authoring commands.",
            )
        } else if project.is_some_and(|project| {
            project.project_type == raf_core::project::ProjectType::Electronics
        }) {
            let Some(editor) = electronics.as_deref_mut() else {
                let response = EngineCommandResponse::error(
                    request_id,
                    "Electronics editor unavailable",
                    "The native Electronics document is not mounted yet.",
                );
                attached_host.respond(command, response);
                continue;
            };
            let executor = NativeElectronicsCommandExecutor { editor };
            let mut gateway = CommandGateway {
                executor,
                ledger: std::mem::take(attached_ledger),
            };
            let response = gateway.execute(command.request.clone());
            *attached_ledger = gateway.ledger;
            response
        } else if command.request.name == "game.batch" {
            execute_game_batch(command.request.clone(), scene, runtime, attached_ledger)
        } else {
            let current_selection = runtime.game_viewport().selected.clone();
            let mut selection = SceneSelectionState {
                selected_node: current_selection.first().copied(),
                selected_nodes: current_selection,
            };
            let executor = NativeGameCommandExecutor {
                scene,
                selection: &mut selection,
                viewport: runtime.game_viewport_mut(),
            };
            let mut gateway = CommandGateway {
                executor,
                ledger: std::mem::take(attached_ledger),
            };
            let response = gateway.execute(command.request.clone());
            *attached_ledger = gateway.ledger;
            response
        };
        any_changed |= response.changed;
        if response.changed {
            runtime.graphics_mut().request_frame(
                raf_render::api_graphic_basic::FrameInvalidation::DOCUMENT
                    | raf_render::api_graphic_basic::FrameInvalidation::UI,
            );
        }
        attached_host.respond(command, response);
    }
    attached_host.update_revision(attached_ledger.revision());
    any_changed
}

/// Domain-agnostic status probe so agents learn the editor state before
/// choosing a command domain.
fn is_status_probe(name: &str) -> bool {
    matches!(name, "engine.status" | "status")
}

fn editor_status_response(
    id: raf_core::CommandId,
    project: Option<&Project>,
    entity_count: usize,
    capabilities: &[String],
) -> EngineCommandResponse {
    EngineCommandResponse {
        protocol: raf_core::COMMAND_PROTOCOL_VERSION,
        id,
        ok: true,
        changed: false,
        title: "Editor status".to_string(),
        lines: vec![match project {
            Some(project) => format!(
                "{} ({:?}) is open with {} entitie(s).",
                project.name, project.project_type, entity_count
            ),
            None => "Editor is at the Hub; no project is open.".to_string(),
        }],
        data: serde_json::json!({
            "attached": true,
            "editor_state": if project.is_some() { "project" } else { "hub" },
            "project": project.map(|project| serde_json::json!({
                "id": project.id,
                "name": project.name,
                "type": match project.project_type {
                    raf_core::project::ProjectType::Game => "game",
                    raf_core::project::ProjectType::Electronics => "electronics",
                },
                "path": project.path,
            })),
            "entity_count": entity_count,
            "capabilities": capabilities,
            "runtime_enabled": false,
            "play_enabled": false,
        }),
        warnings: Vec::new(),
        diff: None,
        undo_available: false,
        revision: 0,
        transaction_id: None,
        undo_token: None,
        artifacts: Vec::new(),
        metrics: serde_json::Value::Null,
        verification: None,
    }
}

/// Executes a list of game operations as one round trip. Each operation runs
/// through the normal gateway (history, idempotency, revision), so a batch is
/// a convenience wrapper, not a parallel transaction path.
fn execute_game_batch(
    request: raf_core::EngineCommandRequest,
    scene: &mut SceneGraph,
    runtime: &mut NativeEditorRuntime,
    attached_ledger: &mut TransactionLedger,
) -> EngineCommandResponse {
    let id = request.id;
    let Some(operations) = request
        .params
        .get("operations")
        .and_then(serde_json::Value::as_array)
        .cloned()
    else {
        return EngineCommandResponse::error(
            id,
            "Game batch",
            "params.operations must be an array of { name, params } entries.",
        );
    };
    if operations.len() > 64 {
        return EngineCommandResponse::error(
            id,
            "Game batch",
            "A batch accepts at most 64 operations.",
        );
    }

    let mut results = Vec::new();
    let mut any_changed = false;
    let mut all_ok = true;
    for operation in operations {
        let Some(name) = operation.get("name").and_then(serde_json::Value::as_str) else {
            all_ok = false;
            results.push(serde_json::json!({"ok": false, "error": "missing name"}));
            continue;
        };
        if name == "game.batch" {
            all_ok = false;
            results.push(serde_json::json!({"ok": false, "error": "nested batches are rejected"}));
            continue;
        }
        let params = operation
            .get("params")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let mut sub_request = raf_core::EngineCommandRequest::new(name, params, request.source);
        sub_request.confirm = request.confirm;
        sub_request.idempotency_key = request.idempotency_key.clone();

        let current_selection = runtime.game_viewport().selected.clone();
        let mut selection = SceneSelectionState {
            selected_node: current_selection.first().copied(),
            selected_nodes: current_selection,
        };
        let executor = NativeGameCommandExecutor {
            scene,
            selection: &mut selection,
            viewport: runtime.game_viewport_mut(),
        };
        let mut gateway = CommandGateway {
            executor,
            ledger: std::mem::take(attached_ledger),
        };
        let response = gateway.execute(sub_request);
        *attached_ledger = gateway.ledger;
        runtime.game_viewport_mut().selected = selection.selected_nodes;
        any_changed |= response.changed;
        all_ok &= response.ok;
        if response.changed {
            runtime.graphics_mut().request_frame(
                raf_render::api_graphic_basic::FrameInvalidation::DOCUMENT
                    | raf_render::api_graphic_basic::FrameInvalidation::UI,
            );
        }
        results.push(serde_json::json!({
            "name": name,
            "ok": response.ok,
            "changed": response.changed,
            "title": response.title,
            "lines": response.lines,
            "undo_token": response.undo_token,
        }));
    }

    EngineCommandResponse {
        protocol: raf_core::COMMAND_PROTOCOL_VERSION,
        id,
        ok: all_ok,
        changed: any_changed,
        title: "Game batch".to_string(),
        lines: vec![format!(
            "{} operation(s) executed; {} changed the scene.",
            results.len(),
            results
                .iter()
                .filter(|r| r["changed"] == serde_json::json!(true))
                .count()
        )],
        data: serde_json::json!({ "results": results }),
        warnings: Vec::new(),
        diff: None,
        undo_available: any_changed,
        revision: attached_ledger.revision(),
        transaction_id: None,
        undo_token: None,
        artifacts: Vec::new(),
        metrics: serde_json::Value::Null,
        verification: None,
    }
}
