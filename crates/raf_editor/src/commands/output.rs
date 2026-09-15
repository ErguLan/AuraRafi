use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub level: CommandLevel,
    pub title: String,
    pub lines: Vec<String>,
    pub json: Value,
    pub changed: bool,
}

impl CommandOutput {
    pub fn info(title: impl Into<String>, lines: Vec<String>, json: Value) -> Self {
        Self {
            level: CommandLevel::Info,
            title: title.into(),
            lines,
            json,
            changed: false,
        }
    }

    pub fn changed(title: impl Into<String>, lines: Vec<String>, json: Value) -> Self {
        Self {
            level: CommandLevel::Info,
            title: title.into(),
            lines,
            json,
            changed: true,
        }
    }

    pub fn warning(title: impl Into<String>, lines: Vec<String>, json: Value) -> Self {
        Self {
            level: CommandLevel::Warning,
            title: title.into(),
            lines,
            json,
            changed: false,
        }
    }

    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        let message = message.into();
        let diagnostic = actionable_diagnostic(&message);
        Self {
            level: CommandLevel::Error,
            title: title.into(),
            lines: vec![message],
            json: serde_json::json!({
                "ok": false,
                "error": diagnostic
            }),
            changed: false,
        }
    }
}

fn actionable_diagnostic(message: &str) -> Value {
    let lower = message.to_ascii_lowercase();
    let (code, suggestion) = if lower.contains("expected [x, y, z]")
        || lower.contains("must be [x, y, z]")
        || lower.contains("must be an array or object")
    {
        (
            "invalid_vector_shape",
            "Use one flat vector such as [0, 1.5, -2]. Do not wrap it in another array.",
        )
    } else if lower.contains("is required") || lower.starts_with("missing ") {
        (
            "missing_required_field",
            "Add the named field using the tool schema, then retry the same operation.",
        )
    } else if lower.contains("not found") {
        (
            "target_not_found",
            "Inspect or query the scene and retry with a stable ref, UUID, path, or stable_key.",
        )
    } else if lower.contains("already used") || lower.contains("conflict") {
        (
            "conflict",
            "Refresh the scene state and retry with a unique key or the current revision.",
        )
    } else if lower.contains("unsupported") || lower.contains("unknown") {
        (
            "unsupported_value",
            "Use one of the values advertised by the command or tool schema.",
        )
    } else {
        (
            "command_failed",
            "Inspect the structured error and current project state before retrying.",
        )
    };
    let path = message
        .split_once(" must ")
        .or_else(|| message.split_once(" is required"))
        .or_else(|| message.split_once(" contains "))
        .map(|(path, _)| path.trim().trim_matches('`'))
        .filter(|path| !path.is_empty() && !path.contains(' '));
    serde_json::json!({
        "code": code,
        "message": message,
        "path": path,
        "suggestion": suggestion
    })
}
