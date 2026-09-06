//! UI-independent command gateway.
//!
//! Console, CLI, MCP and RafUI should all stop at this boundary. A concrete
//! executor owns project/domain state; this module owns only protocol mapping
//! and never creates a window or a presentation console.

use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse};
use raf_core::{TransactionId, TransactionLedger};
use serde_json::Value;
use std::time::Instant;

use super::output::{CommandLevel, CommandOutput};
use super::parser::ParsedCommand;

pub trait ParsedCommandExecutor {
    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput;

    /// Current live scene count for hosts that can preflight a construction
    /// budget. Non-scene domains keep the default and are not constrained by
    /// the scene-entity limit.
    fn scene_entity_count(&self) -> Option<usize> {
        None
    }

    /// Execute against disposable state. Implementors that cannot guarantee a
    /// non-mutating preview must keep the explicit error instead of applying.
    fn preview_parsed(&mut self, _command: &ParsedCommand) -> CommandOutput {
        CommandOutput::error(
            "Preview unavailable",
            "This command adapter does not provide a non-mutating preview.",
        )
    }
}

pub struct CommandGateway<E> {
    pub executor: E,
    pub ledger: TransactionLedger,
}

impl<E> CommandGateway<E> {
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            ledger: TransactionLedger::new(),
        }
    }

    pub fn into_inner(self) -> E {
        self.executor
    }

    pub fn revision(&self) -> raf_core::Revision {
        self.ledger.revision()
    }
}

impl<E: ParsedCommandExecutor> CommandEndpoint for CommandGateway<E> {
    fn execute(&mut self, request: EngineCommandRequest) -> EngineCommandResponse {
        let id = request.id;
        if let Err(error) = request.validate() {
            let mut response = EngineCommandResponse::error(id, "Invalid command", error);
            response.revision = self.ledger.revision();
            return response;
        }
        if let Err(error) = self.ledger.check_expected(request.expected_revision) {
            let mut response = EngineCommandResponse::error(id, "Revision conflict", error);
            response.revision = self.ledger.revision();
            return response;
        }
        if let Err(error) = validate_execution_budget(&request, self.executor.scene_entity_count())
        {
            let mut response = EngineCommandResponse::error(id, "Execution budget exceeded", error);
            response.revision = self.ledger.revision();
            return response;
        }
        if !request.dry_run {
            if let Some(existing) = self
                .ledger
                .find_idempotency_key(request.idempotency_key.as_deref())
            {
                let mut response = EngineCommandResponse::error(
                    id,
                    "Idempotent replay",
                    "The idempotency key was already applied; the editor executor was not called again.",
                );
                response.ok = true;
                response.changed = existing.changed;
                response.data = serde_json::json!({"replayed": true});
                response.diff = existing.diff.clone();
                response.revision = self.ledger.revision();
                response.transaction_id = Some(existing.id);
                response.undo_token = existing.undo_token;
                response.undo_available = existing.undo_token.is_some();
                return response;
            }
        }
        let structured_args = request.params.clone();
        let args = match request.params {
            Value::Null => std::collections::BTreeMap::new(),
            Value::Object(object) => object
                .into_iter()
                .map(|(key, value)| (key, json_arg(value)))
                .collect(),
            _ => {
                let mut response = EngineCommandResponse::error(
                    id,
                    "Command request",
                    "params must be a JSON object.",
                );
                response.revision = self.ledger.revision();
                return response;
            }
        };
        let command = ParsedCommand {
            raw: format!("/{}", request.name),
            name: request.name.trim_start_matches('/').to_ascii_lowercase(),
            args,
            positional: Vec::new(),
            structured_args: Some(structured_args),
        };
        if request.dry_run {
            let started = Instant::now();
            let output = self.executor.preview_parsed(&command);
            let would_change = output.changed;
            let mut response = response_from_output(id, output);
            response.changed = false;
            response.undo_available = false;
            response.revision = self.ledger.revision();
            response.diff = would_change.then(|| {
                serde_json::json!({
                    "preview": true,
                    "would_change": true,
                    "command": command.name
                })
            });
            response.data = serde_json::json!({
                "preview": true,
                "would_change": would_change,
                "result": response.data
            });
            apply_result_budget(&mut response, request.budget.as_ref(), started.elapsed());
            return response;
        }
        let started = Instant::now();
        let output = self.executor.execute_parsed(&command);
        let changed = output.changed;
        let mut response = response_from_output(id, output);
        if response.ok {
            let record = self.ledger.record_without_undo(
                request.transaction_id.unwrap_or_else(TransactionId::new),
                changed,
                response.diff.clone(),
                request.idempotency_key,
            );
            response.revision = self.ledger.revision();
            response.transaction_id = Some(record.id);
            response.undo_token = record.undo_token;
            response.undo_available = record.undo_token.is_some();
        } else {
            // Failed commands must not consume an idempotency key. Otherwise
            // a retry would be reported as a successful replay and the Agent
            // could stop repairing after the first validation error.
            response.revision = self.ledger.revision();
        }
        apply_result_budget(&mut response, request.budget.as_ref(), started.elapsed());
        response
    }
}

fn validate_execution_budget(
    request: &EngineCommandRequest,
    current_scene_entities: Option<usize>,
) -> Result<(), String> {
    let Some(budget) = request.budget.as_ref() else {
        return Ok(());
    };
    let name = request.name.trim_start_matches('/').to_ascii_lowercase();
    if let Some(max_operations) = budget.max_scene_operations {
        let operations = match name.as_str() {
            "game.batch" | "scene.batch" | "game.repair" | "scene.repair" => request
                .params
                .get("operations")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            "game.build" | "scene.build" | "game.reconcile" | "scene.reconcile" => request
                .params
                .get("groups")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
                .saturating_add(
                    request
                        .params
                        .get("entities")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len),
                ),
            "game.add"
            | "scene.add"
            | "game.create"
            | "scene.create"
            | "game.create_group"
            | "scene.create_group"
            | "game.update"
            | "scene.update"
            | "game.reparent"
            | "scene.reparent"
            | "game.delete"
            | "scene.delete"
            | "game.duplicate"
            | "scene.duplicate"
            | "game.arrange_grid"
            | "scene.arrange"
            | "game.generate_prefab"
            | "scene.instantiate_prefab" => 1,
            _ => 0,
        };
        if operations > max_operations as usize {
            return Err(format!(
                "{name} requests {operations} scene operations, but the budget allows {max_operations}. Use a smaller batch or raise max_scene_operations."
            ));
        }
    }
    if let (Some(max_entities), Some(current)) = (budget.max_scene_entities, current_scene_entities)
    {
        let requested_entities = requested_scene_entities(&name, &request.params);
        let projected = current.saturating_add(requested_entities);
        if projected > max_entities as usize {
            return Err(format!(
                "{name} may reach {projected} live scene entities, but the budget allows {max_entities}. Use a smaller build or raise max_scene_entities."
            ));
        }
    }
    Ok(())
}

fn requested_scene_entities(name: &str, params: &Value) -> usize {
    match name {
        "game.add"
        | "scene.add"
        | "game.create"
        | "scene.create"
        | "game.create_group"
        | "scene.create_group"
        | "game.generate_prefab"
        | "scene.instantiate_prefab" => 1,
        "game.build" | "scene.build" | "game.reconcile" | "scene.reconcile" => params
            .get("groups")
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
            .saturating_add(
                params
                    .get("entities")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            ),
        "game.batch" | "scene.batch" | "game.repair" | "scene.repair" => params
            .get("operations")
            .and_then(Value::as_array)
            .map(|operations| {
                operations
                    .iter()
                    .filter(|operation| {
                        matches!(
                            operation
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            "scene_create"
                                | "scene_create_group"
                                | "scene_duplicate"
                                | "scene_instantiate_prefab"
                        )
                    })
                    .count()
            })
            .unwrap_or(0),
        _ => 0,
    }
}

fn apply_result_budget(
    response: &mut EngineCommandResponse,
    budget: Option<&raf_core::ExecutionBudget>,
    elapsed: std::time::Duration,
) {
    if let Some(budget) = budget {
        if let Some(max_milliseconds) = budget.max_milliseconds {
            let elapsed_millis = elapsed.as_millis() as u64;
            if elapsed_millis > max_milliseconds {
                response.warnings.push(format!(
                    "Command exceeded its time budget ({elapsed_millis} ms > {max_milliseconds} ms)."
                ));
            }
        }
        if let Some(max_result_bytes) = budget.max_result_bytes {
            let size = serde_json::to_vec(&response.data)
                .map(|encoded| encoded.len())
                .unwrap_or(usize::MAX);
            if size > max_result_bytes as usize {
                let compact = raf_core::agent_context::compact_result_data(&response.data);
                let compact_size = serde_json::to_vec(&compact)
                    .map(|encoded| encoded.len())
                    .unwrap_or(usize::MAX);
                response.data = if compact_size <= max_result_bytes as usize {
                    compact
                } else {
                    serde_json::json!({
                        "truncated": true,
                        "result_bytes": size,
                        "max_result_bytes": max_result_bytes,
                    })
                };
                response.warnings.push(format!(
                    "Result compacted to respect the {max_result_bytes}-byte result budget."
                ));
            }
        }
    }
}

pub fn response_from_output(
    id: raf_core::CommandId,
    output: CommandOutput,
) -> EngineCommandResponse {
    let ok = !matches!(output.level, CommandLevel::Error);
    let warnings = if matches!(output.level, CommandLevel::Warning) {
        output.lines.clone()
    } else {
        Vec::new()
    };
    EngineCommandResponse {
        protocol: raf_core::COMMAND_PROTOCOL_VERSION,
        id,
        ok,
        changed: output.changed,
        title: output.title,
        lines: output.lines,
        data: output.json,
        warnings,
        diff: None,
        undo_available: output.changed,
        revision: 0,
        transaction_id: None,
        undo_token: None,
        artifacts: Vec::new(),
        metrics: Value::Null,
        verification: None,
    }
}

fn json_arg(value: Value) -> String {
    match value {
        Value::String(value) => value,
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

/// Marker used by host code to state that the endpoint is available even when
/// no RafUI surface or console is mounted. It is intentionally empty: the
/// domain executor remains the only place allowed to mutate project state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadlessCommandMode {
    pub source: CommandSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recording;

    impl ParsedCommandExecutor for Recording {
        fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
            CommandOutput::info(
                "recorded",
                vec![command.name.clone()],
                serde_json::json!({"command": command.name}),
            )
        }
    }

    struct PreviewRecording {
        executed: bool,
    }

    struct StructuredRecording {
        transform: Option<Value>,
    }

    struct SceneCountingRecording {
        executed: bool,
    }

    impl ParsedCommandExecutor for SceneCountingRecording {
        fn scene_entity_count(&self) -> Option<usize> {
            Some(3)
        }

        fn execute_parsed(&mut self, _command: &ParsedCommand) -> CommandOutput {
            self.executed = true;
            CommandOutput::changed("created", Vec::new(), serde_json::json!({"ok": true}))
        }
    }

    impl ParsedCommandExecutor for StructuredRecording {
        fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
            self.transform = command.structured_arg("transform").cloned();
            CommandOutput::info("recorded", Vec::new(), serde_json::json!({"ok": true}))
        }
    }

    impl ParsedCommandExecutor for PreviewRecording {
        fn execute_parsed(&mut self, _command: &ParsedCommand) -> CommandOutput {
            self.executed = true;
            CommandOutput::changed(
                "live",
                vec!["should not run".to_string()],
                serde_json::json!({}),
            )
        }

        fn preview_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
            CommandOutput::changed(
                "preview",
                vec![format!("would run {}", command.name)],
                serde_json::json!({"preview": true}),
            )
        }
    }

    #[test]
    fn gateway_executes_without_a_ui_surface() {
        let mut gateway = CommandGateway::new(Recording);
        let response = gateway.execute(EngineCommandRequest::new(
            "game.describe_scene",
            serde_json::json!({}),
            CommandSource::Mcp,
        ));
        assert!(response.ok);
        assert_eq!(response.lines, vec!["game.describe_scene"]);
    }

    #[test]
    fn gateway_does_not_execute_the_same_idempotency_key_twice() {
        let mut gateway = CommandGateway::new(Recording);
        let mut first = EngineCommandRequest::new(
            "game.describe_scene",
            serde_json::json!({}),
            CommandSource::Cli,
        );
        first.idempotency_key = Some("stable-request".to_string());
        let first_response = gateway.execute(first);
        assert!(first_response.ok);

        let mut replay = EngineCommandRequest::new(
            "game.describe_scene",
            serde_json::json!({}),
            CommandSource::Cli,
        );
        replay.idempotency_key = Some("stable-request".to_string());
        let replay_response = gateway.execute(replay);
        assert!(replay_response.ok);
        assert_eq!(replay_response.data, serde_json::json!({"replayed": true}));
        assert_eq!(gateway.ledger.records().len(), 1);
    }

    #[test]
    fn dry_run_uses_preview_without_changing_revision_or_calling_live_executor() {
        let mut gateway = CommandGateway::new(PreviewRecording { executed: false });
        let mut request = EngineCommandRequest::new(
            "game.update",
            serde_json::json!({"target": "Shelf", "x": 2}),
            CommandSource::Agent,
        );
        request.dry_run = true;
        let response = gateway.execute(request);

        assert!(response.ok);
        assert!(!response.changed);
        assert_eq!(response.revision, 0);
        assert_eq!(response.data["preview"], serde_json::json!(true));
        assert!(!gateway.executor.executed);
    }

    #[test]
    fn gateway_preserves_nested_protocol_values_for_domain_handlers() {
        let mut gateway = CommandGateway::new(StructuredRecording { transform: None });
        let response = gateway.execute(EngineCommandRequest::new(
            "game.update",
            serde_json::json!({
                "target": "Shelf",
                "transform": {
                    "position": [1, 2, 3],
                    "scale": [2, 1, 1]
                }
            }),
            CommandSource::Agent,
        ));

        assert!(response.ok);
        assert_eq!(
            gateway.executor.transform,
            Some(serde_json::json!({
                "position": [1, 2, 3],
                "scale": [2, 1, 1]
            }))
        );
    }

    #[test]
    fn gateway_rejects_scene_budget_before_mutation() {
        let mut gateway = CommandGateway::new(SceneCountingRecording { executed: false });
        let mut request = EngineCommandRequest::new(
            "game.add",
            serde_json::json!({"primitive": "cube", "name": "Shelf"}),
            CommandSource::Agent,
        );
        request.budget = Some(raf_core::ExecutionBudget {
            max_tool_calls: None,
            max_milliseconds: None,
            max_scene_operations: None,
            max_scene_entities: Some(3),
            max_result_bytes: None,
            profile: Some("test".to_string()),
        });

        let response = gateway.execute(request);

        assert!(!response.ok);
        assert_eq!(response.title, "Execution budget exceeded");
        assert!(!gateway.executor.executed);
        assert_eq!(gateway.revision(), 0);
    }

    struct FailingRecording;

    impl ParsedCommandExecutor for FailingRecording {
        fn execute_parsed(&mut self, _command: &ParsedCommand) -> CommandOutput {
            CommandOutput::error("validation", "invalid scene target")
        }
    }

    #[test]
    fn failed_commands_do_not_poison_idempotency_retries() {
        let mut gateway = CommandGateway::new(FailingRecording);
        let mut request = EngineCommandRequest::new(
            "game.update",
            serde_json::json!({"target": "missing"}),
            CommandSource::Agent,
        );
        request.idempotency_key = Some("repair-attempt".to_string());

        let first = gateway.execute(request.clone());
        assert!(!first.ok);
        assert!(gateway
            .ledger
            .find_idempotency_key(Some("repair-attempt"))
            .is_none());

        let second = gateway.execute(request);
        assert!(!second.ok);
        assert_eq!(second.title, "validation");
        assert_eq!(gateway.ledger.records().len(), 0);
    }
}
