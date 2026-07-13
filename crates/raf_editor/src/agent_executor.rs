//! Tool executor that routes Agent tool calls to the engine command handlers.
//!
//! The commands documented in `docs/COMMANDS.md` are the Agent's tools. Each
//! tool call is converted to the slash-command string format, parsed, and
//! dispatched to the matching domain handler.

use raf_ai::agent_runtime::ToolExecutor;
use raf_ai::openai_client::OpenAiTool;
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_core::Language;
use serde_json::Value;
use std::collections::HashMap;

use crate::commands::{
    assets::{self, AssetCommandContext},
    catalog::{CommandCatalog, CommandDefinition},
    electronics::{self, ElectronicsCommandContext},
    game::{self, GameCommandContext},
    output::CommandOutput,
    parser::{parse_console_input, ParsedInput},
    script::{self, ScriptCommandContext},
    sessions::{self, SessionCommandContext, SessionCommandEvent},
    ui_document::{self, UiDocumentCommandContext},
    workspace,
};
use crate::panels::console::ConsolePanel;
use crate::panels::hierarchy::HierarchyPanel;
use crate::panels::pcb_view::PcbViewPanel;
use crate::panels::schematic_view::SchematicViewPanel;
use crate::panels::viewport::ViewportPanel;
use raf_ai::AssetImageGenerationQueue;
use raf_core::scene::SceneGraph;
use raf_core::session::ProjectSessionRegistry;
use raf_ui::UiDocument;

/// Editor-level actions are queued because tools execute while the app has
/// temporary mutable borrows into documents and panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEditorAction {
    Undo,
    Redo,
}

/// Sanitize a command name for use as an OpenAI tool name.
/// OpenAI tool names must match `^[a-zA-Z0-9_]+$` and cannot start with a digit.
pub fn sanitize_tool_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", sanitized)
    } else {
        sanitized
    }
}

/// Build a map from sanitized tool name -> original command name.
pub fn build_tool_name_map(catalog: &CommandCatalog) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for command in &catalog.commands {
        let sanitized = sanitize_tool_name(&command.name);
        map.insert(sanitized, command.name.clone());
        for alias in &command.aliases {
            let sanitized = sanitize_tool_name(alias);
            map.entry(sanitized).or_insert_with(|| command.name.clone());
        }
    }
    map
}

/// Executor context for one Agent step.
pub struct AgentToolExecutor<'a> {
    pub scene: &'a mut SceneGraph,
    pub hierarchy: &'a mut HierarchyPanel,
    pub viewport: &'a mut ViewportPanel,
    pub schematic_view: &'a mut SchematicViewPanel,
    pub pcb_view: &'a mut PcbViewPanel,
    pub project: Option<&'a Project>,
    pub catalog: &'a CommandCatalog,
    pub console: Option<&'a mut ConsolePanel>,
    pub image_queue: &'a mut AssetImageGenerationQueue,
    pub sessions: &'a mut ProjectSessionRegistry,
    pub session_events: &'a mut Vec<SessionCommandEvent>,
    pub editor_actions: &'a mut Vec<AgentEditorAction>,
    pub ui_document: &'a mut UiDocument,
    /// Map from sanitized tool name -> original command name.
    pub tool_name_map: &'a HashMap<String, String>,
}

impl<'a> AgentToolExecutor<'a> {
    /// Convert the command catalog into OpenAI tool definitions using sanitized names.
    pub fn build_tools(catalog: &CommandCatalog, language: Language) -> Vec<OpenAiTool> {
        catalog
            .commands
            .iter()
            .map(|command| {
                let sanitized = sanitize_tool_name(&command.name);
                command_to_openai_tool(&sanitized, command, language)
            })
            .collect()
    }
}

impl<'a> ToolExecutor for AgentToolExecutor<'a> {
    fn execute(&mut self, name: &str, arguments: Value) -> Result<String, String> {
        // Resolve sanitized tool name back to original command name.
        let original_name = self
            .tool_name_map
            .get(name)
            .map(|s| s.as_str())
            .unwrap_or(name);
        let definition = self
            .catalog
            .find(original_name)
            .ok_or_else(|| format!("Unknown tool: {}", name))?;

        if !domain_allowed(&definition.domain, self.project) {
            return Err(format!(
                "Command '{}' is not available for the active project type.",
                name
            ));
        }

        let command_line = build_command_line(name, &arguments);
        let parsed = match parse_console_input(&command_line) {
            Ok(ParsedInput::Command(command)) => command,
            Ok(ParsedInput::Message(_)) => {
                return Err("Tool arguments did not form a valid command.".to_string())
            }
            Err(error) => return Err(format!("Failed to parse command: {}", error)),
        };

        let output = match definition.domain.as_str() {
            "game" => {
                let mut ctx = GameCommandContext {
                    scene: self.scene,
                    hierarchy: self.hierarchy,
                    viewport: self.viewport,
                };
                game::execute(&definition.name, &parsed, &mut ctx)
            }
            "electronics" => {
                let mut ctx = ElectronicsCommandContext {
                    schematic_view: self.schematic_view,
                    pcb_view: self.pcb_view,
                };
                electronics::execute(&definition.name, &parsed, &mut ctx)
            }
            "shared" => dispatch_shared_command(&definition.name, &parsed, self),
            other => return Err(format!("Unsupported command domain: {}", other)),
        };

        if let Some(console) = self.console.as_mut() {
            console.log_command_output(output.clone());
        }

        Ok(command_output_to_string(&output))
    }

    fn describe(&self, name: &str) -> String {
        self.catalog
            .find(name)
            .map(|definition| {
                format!(
                    "{} (domain: {}, category: {})",
                    definition.name, definition.domain, definition.category
                )
            })
            .unwrap_or_else(|| format!("Execute {}", name))
    }
}

fn dispatch_shared_command(
    name: &str,
    parsed: &crate::commands::parser::ParsedCommand,
    ctx: &mut AgentToolExecutor<'_>,
) -> CommandOutput {
    match name {
        "help" | "commands" | "describe" | "history" | "clear" | "undo" | "redo"
        | "project.info" => {
            // Workspace / meta commands.
            workspace_or_meta_command(name, parsed, ctx)
        }
        "workspace.read" | "workspace.search" => workspace_command(name, parsed, ctx),
        "asset.generate_image"
        | "asset.generate_local_png"
        | "asset.image_status"
        | "asset.cancel_image" => {
            let project_root = ctx.project.map(|project| project.path.as_path());
            let mut asset_ctx = AssetCommandContext {
                project_root,
                image_queue: ctx.image_queue,
            };
            assets::execute(name, parsed, &mut asset_ctx)
        }
        "session.list" | "session.create" | "session.open" | "session.duplicate"
        | "session.remove" => {
            let mut session_ctx = SessionCommandContext {
                project: ctx.project,
                registry: ctx.sessions,
                events: ctx.session_events,
            };
            sessions::execute(name, parsed, &mut session_ctx)
        }
        "ui.document.describe"
        | "ui.node.add"
        | "ui.node.remove"
        | "ui.document.set_space"
        | "ui.document.bind_camera"
        | "ui.document.clear_camera" => {
            let mut document_ctx = UiDocumentCommandContext {
                document: ctx.ui_document,
            };
            ui_document::execute(name, parsed, &mut document_ctx)
        }
        "script.create"
        | "script.attach"
        | "script.detach"
        | "script.list"
        | "script.validate"
        | "script.run"
        | "script.compile_nodes" => {
            let assets_path = ctx.project.map(|project| project.path.join("assets"));
            let assets_root = assets_path.as_deref();
            let mut script_ctx = ScriptCommandContext {
                scene: ctx.scene,
                assets_root,
            };
            script::execute(name, parsed, &mut script_ctx)
        }
        _ => CommandOutput::error("Shared command", format!("Not routed: {}", name)),
    }
}

fn workspace_command(
    name: &str,
    parsed: &crate::commands::parser::ParsedCommand,
    ctx: &AgentToolExecutor<'_>,
) -> CommandOutput {
    let Some(project) = ctx.project else {
        return CommandOutput::error("Workspace command", "No active project.");
    };
    match name {
        "workspace.read" => workspace::read_file(parsed, &project.path),
        "workspace.search" => workspace::search(parsed, &project.path),
        _ => CommandOutput::error("Workspace command", format!("Unknown: {}", name)),
    }
}

fn workspace_or_meta_command(
    name: &str,
    _parsed: &crate::commands::parser::ParsedCommand,
    ctx: &mut AgentToolExecutor<'_>,
) -> CommandOutput {
    match name {
        "undo" => {
            ctx.editor_actions.push(AgentEditorAction::Undo);
            CommandOutput::info(
                "Undo",
                vec!["Undo queued for the editor history.".to_string()],
                serde_json::json!({"ok": true, "status": "queued"}),
            )
        }
        "redo" => {
            ctx.editor_actions.push(AgentEditorAction::Redo);
            CommandOutput::info(
                "Redo",
                vec!["Redo queued for the editor history.".to_string()],
                serde_json::json!({"ok": true, "status": "queued"}),
            )
        }
        "project.info" => {
            let lines = if let Some(project) = ctx.project {
                vec![
                    format!("name: {}", project.name),
                    format!("type: {:?}", project.project_type),
                    format!("path: {}", project.path.display()),
                ]
            } else {
                vec!["No active project.".to_string()]
            };
            CommandOutput::info("Project info", lines, serde_json::json!({"ok": true}))
        }
        "help" | "commands" | "describe" | "history" | "clear" => CommandOutput::info(
            "Meta command",
            vec![format!("{} is available in the Console panel.", name)],
            serde_json::json!({"ok": true, "command": name}),
        ),
        _ => CommandOutput::error("Meta command", format!("Unknown: {}", name)),
    }
}

fn domain_allowed(domain: &str, project: Option<&Project>) -> bool {
    match domain {
        "shared" => true,
        "game" => project.map(|p| p.project_type) == Some(ProjectType::Game),
        "electronics" => project.map(|p| p.project_type) == Some(ProjectType::Electronics),
        _ => false,
    }
}

fn command_to_openai_tool(
    sanitized_name: &str,
    command: &CommandDefinition,
    language: Language,
) -> OpenAiTool {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for parameter in &command.parameters {
        let mut schema = serde_json::Map::new();
        schema.insert("type".to_string(), json_type_for_kind(&parameter.kind));
        let description = parameter
            .description_key
            .as_deref()
            .map(|key| t(key, language))
            .unwrap_or_else(|| format!("{} ({})", parameter.name, parameter.kind));
        schema.insert("description".to_string(), Value::String(description));
        if let Some(default) = &parameter.default {
            schema.insert("default".to_string(), Value::String(default.clone()));
        }
        properties.insert(parameter.name.clone(), Value::Object(schema));
        if parameter.required {
            required.push(Value::String(parameter.name.clone()));
        }
    }

    OpenAiTool {
        tool_type: "function".to_string(),
        function: raf_ai::openai_client::OpenAiFunction {
            name: sanitized_name.to_string(),
            description: Some(localized_tool_description(command, language)),
            parameters: Value::Object({
                let mut map = serde_json::Map::new();
                map.insert("type".to_string(), Value::String("object".to_string()));
                map.insert("properties".to_string(), Value::Object(properties));
                map.insert("required".to_string(), Value::Array(required));
                map
            }),
        },
    }
}

fn localized_tool_description(command: &CommandDefinition, language: Language) -> String {
    let mut description = t(&command.description_key, language);
    if let Some(example) = command.examples.first() {
        description.push_str(" Example: ");
        description.push_str(example);
    }
    description
}

fn json_type_for_kind(kind: &str) -> Value {
    Value::String(
        match kind {
            "usize" | "i32" | "i64" | "f32" | "f64" => "number",
            "bool" => "boolean",
            "array" => "array",
            "object" => "object",
            _ => "string",
        }
        .to_string(),
    )
}

fn build_command_line(name: &str, arguments: &Value) -> String {
    let mut parts = vec![format!("/{}", name)];
    if let Some(object) = arguments.as_object() {
        for (key, value) in object {
            if key == "command" && value.is_string() {
                // Special case for /describe command=<name>.
                parts.push(format!("{}={}", key, value.as_str().unwrap_or("")));
            } else if let Some(text) = value.as_str() {
                if text.contains(' ') {
                    parts.push(format!("{}=\"{}\"", key, text));
                } else {
                    parts.push(format!("{}={}", key, text));
                }
            } else {
                parts.push(format!("{}={}", key, value));
            }
        }
    }
    parts.join(" ")
}

fn command_output_to_string(output: &CommandOutput) -> String {
    let mut lines = vec![format!("title: {}", output.title)];
    lines.extend(output.lines.iter().cloned());
    lines.push(format!("changed: {}", output.changed));
    lines.push(format!("json: {}", output.json));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_contract_uses_localized_descriptions_and_examples() {
        let catalog = CommandCatalog::builtin();
        let tools = AgentToolExecutor::build_tools(&catalog, Language::Spanish);
        let tool = tools
            .iter()
            .find(|tool| tool.function.name == "game_add")
            .expect("game.add tool");

        let description = tool.function.description.as_deref().unwrap_or_default();
        assert!(description.contains("Crea"));
        assert!(description.contains("/game.add"));

        let primitive_description = tool
            .function
            .parameters
            .get("properties")
            .and_then(|properties| properties.get("primitive"))
            .and_then(|primitive| primitive.get("description"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert_ne!(primitive_description, "commands.param.primitive");
        assert!(!primitive_description.is_empty());
    }
}
