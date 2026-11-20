//! Shared fixtures for game command tests.

use serde_json::Value;

use super::query::json_command_arg;
use crate::commands::parser::ParsedCommand;

pub(super) fn parsed_command(name: &str, params: Value) -> ParsedCommand {
    ParsedCommand {
        raw: format!("/{name}"),
        name: name.to_string(),
        args: params
            .as_object()
            .expect("test params object")
            .iter()
            .map(|(key, value)| (key.clone(), json_command_arg(value.clone())))
            .collect(),
        positional: Vec::new(),
        structured_args: Some(params),
    }
}
