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
        Self {
            level: CommandLevel::Error,
            title: title.into(),
            lines: vec![message.into()],
            json: serde_json::json!({
                "ok": false
            }),
            changed: false,
        }
    }
}
