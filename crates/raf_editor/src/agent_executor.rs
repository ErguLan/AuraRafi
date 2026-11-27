//! UI-independent Agent tool layer for the native editor.
//!
//! Provider tool calls stay structured here. Mutations enter the same typed
//! command gateway used by RafUI, CLI and MCP; no tool is converted to a CLI
//! command line and no transport is spawned inside the editor.

mod capture;
mod evidence;
mod execution;
mod image;
mod intent;
mod performance;
mod schemas;
mod scripting;
#[cfg(test)]
mod tests;
mod tool_pack;
mod world_build;

use std::collections::HashMap;
use std::path::PathBuf;

use raf_ai::agent_runtime::{AgentToolKind, AgentToolResult, ToolExecutionMode, ToolExecutor};
use raf_ai::provider::AgentMode;
use raf_core::project::{Project, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::{
    CommandEndpoint, CommandSource, EngineCommandRequest, ExecutionBudget, Language,
    TransactionLedger,
};
use serde_json::Value;

use crate::agent_context::AgentObservationContext;
use crate::commands::catalog::CommandCatalog;
use crate::commands::game::{GameViewportPort, SceneSelectionState};
use crate::commands::gateway::CommandGateway;
use crate::electronics_controller::NativeElectronicsEditor;
use raf_render::bridge::RenderRuntime;

use self::image::visual_review_tool_result;
use evidence::{
    expects_observable_scene_change, live_scene_entity_count, request_preview,
    response_to_tool_result,
};
use execution::AgentParsedExecutor;
use world_build::translate_arguments;

pub use schemas::sanitize_tool_name;
pub use tool_pack::{AgentToolPack, AgentToolRoute};

pub(crate) use capture::{capture_request, capture_target_values};
pub(crate) use evidence::verify_postconditions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEditorAction {
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
pub struct AgentProjectContext {
    pub root: PathBuf,
    pub project_type: ProjectType,
}

impl AgentProjectContext {
    pub fn from_project(project: Option<&Project>) -> Option<Self> {
        project.map(|project| Self {
            root: project.path.clone(),
            project_type: project.project_type,
        })
    }
}

pub struct AgentToolExecutor<'a> {
    pub(crate) asset_tools: &'a mut crate::agent_asset_tools::AgentAssetTools,
    pub(crate) asset_studio: &'a raf_core::ai::ExternalAssetStudio,
    pub(crate) image_provider: Option<raf_ai::AssetImageProvider>,
    pub(crate) image_model: String,
    pub(crate) vision_reviewer: Option<raf_ai::vision_bridge::VisionReviewer>,
    pub scene: &'a mut SceneGraph,
    pub selection: &'a mut SceneSelectionState,
    pub viewport: &'a mut dyn GameViewportPort,
    pub graphics: Option<&'a mut RenderRuntime>,
    pub electronics: Option<&'a mut NativeElectronicsEditor>,
    pub project: Option<AgentProjectContext>,
    pub project_info: Option<&'a Project>,
    pub project_assets: &'a [String],
    pub active_session: &'a str,
    pub catalog_pending: bool,
    pub catalog_error: Option<&'a str>,
    pub catalog: &'a CommandCatalog,
    pub language: Language,
    pub routes: HashMap<String, AgentToolRoute>,
    pub ledger: &'a mut TransactionLedger,
    pub editor_actions: &'a mut Vec<AgentEditorAction>,
    pub canvas_changed: &'a mut bool,
}

impl AgentToolExecutor<'_> {
    pub fn build_tool_pack(
        catalog: &CommandCatalog,
        language: Language,
        project_type: Option<ProjectType>,
        mode: AgentMode,
        prompt: &str,
    ) -> AgentToolPack {
        tool_pack::build_tool_pack(catalog, language, project_type, mode, prompt)
    }

    fn observation_context(&self) -> AgentObservationContext<'_> {
        AgentObservationContext {
            scene: self.scene,
            selection: self.selection,
            project: self.project_info,
            assets: self.project_assets,
            active_session: self.active_session,
            revision: self.ledger.revision(),
            catalog_pending: self.catalog_pending,
            catalog_error: self.catalog_error,
        }
    }

    fn execute_electronics_lab(
        &mut self,
        call_id: &str,
        name: &str,
        arguments: &Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let editor = self
            .electronics
            .as_deref_mut()
            .ok_or("Electronics document is not mounted.")?;
        match name {
            "electronics_lab_simulate" => {
                editor.start_lab_simulation(
                    Some(call_id.to_owned()),
                    mode == ToolExecutionMode::Apply,
                )?;
                Ok(AgentToolResult::success(
                    "DC analysis queued.",
                    serde_json::json!({"state":"queued"}),
                ))
            }
            "electronics_lab_probe" => {
                let run = editor.lab.run().ok_or("Run a DC analysis first.")?;
                Ok(laboratory_probe(run, editor.lab_fingerprint(), arguments))
            }
            _ => {
                editor.poll_lab(false);
                let Some(serial) = editor.lab.serial.as_ref() else {
                    return Ok(AgentToolResult::success(
                        "No connected serial device.",
                        serde_json::json!({"connected":false}),
                    ));
                };
                let channels: Vec<_> = serial.latest.values().take(16).collect();
                Ok(AgentToolResult::success(
                    "Latest serial measurements.",
                    serde_json::json!({
                        "source":"measured", "connected":serial.state == raf_hardware::serial_session::LinkState::Open,
                        "device":serial.device.as_ref().map(|device| &device.board), "port":editor.lab.port,
                        "received":serial.received, "invalid":serial.invalid, "evicted":serial.evicted, "channels":channels,
                    }),
                ))
            }
        }
    }

    fn execute_gateway(
        &mut self,
        call_id: &str,
        command_name: &str,
        params: Value,
        kind: AgentToolKind,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let before = raf_core::agent_context::scene_fingerprint(self.scene);
        let expected_params = params.clone();
        let domain = self
            .catalog
            .find(command_name)
            .map(|command| command.domain.as_str())
            .unwrap_or_else(|| {
                if command_name.starts_with("game.") {
                    "game"
                } else {
                    "shared"
                }
            });

        if mode == ToolExecutionMode::Preview && domain == "electronics" {
            let mut result = AgentToolResult::success(
                format!("Previewed {command_name}; no Electronics state was changed."),
                serde_json::json!({"command": command_name, "params": params}),
            );
            result.preview = true;
            result.diff = Some(serde_json::json!({"preview": true, "would_change": true}));
            result.revision = Some(self.ledger.revision());
            return Ok(result);
        }

        let executor = AgentParsedExecutor {
            scene: self.scene,
            selection: self.selection,
            viewport: self.viewport,
            electronics: self.electronics.as_deref_mut(),
            project: self.project.as_ref(),
            editor_actions: self.editor_actions,
        };
        let mut gateway = CommandGateway {
            executor,
            ledger: std::mem::take(self.ledger),
        };
        let mut request = EngineCommandRequest::new(command_name, params, CommandSource::Agent);
        request.session = Some(self.active_session.to_string());
        request.confirm = mode == ToolExecutionMode::Apply;
        request.dry_run = kind == AgentToolKind::Mutation && mode == ToolExecutionMode::Preview;
        request.expected_revision = Some(gateway.ledger.revision());
        request.budget = Some(native_agent_budget());
        if !request.dry_run {
            request.idempotency_key = Some(call_id.to_string());
        }
        let response = gateway.execute(request);
        *self.ledger = gateway.ledger;
        let after = raf_core::agent_context::scene_fingerprint(self.scene);
        let scene_diff = raf_core::agent_context::scene_diff(&before, &after);
        if let (Some(transaction_id), Some(diff)) = (response.transaction_id, scene_diff.as_ref()) {
            self.ledger.attach_diff(transaction_id, diff.clone());
        }
        let observed_scene_change = scene_diff.is_some();
        let mut result = response_to_tool_result(response, request_preview(mode, kind), scene_diff);
        if result.ok && kind == AgentToolKind::Mutation {
            let is_game_mutation = command_name.starts_with("game.")
                || command_name.starts_with("behavior.")
                || matches!(command_name, "lighting.set" | "lighting.reset")
                || command_name == "prefab.instantiate"
                || command_name == "assembly.revert"
                || command_name == "assembly.update"
                || command_name == "assembly.make_unique";
            let scene_changed =
                observed_scene_change && is_game_mutation && mode == ToolExecutionMode::Apply;
            if is_game_mutation {
                let replayed = result
                    .data
                    .get("replayed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if result.changed && !scene_changed && !replayed {
                    if !matches!(command_name, "game.reconcile" | "game.repair") {
                        result.warnings.push(
                            "The game command reported a change, but the scene state did not change."
                                .to_string(),
                        );
                    }
                }
                let no_change_failure = mode == ToolExecutionMode::Apply
                    && expects_observable_scene_change(command_name)
                    && !scene_changed
                    && !replayed
                    && !matches!(command_name, "game.reconcile" | "game.repair");
                if no_change_failure {
                    result.ok = false;
                    result.summary = format!(
                        "No scene change was observed after {command_name}; inspect the result before continuing."
                    );
                    result.warnings.push(
                        "The mutation was reported as successful, but no observable scene diff was found."
                            .to_string(),
                    );
                }
                result.changed = scene_changed;
                let postcondition_failures = if mode == ToolExecutionMode::Apply
                    && (scene_changed || matches!(command_name, "game.reconcile" | "game.repair"))
                    && !replayed
                {
                    verify_postconditions(command_name, &expected_params, &result.data, self.scene)
                } else {
                    Vec::new()
                };
                if !postcondition_failures.is_empty() {
                    result.ok = false;
                    result.summary = format!(
                        "{command_name} changed the scene, but its result did not match the requested state."
                    );
                    result.warnings.extend(
                        postcondition_failures
                            .iter()
                            .map(|failure| format!("Postcondition: {failure}")),
                    );
                }
                result.verification = Some(serde_json::json!({
                    "status": if request_preview(mode, kind) {
                        "preview"
                    } else if replayed {
                        "replayed"
                    } else if !postcondition_failures.is_empty() {
                        "failed"
                    } else if no_change_failure {
                        "failed"
                    } else if scene_changed {
                        "passed"
                    } else {
                        "no_change"
                    },
                    "scene_changed": scene_changed,
                    "revision": self.ledger.revision(),
                    "scene_entities": live_scene_entity_count(self.scene),
                    "postcondition_failures": postcondition_failures
                }));
            } else if mode == ToolExecutionMode::Apply {
                *self.canvas_changed = true;
                result.verification = Some(serde_json::json!({
                    "status": "passed",
                    "revision": self.ledger.revision(),
                    "scene_entities": live_scene_entity_count(self.scene)
                }));
            }
            if scene_changed {
                *self.canvas_changed = true;
            }
        }
        // Domain failures remain structured tool results so the model receives
        // the command title, diagnostics, revision, and any partial evidence.
        // `Err` is reserved for adapter/runtime failures such as an unknown
        // route or an unavailable executor.
        Ok(result)
    }
}

fn native_agent_budget() -> ExecutionBudget {
    ExecutionBudget {
        max_tool_calls: None,
        max_milliseconds: None,
        max_scene_operations: Some(512),
        max_scene_entities: Some(2_048),
        // Internal verification consumes the complete receipt. The model-facing
        // result is bounded separately by AgentToolResult::model_content.
        // A transport truncation must never become a failed postcondition.
        max_result_bytes: None,
        profile: Some("native-agent-balanced".to_string()),
    }
}

impl ToolExecutor for AgentToolExecutor<'_> {
    fn revision(&self) -> Option<u64> {
        Some(self.ledger.revision())
    }
    fn preview_for_review(
        &mut self,
        id: &str,
        name: &str,
        arguments: Value,
    ) -> Option<AgentToolResult> {
        let route = self.routes.get(name)?;
        let command = route.command_name.as_deref()?;
        // Gateway previews use the existing disposable-document contract.
        // Queued undo/redo and external provider jobs cannot be previewed here.
        if matches!(command, "undo" | "redo") {
            return None;
        }
        self.execute(id, name, arguments, ToolExecutionMode::Preview)
            .ok()
    }
    fn is_pending(&self, call_id: &str) -> bool {
        raf_ai::vision_bridge::is_review_pending(call_id)
            || self.asset_tools.is_pending(call_id)
            || self
                .electronics
                .as_deref()
                .is_some_and(|editor| editor.lab.pending_dc_tool(call_id))
    }

    fn poll_pending(&mut self, call_id: &str) -> Option<Result<AgentToolResult, String>> {
        if raf_ai::vision_bridge::is_review_pending(call_id) {
            let data = raf_ai::vision_bridge::poll_review(call_id)?;
            return Some(Ok(visual_review_tool_result(data, self.ledger.revision())));
        }
        if let Some(editor) = self
            .electronics
            .as_deref_mut()
            .filter(|editor| editor.lab.pending_dc_tool(call_id))
        {
            editor.poll_lab(false);
            let result = editor.lab.take_dc_tool(call_id)?;
            return Some(
                result.map(|run| laboratory_probe(&run, editor.lab_fingerprint(), &Value::Null)),
            );
        }
        self.asset_tools.poll(call_id, self.language)
    }

    fn kind(&self, name: &str) -> AgentToolKind {
        self.routes
            .get(name)
            .map(|route| route.kind)
            .unwrap_or(AgentToolKind::Mutation)
    }

    fn execute(
        &mut self,
        call_id: &str,
        name: &str,
        arguments: Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let route = self
            .routes
            .get(name)
            .cloned()
            .ok_or_else(|| {
                let mut available = self.routes.keys().cloned().collect::<Vec<_>>();
                available.sort();
                format!("Unknown Agent tool: {name}. Available in this turn: {}. Use an exact advertised name; unavailable tools were not executed.", available.join(", "))
            })?;
        if route.kind == AgentToolKind::Mutation && mode == ToolExecutionMode::Inspect {
            return Err(format!("Tool '{name}' is unavailable in Inspect mode."));
        }
        if matches!(
            name,
            "electronics_lab_simulate" | "electronics_lab_probe" | "electronics_lab_signals"
        ) {
            return self.execute_electronics_lab(call_id, name, &arguments, mode);
        }

        if matches!(
            name,
            "asset_generate_local_png"
                | "asset_generate_image"
                | "external_studio_install"
                | "external_studio_generate"
                | "particles_generate"
        ) {
            return self.execute_asset_generation(call_id, name, &arguments, mode);
        }

        if name == "world_brief" {
            return self.execute_world_brief(&arguments, mode);
        }

        if name == "capabilities_call" {
            return self.execute_capabilities_call(call_id, &arguments, mode);
        }

        if name == "viewport_capture" {
            return self.execute_viewport_capture(arguments);
        }

        if name.starts_with("performance_") {
            return self.execute_performance(name, &arguments);
        }

        if name == "scene_visual_review" {
            return self.execute_visual_review(call_id, &arguments);
        }

        if name == "scene_diff" {
            return Ok(self.execute_scene_diff(&arguments));
        }

        if let Some(mut result) = self.observation_context().execute(name, &arguments) {
            result.revision = Some(self.ledger.revision());
            return Ok(result);
        }
        if name == "capabilities_search" {
            return Ok(self.execute_capabilities_search(&arguments, mode));
        }
        let command_name = route
            .command_name
            .as_deref()
            .ok_or_else(|| format!("Agent tool '{name}' has no command route."))?;
        let params = match translate_arguments(name, command_name, arguments) {
            Ok(params) => params,
            Err(error) => {
                return Ok(AgentToolResult {
                    ok: false,
                    summary: format!("Invalid arguments for {name}."),
                    data: serde_json::json!({
                        "error": {
                            "code": "invalid_tool_arguments",
                            "message": error,
                            "suggestion": "Compare the failing path with the tool schema, remove extra array nesting, and retry only the failed operation."
                        },
                        "tool": name,
                    }),
                    references: Vec::new(),
                    changed: false,
                    revision: Some(self.ledger.revision()),
                    diff: None,
                    verification: Some(serde_json::json!({
                        "status": "failed",
                        "reason": "invalid_tool_arguments"
                    })),
                    warnings: Vec::new(),
                    details: Vec::new(),
                    preview: mode == ToolExecutionMode::Preview,
                    partial: false,
                    artifacts: Vec::new(),
                });
            }
        };
        self.execute_gateway(call_id, command_name, params, route.kind, mode)
    }

    fn describe(&self, name: &str) -> String {
        self.routes
            .get(name)
            .and_then(|route| route.command_name.as_deref())
            .and_then(|command| self.catalog.find(command))
            .map(|definition| {
                format!(
                    "{} (domain: {}, category: {})",
                    definition.name, definition.domain, definition.category
                )
            })
            .unwrap_or_else(|| format!("Native Agent tool {name}"))
    }
}

fn laboratory_probe(
    run: &crate::electronics_lab::DcRun,
    fingerprint: u64,
    args: &Value,
) -> AgentToolResult {
    let component = args.get("component").and_then(Value::as_str);
    let uuid = component.and_then(|target| uuid::Uuid::parse_str(target).ok());
    let offset = args
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(512) as usize;
    let mut page = run
        .values
        .iter()
        .filter(|value| {
            component.is_none_or(|target| {
                value.designator.eq_ignore_ascii_case(target) || uuid == Some(value.component)
            })
        })
        .skip(offset)
        .take(17)
        .collect::<Vec<_>>();
    let more = page.len() > 16;
    page.truncate(16);
    let stale = run.fingerprint != fingerprint;
    let mut result = AgentToolResult::success(
        if run.converged {
            "DC analysis converged."
        } else {
            "DC analysis did not converge."
        },
        serde_json::json!({
            "source":"simulated", "analysis":"dc", "run":run.id, "revision":format!("{:016x}",run.fingerprint),
            "stale":stale, "converged":run.converged, "kcl_residual_a":run.kcl_residual_a,
            "power_residual_w":run.power_residual_w, "warnings":run.messages, "values":page,
            "values_truncated":run.values_truncated, "next_offset":more.then_some(offset + 16),
        }),
    );
    result.ok = run.converged && !stale;
    result.references = page
        .iter()
        .map(|value| value.component.to_string())
        .collect();
    result.references.sort_unstable();
    result.references.dedup();
    result
}
