//! CLI/MCP attached-command boundary for the native editor.
//!
//! Attached authoring commands enter through the same domain command gateway
//! used by the native editor. This module owns only transport adaptation and
//! revision responses; document mutation remains in the domain controllers.

use raf_core::agent_context::{self, AgentObservationResult};
use raf_core::capabilities::{CapabilityCatalog, CapabilityDefinition};
use raf_core::i18n::t;
use raf_core::project::Project;
use raf_core::project::ProjectType;
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::ProjectSessionRegistry;
use raf_core::{
    ArtifactRef, CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse,
    Language, TransactionId, TransactionLedger, UndoToken,
};
use serde_json::Value;
use std::path::Path;

use crate::agent_artifacts::capture_viewport_artifact;
use crate::attached::{AttachedCommandHost, PendingAttachedCommand};
use crate::commands::game::{
    GameCommandContext, GameViewportPort, HeadlessGameViewportPort, SceneSelectionState,
};
use crate::commands::gateway::{CommandGateway, ParsedCommandExecutor};
use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_project_controller::save_project_document;
use crate::native_workbench::NativeGameWorkbench;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AttachedPollResult {
    pub changed: bool,
    pub document_saved: bool,
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
    fn scene_entity_count(&self) -> Option<usize> {
        Some(self.scene.all_live_ids().len())
    }

    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        let mut context = GameCommandContext {
            scene: self.scene,
            selection: self.selection,
            viewport: self.viewport as &mut dyn GameViewportPort,
        };
        crate::commands::game::execute(&command.name, command, &mut context)
    }

    fn preview_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        let mut scene = self.scene.clone();
        let mut selection = self.selection.clone();
        let mut viewport = HeadlessGameViewportPort::default();
        viewport.set_selected_ids(selection.selected_nodes.clone());
        let mut context = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        crate::commands::game::execute(&command.name, command, &mut context)
    }
}

pub(crate) fn poll_attached_commands(
    attached_host: &mut AttachedCommandHost,
    pending: Vec<PendingAttachedCommand>,
    attached_ledger: &mut TransactionLedger,
    runtime: Option<&mut NativeEditorRuntime>,
    mut workbench: Option<&mut NativeGameWorkbench>,
    scene: &mut SceneGraph,
    mut electronics: Option<&mut NativeElectronicsEditor>,
    project: Option<&Project>,
    project_assets: &[String],
    catalog_pending: bool,
    catalog_error: Option<&str>,
    language: Language,
) -> AttachedPollResult {
    if pending.is_empty() {
        return AttachedPollResult::default();
    }
    let mut any_changed = false;
    let mut document_saved = false;
    let mut runtime = runtime;
    for command in pending {
        let request_id = command.request.id;
        let selected = runtime
            .as_deref()
            .map(|runtime| runtime.game_viewport().selected.clone())
            .unwrap_or_default();
        if is_project_save_command(&command.request.name) {
            let response = execute_attached_project_save(
                &command.request,
                project,
                runtime.as_deref(),
                scene,
                electronics.as_deref_mut(),
                attached_ledger,
                language,
            );
            document_saved |= response.ok
                && response
                    .data
                    .get("saved")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            attached_host.respond(command, response);
            continue;
        }
        if is_viewport_capture_command(&command.request.name) {
            let response = execute_attached_viewport_capture(
                &command.request,
                runtime.as_deref_mut(),
                project,
                attached_ledger.revision(),
                language,
            );
            attached_host.respond(command, response);
            continue;
        }
        if is_task_command(&command.request.name) {
            let response = execute_attached_task(
                &command.request,
                workbench.as_deref_mut(),
                attached_ledger.revision(),
                language,
            );
            attached_host.respond(command, response);
            continue;
        }
        let attached_revision = attached_ledger.revision();
        if let Some(response) = execute_attached_metadata(
            &command.request,
            project,
            scene,
            &selected,
            attached_ledger,
            attached_revision,
            project_assets,
            catalog_pending,
            catalog_error,
        ) {
            attached_host.respond(command, response);
            continue;
        }

        let Some(runtime) = runtime.as_deref_mut() else {
            attached_host.respond(
                command,
                EngineCommandResponse::error(
                    request_id,
                    "Editor unavailable",
                    "The native editor surface is not ready yet.",
                ),
            );
            continue;
        };

        if is_attached_undo_command(&command.request.name) {
            if project.is_none() {
                attached_host.respond(
                    command,
                    EngineCommandResponse::error(
                        request_id,
                        "No project is open",
                        "An attached undo token belongs to an open Game project.",
                    ),
                );
                continue;
            }
            let response = execute_attached_undo(&command.request, runtime, scene, attached_ledger);
            any_changed |= response.changed;
            if response.changed {
                runtime.graphics_mut().request_frame(
                    raf_render::api_graphic_basic::FrameInvalidation::DOCUMENT
                        | raf_render::api_graphic_basic::FrameInvalidation::UI,
                );
            }
            attached_host.respond(command, response);
            continue;
        }

        let mut game_before = None;
        let mut game_fingerprint_before = None;
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
        } else {
            game_before = Some(scene.clone());
            game_fingerprint_before = Some(raf_core::agent_context::scene_fingerprint(scene));
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
        let mut response = response;
        if response.changed {
            if let (Some(before), Some(transaction_id)) =
                (game_fingerprint_before.as_ref(), response.transaction_id)
            {
                let after = raf_core::agent_context::scene_fingerprint(scene);
                if let Some(diff) = raf_core::agent_context::scene_diff(before, &after) {
                    if response.diff.is_none() {
                        response.diff = Some(diff.clone());
                    }
                    attached_ledger.attach_diff(transaction_id, diff);
                }
            }
            if let (Some(before), Some(transaction_id)) = (game_before, response.transaction_id) {
                if let Some(token) =
                    runtime.record_attached_scene_change(before, scene, attached_ledger.revision())
                {
                    if attached_ledger.attach_undo_token(transaction_id, token) {
                        response.undo_token = Some(token);
                        response.undo_available = true;
                    }
                }
            }
        }
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
    AttachedPollResult {
        changed: any_changed,
        document_saved,
    }
}

/// Commands answered by the attached bridge itself. They describe the live
/// editor and must never fall through to a Game/Electronics domain parser.
fn is_metadata_command(name: &str) -> bool {
    matches!(
        name.trim_start_matches('/').to_ascii_lowercase().as_str(),
        "engine.status"
            | "status"
            | "engine.context"
            | "agent.context"
            | "project.summary"
            | "capabilities.list"
            | "capabilities.search"
            | "project.info"
            | "session.list"
            | "workspace.describe"
            | "scene.outline"
            | "scene.query"
            | "scene.spatial_map"
            | "scene.spatial"
            | "scene.overlaps"
            | "scene.check_overlaps"
            | "scene.diff"
            | "scene.design_audit"
            | "scene.layout_audit"
            | "scene.design_check"
            | "scene.inspect"
            | "selection.get"
            | "viewport.capture"
            | "viewport.screenshot"
            | "assets.catalog"
            | "assets.search"
            | "assets.inspect"
            | "assets.recommend"
            | "scripts.catalog"
            | "scripts.search"
            | "project.health"
            | "scene.verify"
            | "game.validate_layout"
            | "project.save"
            | "project.checkpoint"
            | "checkpoint"
    )
}

fn is_project_save_command(name: &str) -> bool {
    matches!(
        name.trim_start_matches('/').to_ascii_lowercase().as_str(),
        "project.save" | "project.checkpoint" | "checkpoint" | "save"
    )
}

fn is_attached_undo_command(name: &str) -> bool {
    matches!(
        name.trim_start_matches('/').to_ascii_lowercase().as_str(),
        "transaction.undo" | "undo.token"
    )
}

fn is_viewport_capture_command(name: &str) -> bool {
    matches!(
        name.trim_start_matches('/').to_ascii_lowercase().as_str(),
        "viewport.capture" | "viewport.screenshot"
    )
}

fn is_task_command(name: &str) -> bool {
    matches!(
        name.trim_start_matches('/').to_ascii_lowercase().as_str(),
        "task.list" | "tasks" | "task.get" | "task.events" | "task.cancel"
    )
}

fn execute_attached_task(
    request: &EngineCommandRequest,
    workbench: Option<&mut NativeGameWorkbench>,
    revision: u64,
    language: Language,
) -> EngineCommandResponse {
    let Some(workbench) = workbench else {
        return EngineCommandResponse::error(
            request.id,
            t("agent.task.title", language),
            t("agent.task.workbench_unavailable", language),
        );
    };
    let normalized = request.name.trim_start_matches('/').to_ascii_lowercase();
    match normalized.as_str() {
        "task.list" | "tasks" => {
            let tasks = workbench
                .agent_task_snapshots()
                .into_iter()
                .map(|task| serde_json::to_value(task).unwrap_or(Value::Null))
                .collect::<Vec<_>>();
            metadata_response(
                request.id,
                &t("agent.task.title", language),
                vec![format!(
                    "{} {}",
                    tasks.len(),
                    t("agent.task.retained", language)
                )],
                serde_json::json!({"tasks": tasks, "durable": false}),
                revision,
            )
        }
        "task.get" => {
            let Some(id) = parse_task_id(&request.params) else {
                return EngineCommandResponse::error(
                    request.id,
                    t("agent.task.title", language),
                    t("agent.task.invalid_id", language),
                );
            };
            let task = workbench.agent_task_snapshot_by_id(id);
            let found = task.is_some();
            let mut response = metadata_response(
                request.id,
                &t("agent.task.title", language),
                vec![if task.is_some() {
                    t("agent.task.found", language)
                } else {
                    t("agent.task.not_found", language)
                }],
                serde_json::json!({"task": task}),
                revision,
            );
            response.ok = found;
            response
        }
        "task.events" => {
            let since = request
                .params
                .get("since")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let events = workbench
                .agent_task_events_since(since)
                .into_iter()
                .map(|event| serde_json::to_value(event).unwrap_or(Value::Null))
                .collect::<Vec<_>>();
            metadata_response(
                request.id,
                &t("agent.task.events_title", language),
                vec![format!(
                    "{} {} {} {}.",
                    events.len(),
                    t("agent.task.events_prefix", language),
                    t("agent.task.events_after", language),
                    since
                )],
                serde_json::json!({"events": events, "since": since}),
                revision,
            )
        }
        "task.cancel" => {
            let Some(id) = parse_task_id(&request.params) else {
                return EngineCommandResponse::error(
                    request.id,
                    t("agent.task.cancel_title", language),
                    t("agent.task.invalid_id", language),
                );
            };
            let cancelled = workbench.cancel_agent_task(id);
            let mut response = metadata_response(
                request.id,
                &t("agent.task.cancel_title", language),
                vec![if cancelled {
                    t("agent.task.cancel_requested", language)
                } else {
                    t("agent.task.not_active", language)
                }],
                serde_json::json!({"task_id": id, "cancelled": cancelled}),
                revision,
            );
            response.changed = cancelled;
            response.ok = cancelled;
            response
        }
        _ => unreachable!("is_task_command filtered this command"),
    }
}

fn parse_task_id(params: &Value) -> Option<raf_core::AgentTaskId> {
    let value = params.get("id").or_else(|| params.get("task_id"))?;
    let id = value.as_str()?;
    uuid::Uuid::parse_str(id).ok().map(raf_core::AgentTaskId)
}

fn execute_attached_viewport_capture(
    request: &EngineCommandRequest,
    runtime: Option<&mut NativeEditorRuntime>,
    project: Option<&Project>,
    revision: u64,
    language: Language,
) -> EngineCommandResponse {
    let Some(runtime) = runtime else {
        return EngineCommandResponse::error(
            request.id,
            "Viewport capture",
            "The native editor surface is not ready yet.",
        );
    };
    let refresh_requested = request
        .params
        .get("refresh")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if refresh_requested {
        // The command is serviced before the next compositor tick. Requesting
        // the document frame here makes the capture fresh on the following
        // tick while preserving the synchronous IPC contract.
        runtime.request_canvas_frame();
    }
    let result = capture_viewport_artifact(runtime.graphics(), project, language);
    let mut response = metadata_response(
        request.id,
        &result.summary,
        result.details.clone(),
        result.data.clone(),
        revision,
    );
    response.ok = result.ok;
    response.warnings = result.warnings;
    if let Some(object) = response.data.as_object_mut() {
        object.insert(
            "refresh_requested".to_string(),
            Value::Bool(refresh_requested),
        );
        object.insert(
            "capture_is_last_completed_frame".to_string(),
            Value::Bool(refresh_requested),
        );
    }
    if refresh_requested {
        response.warnings.push(
            "A fresh viewport frame was requested; the artifact is the last completed frame available to this synchronous call.".to_string(),
        );
    }
    response.metrics = serde_json::json!({"attached": true, "observation": true, "artifact": true});
    if let Some(artifact) = result.data.get("artifact").and_then(Value::as_object) {
        if let (Some(id), Some(kind), Some(uri)) = (
            artifact.get("id").and_then(Value::as_str),
            artifact.get("kind").and_then(Value::as_str),
            artifact.get("uri").and_then(Value::as_str),
        ) {
            response.artifacts.push(ArtifactRef {
                id: id.to_string(),
                kind: kind.to_string(),
                uri: uri.to_string(),
                label: artifact
                    .get("label")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
    }
    response
}

fn execute_attached_project_save(
    request: &EngineCommandRequest,
    project: Option<&Project>,
    runtime: Option<&NativeEditorRuntime>,
    scene: &SceneGraph,
    mut electronics: Option<&mut NativeElectronicsEditor>,
    ledger: &mut TransactionLedger,
    language: Language,
) -> EngineCommandResponse {
    if let Err(error) = ledger.check_expected(request.expected_revision) {
        let mut response = EngineCommandResponse::error(
            request.id,
            t("agent.project_save.title", language),
            error,
        );
        response.revision = ledger.revision();
        return response;
    }

    if !request.dry_run {
        if let Some(existing) = ledger.find_idempotency_key(request.idempotency_key.as_deref()) {
            let mut response = metadata_response(
                request.id,
                &t("agent.project_save.title", language),
                vec![t("agent.project_save.replayed", language)],
                serde_json::json!({"saved": true, "replayed": true}),
                ledger.revision(),
            );
            response.transaction_id = Some(existing.id);
            response.changed = existing.changed;
            return response;
        }
    }

    let Some(project) = project else {
        return EngineCommandResponse::error(
            request.id,
            t("agent.project_save.title", language),
            t("agent.project_save.no_project", language),
        );
    };
    if !request.dry_run && !request.confirm {
        let mut response = EngineCommandResponse::error(
            request.id,
            t("agent.project_save.title", language),
            t("agent.project_save.confirm_required", language),
        );
        response.revision = ledger.revision();
        return response;
    }

    let diff = serde_json::json!({
        "operation": "project.save",
        "path": project.path,
        "project_type": project_type_label(project.project_type),
    });
    if request.dry_run {
        let mut response = metadata_response(
            request.id,
            &t("agent.project_save.title", language),
            vec![t("agent.project_save.preview", language)],
            serde_json::json!({"preview": diff, "would_change_files": true}),
            ledger.revision(),
        );
        response.diff = Some(diff);
        return response;
    }

    let Some(runtime) = runtime else {
        let mut response = EngineCommandResponse::error(
            request.id,
            t("agent.project_save.title", language),
            t("agent.project_save.runtime_unavailable", language),
        );
        response.revision = ledger.revision();
        return response;
    };
    if project.project_type == ProjectType::Electronics && electronics.is_none() {
        let mut response = EngineCommandResponse::error(
            request.id,
            t("agent.project_save.title", language),
            t("agent.project_save.electronics_unavailable", language),
        );
        response.revision = ledger.revision();
        return response;
    }

    if let Err(error) = save_project_document(project, scene, runtime.node_graph()) {
        let mut response = EngineCommandResponse::error(
            request.id,
            t("agent.project_save.title", language),
            format!("{}: {error}", t("agent.project_save.failure", language)),
        );
        response.revision = ledger.revision();
        return response;
    }
    if project.project_type == ProjectType::Electronics {
        if let Some(editor) = electronics.as_deref_mut() {
            if let Err(error) = editor.save(project) {
                let mut response = EngineCommandResponse::error(
                    request.id,
                    t("agent.project_save.title", language),
                    format!("{}: {error}", t("agent.project_save.failure", language)),
                );
                response.revision = ledger.revision();
                return response;
            }
        }
    }

    // Saving writes the current document but does not create a new scene
    // revision. The transaction remains recorded for idempotency and audit.
    let record = ledger.record_without_undo(
        request.transaction_id.unwrap_or_else(TransactionId::new),
        false,
        Some(diff.clone()),
        request.idempotency_key.clone(),
    );
    let mut response = metadata_response(
        request.id,
        &t("agent.project_save.title", language),
        vec![t("agent.project_save.completed", language)],
        serde_json::json!({"saved": true, "path": project.path}),
        record.revision_after,
    );
    response.transaction_id = Some(record.id);
    response.diff = Some(diff);
    response
}

fn execute_attached_undo(
    request: &EngineCommandRequest,
    runtime: &mut NativeEditorRuntime,
    scene: &mut SceneGraph,
    ledger: &mut TransactionLedger,
) -> EngineCommandResponse {
    if let Err(error) = ledger.check_expected(request.expected_revision) {
        let mut response = EngineCommandResponse::error(request.id, "Revision conflict", error);
        response.revision = ledger.revision();
        return response;
    }

    let token = request
        .params
        .get("token")
        .or_else(|| request.params.get("undo_token"))
        .cloned()
        .ok_or_else(|| "token is required for transaction.undo.".to_string())
        .and_then(|value| {
            serde_json::from_value::<UndoToken>(value)
                .map_err(|error| format!("token is invalid: {error}"))
        });
    let token = match token {
        Ok(token) => token,
        Err(error) => {
            let mut response = EngineCommandResponse::error(request.id, "Undo token", error);
            response.revision = ledger.revision();
            return response;
        }
    };

    let can_undo = runtime.can_undo_attached_scene(scene, token, ledger.revision());
    if request.dry_run {
        let mut response = metadata_response(
            request.id,
            "Undo preview",
            vec![if can_undo {
                "The attached scene edit can be rolled back.".to_string()
            } else {
                "The attached undo token is no longer valid.".to_string()
            }],
            serde_json::json!({"preview": true, "would_change": can_undo, "token_valid": can_undo}),
            ledger.revision(),
        );
        response.diff = Some(serde_json::json!({
            "preview": true,
            "would_change": can_undo,
            "operation": "transaction.undo",
        }));
        return response;
    }

    if !can_undo || !runtime.undo_attached_scene(scene, token, ledger.revision()) {
        let mut response = EngineCommandResponse::error(
            request.id,
            "Undo unavailable",
            "The token expired or the scene changed after the attached edit.",
        );
        response.revision = ledger.revision();
        return response;
    }

    let record = ledger.record_without_undo(
        request.transaction_id.unwrap_or_else(TransactionId::new),
        true,
        Some(serde_json::json!({"undone": true, "token": token})),
        request.idempotency_key.clone(),
    );
    EngineCommandResponse {
        protocol: raf_core::COMMAND_PROTOCOL_VERSION,
        id: request.id,
        ok: true,
        changed: true,
        title: "Undo applied".to_string(),
        lines: vec!["The attached scene edit was rolled back.".to_string()],
        data: serde_json::json!({"undone": true, "token": token}),
        warnings: Vec::new(),
        diff: Some(serde_json::json!({"undone": true})),
        undo_available: false,
        revision: record.revision_after,
        transaction_id: Some(record.id),
        undo_token: None,
        artifacts: Vec::new(),
        metrics: serde_json::json!({"attached": true, "undo": true}),
        verification: Some(raf_core::VerificationSummary {
            status: "passed".to_string(),
            checks: vec!["The scene fingerprint matches the pre-edit snapshot.".to_string()],
            failures: Vec::new(),
        }),
    }
}

fn execute_attached_metadata(
    request: &EngineCommandRequest,
    project: Option<&Project>,
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    ledger: &TransactionLedger,
    revision: u64,
    project_assets: &[String],
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> Option<EngineCommandResponse> {
    if !is_metadata_command(&request.name) {
        return None;
    }

    let normalized = request.name.trim_start_matches('/').to_ascii_lowercase();
    let project_type = project.map(|project| project.project_type);
    let capabilities = project_type
        .map(crate::native_project_controller::project_capabilities)
        .unwrap_or_default();

    if project_type != Some(ProjectType::Game)
        && matches!(
            normalized.as_str(),
            "scene.outline"
                | "scene.query"
                | "scene.spatial_map"
                | "scene.spatial"
                | "scene.overlaps"
                | "scene.check_overlaps"
                | "scene.diff"
                | "scene.design_audit"
                | "scene.layout_audit"
                | "scene.design_check"
                | "scene.inspect"
                | "selection.get"
                | "scene.verify"
                | "game.validate_layout"
                | "project.health"
        )
    {
        return Some(EngineCommandResponse::error(
            request.id,
            "Project observation",
            "Scene perception is currently available only for an attached Game project.",
        ));
    }

    let response = match normalized.as_str() {
        "engine.status" | "status" => {
            let entity_count = project_type
                .filter(|project_type| *project_type == ProjectType::Game)
                .map(|_| scene.all_live_ids().len())
                .unwrap_or_default();
            editor_status_response(request.id, project, entity_count, &capabilities, revision)
        }
        "capabilities.list" | "capabilities.search" => {
            let query = request
                .params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("");
            let catalog = CapabilityCatalog::builtin();
            let query = query.trim().to_ascii_lowercase();
            let entries: Vec<Value> = capabilities
                .iter()
                .filter(|name| {
                    query.is_empty()
                        || name.to_ascii_lowercase().contains(&query)
                        || catalog
                            .find(name)
                            .is_some_and(|definition| capability_matches(definition, &query))
                })
                .map(|name| {
                    catalog.find(name).map(capability_value).unwrap_or_else(|| {
                        serde_json::json!({
                            "name": name,
                            "aliases": [],
                            "domain": "shared",
                            "category": "inspection",
                            "description_key": "commands.attached_metadata.desc",
                            "parameters": [],
                            "examples": [],
                            "risk": "read",
                            "read_only": true,
                        })
                    })
                })
                .collect();
            metadata_response(
                request.id,
                if normalized == "capabilities.search" {
                    "Capabilities search"
                } else {
                    "Capabilities"
                },
                vec![format!("{} live capability(ies).", entries.len())],
                serde_json::json!({
                    "version": catalog.version,
                    "query": query,
                    "project_type": project_type.map(project_type_label),
                    "capabilities": entries,
                }),
                revision,
            )
        }
        "project.info" => {
            let Some(project) = project else {
                return Some(EngineCommandResponse::error(
                    request.id,
                    "Project info",
                    "No project is open in the attached editor.",
                ));
            };
            metadata_response(
                request.id,
                "Project info",
                vec![format!(
                    "{} ({}).",
                    project.name,
                    project_type_label(project.project_type)
                )],
                project_info_value(project),
                revision,
            )
        }
        "session.list" => {
            let Some(project) = project else {
                return Some(EngineCommandResponse::error(
                    request.id,
                    "Sessions",
                    "No project is open in the attached editor.",
                ));
            };
            let registry =
                ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
            let sessions: Vec<Value> = registry
                .sessions
                .iter()
                .map(|session| {
                    serde_json::json!({
                        "id": session.id,
                        "name": session.name,
                        "kind": format!("{:?}", session.kind),
                        "directory": session.directory,
                        "active": session.id == registry.active_session,
                    })
                })
                .collect();
            metadata_response(
                request.id,
                "Sessions",
                vec![format!("{} session(s).", sessions.len())],
                serde_json::json!({
                    "sessions": sessions,
                    "active_session": registry.active_session,
                }),
                revision,
            )
        }
        "workspace.describe" => {
            let Some(project) = project else {
                return Some(EngineCommandResponse::error(
                    request.id,
                    "Workspace",
                    "No project is open in the attached editor.",
                ));
            };
            let (files, directories) = count_workspace_entries(&project.path);
            metadata_response(
                request.id,
                "Workspace",
                vec![format!("{} files, {} directories.", files, directories)],
                serde_json::json!({
                    "path": project.path,
                    "files": files,
                    "directories": directories,
                    "internal_metadata_skipped": true,
                }),
                revision,
            )
        }
        "engine.context" | "agent.context" | "project.summary" => {
            let Some(project) = project else {
                return Some(EngineCommandResponse::error(
                    request.id,
                    "Project context",
                    "No project is open in the attached editor.",
                ));
            };
            let registry =
                ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
            let active_session = registry.active().map(|session| {
                serde_json::json!({"id": session.id, "name": session.name, "kind": format!("{:?}", session.kind)})
            });
            let (files, directories) = count_workspace_entries(&project.path);
            let scene_data = if project.project_type == ProjectType::Game {
                agent_context::scene_context(scene, selected)
            } else {
                serde_json::Value::Null
            };
            let used_assets = project_assets
                .iter()
                .filter(|asset| {
                    scene.iter().any(|(_, node)| {
                        node.source_asset.as_deref().is_some_and(|source| {
                            let asset = normalize_asset_path(asset);
                            let source = normalize_asset_path(source);
                            asset == source || asset.ends_with(&source) || source.ends_with(&asset)
                        })
                    })
                })
                .count();
            metadata_response(
                request.id,
                "Project context",
                vec![format!(
                    "Live {} context at revision {}.",
                    project_type_label(project.project_type),
                    revision
                )],
                serde_json::json!({
                    "project": project_info_value(project),
                    "session": active_session,
                    "revision": revision,
                    "capabilities": capabilities,
                    "workspace": {"files": files, "directories": directories, "internal_metadata_skipped": true},
                    "scene": scene_data,
                    "assets": {"imported": project_assets.len(), "used": used_assets, "unused": project_assets.len().saturating_sub(used_assets), "catalog_pending": catalog_pending},
                    "runtime_enabled": false,
                    "play_enabled": false,
                }),
                revision,
            )
        }
        "scene.outline" => observation_metadata_response(
            request.id,
            agent_context::scene_outline(scene, &request.params, selected),
            revision,
        ),
        "scene.query" => observation_metadata_response(
            request.id,
            agent_context::scene_query(scene, &request.params, selected),
            revision,
        ),
        "scene.spatial_map" | "scene.spatial" => observation_metadata_response(
            request.id,
            agent_context::scene_spatial_map(scene, &request.params, selected),
            revision,
        ),
        "scene.overlaps" | "scene.check_overlaps" => observation_metadata_response(
            request.id,
            agent_context::scene_check_overlaps(scene, &request.params, selected),
            revision,
        ),
        "scene.diff" => {
            let from_revision = request
                .params
                .get("from_revision")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| revision.saturating_sub(1));
            match ledger.diff_since(from_revision) {
                Ok(data) => metadata_response(
                    request.id,
                    "Scene diff",
                    vec![format!(
                        "Scene changes from revision {from_revision} to {revision}."
                    )],
                    data,
                    revision,
                ),
                Err(error) => {
                    let mut response =
                        EngineCommandResponse::error(request.id, "Scene diff", error);
                    response.revision = revision;
                    response
                }
            }
        }
        "scene.design_audit" | "scene.layout_audit" | "scene.design_check" => {
            observation_metadata_response(
                request.id,
                agent_context::scene_design_audit(scene, &request.params, selected),
                revision,
            )
        }
        "scene.inspect" => observation_metadata_response(
            request.id,
            agent_context::scene_inspect(scene, &request.params, selected),
            revision,
        ),
        "selection.get" => observation_metadata_response(
            request.id,
            agent_context::selection_info(scene, selected),
            revision,
        ),
        "assets.inspect" => observation_metadata_response(
            request.id,
            agent_context::asset_inspect(
                scene,
                project_assets,
                &request.params,
                catalog_pending,
                catalog_error,
            ),
            revision,
        ),
        "assets.catalog" | "assets.search" => observation_metadata_response(
            request.id,
            agent_context::assets_catalog(
                scene,
                project_assets,
                &request.params,
                catalog_pending,
                catalog_error,
            ),
            revision,
        ),
        "assets.recommend" => observation_metadata_response(
            request.id,
            agent_context::assets_recommend(
                scene,
                project_assets,
                &request.params,
                catalog_pending,
                catalog_error,
            ),
            revision,
        ),
        "scripts.catalog" | "scripts.search" => observation_metadata_response(
            request.id,
            agent_context::scripts_catalog(scene, project_assets, &request.params),
            revision,
        ),
        "project.health" => observation_metadata_response(
            request.id,
            agent_context::project_health_scoped(scene, project_assets, &request.params),
            revision,
        ),
        "scene.verify" | "game.validate_layout" => observation_metadata_response(
            request.id,
            agent_context::scene_verify(scene, &request.params, selected),
            revision,
        ),
        _ => unreachable!("is_metadata_command filtered this command"),
    };
    Some(response)
}

fn observation_metadata_response(
    id: raf_core::CommandId,
    observation: AgentObservationResult,
    revision: u64,
) -> EngineCommandResponse {
    let ok = observation.is_success();
    let mut response = metadata_response(
        id,
        &observation.title,
        vec![observation.summary],
        observation.data,
        revision,
    );
    response.ok = ok;
    response.warnings = observation.warnings;
    response.verification = observation.verification;
    response.metrics = serde_json::json!({"attached": true, "observation": true});
    response
}

fn metadata_response(
    id: raf_core::CommandId,
    title: &str,
    lines: Vec<String>,
    data: Value,
    revision: u64,
) -> EngineCommandResponse {
    EngineCommandResponse {
        protocol: raf_core::COMMAND_PROTOCOL_VERSION,
        id,
        ok: true,
        changed: false,
        title: title.to_string(),
        lines,
        data,
        warnings: Vec::new(),
        diff: None,
        undo_available: false,
        revision,
        transaction_id: None,
        undo_token: None,
        artifacts: Vec::new(),
        metrics: serde_json::json!({"attached": true}),
        verification: None,
    }
}

fn project_info_value(project: &Project) -> Value {
    serde_json::json!({
        "id": project.id,
        "name": project.name,
        "type": project_type_label(project.project_type),
        "path": project.path,
        "engine_version": project.engine_version,
        "created_at": project.created_at,
        "modified_at": project.modified_at,
        "settings": project.settings,
    })
}

fn project_type_label(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Game => "game",
        ProjectType::Electronics => "electronics",
    }
}

fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn capability_matches(definition: &CapabilityDefinition, query: &str) -> bool {
    definition.domain.to_ascii_lowercase().contains(query)
        || definition.category.to_ascii_lowercase().contains(query)
        || definition
            .description_key
            .to_ascii_lowercase()
            .contains(query)
        || definition
            .aliases
            .iter()
            .any(|alias| alias.to_ascii_lowercase().contains(query))
        || definition
            .examples
            .iter()
            .any(|example| example.to_ascii_lowercase().contains(query))
}

fn capability_value(capability: &CapabilityDefinition) -> Value {
    serde_json::json!({
        "name": capability.name,
        "aliases": capability.aliases,
        "domain": capability.domain,
        "category": capability.category,
        "description_key": capability.description_key,
        "parameters": capability.parameters,
        "examples": capability.examples,
        "risk": capability.risk(),
        "read_only": capability.is_read_only(),
    })
}

fn count_workspace_entries(root: &Path) -> (usize, usize) {
    fn visit(path: &Path, files: &mut usize, directories: &mut usize, depth: u8) {
        if depth > 8 || *files + *directories >= 8_192 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git"
                    | ".aura_rafi"
                    | ".ai"
                    | ".codex"
                    | "target"
                    | "target_gnu"
                    | "node_modules"
                    | ".cache"
            ) || name == "agent_history.ron"
                || name == "agent_endpoint.json"
            {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                *directories += 1;
                visit(&entry.path(), files, directories, depth.saturating_add(1));
            } else if kind.is_file() {
                *files += 1;
            }
        }
    }
    let mut files = 0;
    let mut directories = 0;
    visit(root, &mut files, &mut directories, 0);
    (files, directories)
}

fn editor_status_response(
    id: raf_core::CommandId,
    project: Option<&Project>,
    entity_count: usize,
    capabilities: &[String],
    revision: u64,
) -> EngineCommandResponse {
    let active_session = project.and_then(|project| {
        ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type)
            .active()
            .map(|session| serde_json::json!({"id": session.id, "name": session.name, "kind": format!("{:?}", session.kind)}))
    });
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
            "session": active_session,
            "runtime_enabled": false,
            "play_enabled": false,
        }),
        warnings: Vec::new(),
        diff: None,
        undo_available: false,
        revision,
        transaction_id: None,
        undo_token: None,
        artifacts: Vec::new(),
        metrics: serde_json::Value::Null,
        verification: None,
    }
}
