//! UI-independent command gateway.
//!
//! Console, CLI, MCP and RafUI should all stop at this boundary. A concrete
//! executor owns project/domain state; this module owns only protocol mapping
//! and never creates a window or a presentation console.

use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse};
use raf_core::{TransactionId, TransactionLedger};
use serde_json::Value;

use super::output::{CommandLevel, CommandOutput};
use super::parser::ParsedCommand;

pub trait ParsedCommandExecutor {
    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput;
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
        if let Err(error) = self.ledger.check_expected(request.expected_revision) {
            let mut response = EngineCommandResponse::error(id, "Revision conflict", error);
            response.revision = self.ledger.revision();
            return response;
        }
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
        if request.dry_run {
            return EngineCommandResponse::error(
                id,
                "Preview unavailable",
                "This editor adapter does not yet provide non-mutating previews for domain commands.",
            );
        }
        let args = match request.params {
            Value::Null => std::collections::BTreeMap::new(),
            Value::Object(object) => object
                .into_iter()
                .map(|(key, value)| (key, json_arg(value)))
                .collect(),
            _ => {
                return EngineCommandResponse::error(
                    id,
                    "Command request",
                    "params must be a JSON object.",
                )
            }
        };
        let command = ParsedCommand {
            raw: format!("/{}", request.name),
            name: request.name.trim_start_matches('/').to_ascii_lowercase(),
            args,
            positional: Vec::new(),
        };
        let output = self.executor.execute_parsed(&command);
        let changed = output.changed;
        let mut response = response_from_output(id, output);
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
        response
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
}
