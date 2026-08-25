//! UI-independent Agent command adapter for the native editor.
//!
//! The Agent talks to the same command catalog and game command kernel as CLI,
//! MCP and the retained editor. It never receives RafUI or Winit types.

use std::collections::HashMap;
use std::path::PathBuf;

use raf_ai::agent_runtime::ToolExecutor;
use raf_ai::openai_client::{OpenAiFunction, OpenAiTool};
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::Language;
use serde_json::Value;

use crate::commands::{
    catalog::{CommandCatalog, CommandDefinition},
    game::{self, GameCommandContext, GameViewportPort, SceneSelectionState},
    output::CommandOutput,
    parser::{parse_console_input, ParsedInput},
    script::{self, ScriptCommandContext},
    workspace,
};
use crate::electronics_controller::NativeElectronicsEditor;
use crate::panels::viewport_controller::NativeGameViewportController;

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

pub fn sanitize_tool_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if sanitized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

pub fn build_tool_name_map(catalog: &CommandCatalog) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for command in catalog
        .commands
        .iter()
        .filter(|command| tool_supported(command))
    {
        map.insert(sanitize_tool_name(&command.name), command.name.clone());
        for alias in &command.aliases {
            map.entry(sanitize_tool_name(alias))
                .or_insert_with(|| command.name.clone());
        }
    }
    map
}

pub struct AgentToolExecutor<'a> {
    pub scene: &'a mut SceneGraph,
    pub selection: &'a mut SceneSelectionState,
    pub viewport: &'a mut NativeGameViewportController,
    pub electronics: Option<&'a mut NativeElectronicsEditor>,
    pub project: Option<AgentProjectContext>,
    pub catalog: &'a CommandCatalog,
    pub tool_name_map: HashMap<String, String>,
    pub editor_actions: &'a mut Vec<AgentEditorAction>,
}

impl AgentToolExecutor<'_> {
    pub fn build_tools(catalog: &CommandCatalog, language: Language) -> Vec<OpenAiTool> {
        catalog
            .commands
            .iter()
            .filter(|command| tool_supported(command))
            .map(|command| {
                command_to_openai_tool(&sanitize_tool_name(&command.name), command, language)
            })
            .collect()
    }

    fn execute_shared(
        &mut self,
        command_name: &str,
        command: &crate::commands::parser::ParsedCommand,
    ) -> CommandOutput {
        match command_name {
            "workspace.read" => self
                .project
                .as_ref()
                .map(|project| workspace::read_file(command, &project.root))
                .unwrap_or_else(|| CommandOutput::error("Workspace read", "No active project.")),
            "workspace.search" => self
                .project
                .as_ref()
                .map(|project| workspace::search(command, &project.root))
                .unwrap_or_else(|| CommandOutput::error("Workspace search", "No active project.")),
            name if name.starts_with("script.") => {
                let assets_root = self
                    .project
                    .as_ref()
                    .map(|project| project.root.join("assets"));
                let mut context = ScriptCommandContext {
                    scene: self.scene,
                    assets_root: assets_root.as_deref(),
                };
                script::execute(name, command, &mut context)
            }
            "project.info" => {
                let lines = self
                    .project
                    .as_ref()
                    .map(|project| {
                        vec![
                            format!("project_type: {:?}", project.project_type),
                            format!("path: {}", project.root.display()),
                        ]
                    })
                    .unwrap_or_else(|| vec!["No active project.".to_string()]);
                CommandOutput::info("Project info", lines, serde_json::json!({"ok": true}))
            }
            "help" | "commands" | "describe" | "history" | "clear" => CommandOutput::info(
                "Agent command",
                vec![format!(
                    "{command_name} is available through the command catalog."
                )],
                serde_json::json!({"ok": true, "command": command_name}),
            ),
            "undo" => {
                self.editor_actions.push(AgentEditorAction::Undo);
                CommandOutput::info(
                    "Undo",
                    vec!["Undo queued for the active editor history.".to_string()],
                    serde_json::json!({"ok": true, "queued": true}),
                )
            }
            "redo" => {
                self.editor_actions.push(AgentEditorAction::Redo);
                CommandOutput::info(
                    "Redo",
                    vec!["Redo queued for the active editor history.".to_string()],
                    serde_json::json!({"ok": true, "queued": true}),
                )
            }
            _ => CommandOutput::error(
                "Agent command",
                format!("Shared command '{command_name}' is not mounted."),
            ),
        }
    }
}

impl ToolExecutor for AgentToolExecutor<'_> {
    fn execute(&mut self, name: &str, arguments: Value) -> Result<String, String> {
        let original_name = self
            .tool_name_map
            .get(name)
            .map(String::as_str)
            .unwrap_or(name);
        let definition = self
            .catalog
            .find(original_name)
            .ok_or_else(|| format!("Unknown Agent tool: {name}"))?;
        if !tool_supported(definition) {
            return Err(format!(
                "Tool '{}' is not available in this editor build.",
                definition.name
            ));
        }
        if !domain_allowed(&definition.domain, self.project.as_ref()) {
            return Err(format!(
                "Command '{}' is not available for the active project type.",
                definition.name
            ));
        }
        let command_line = build_command_line(&definition.name, &arguments);
        let parsed = match parse_console_input(&command_line) {
            Ok(ParsedInput::Command(command)) => command,
            Ok(ParsedInput::Message(_)) => {
                return Err("Tool arguments did not form a command.".to_string())
            }
            Err(error) => return Err(format!("Failed to parse Agent command: {error}")),
        };
        let output = match definition.domain.as_str() {
            "game" => {
                let mut context = GameCommandContext {
                    scene: self.scene,
                    selection: self.selection,
                    viewport: self.viewport as &mut dyn GameViewportPort,
                };
                game::execute(&definition.name, &parsed, &mut context)
            }
            "shared" => self.execute_shared(&definition.name, &parsed),
            "electronics" => self
                .electronics
                .as_deref_mut()
                .map(|editor| editor.execute_catalog_command(&definition.name, &parsed))
                .unwrap_or_else(|| {
                    CommandOutput::error(
                        "Agent command",
                        "The native Electronics document is not mounted for this session.",
                    )
                }),
            other => CommandOutput::error(
                "Agent command",
                format!("No current editor adapter exists for domain '{other}'."),
            ),
        };
        if output.level == crate::commands::output::CommandLevel::Error {
            Err(command_output_to_string(&output))
        } else {
            Ok(command_output_to_string(&output))
        }
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
            .unwrap_or_else(|| format!("Execute {name}"))
    }
}

fn tool_supported(command: &CommandDefinition) -> bool {
    match command.domain.as_str() {
        "game" => true,
        "electronics" => true,
        "shared" => matches!(
            command.name.as_str(),
            "help"
                | "commands"
                | "describe"
                | "history"
                | "clear"
                | "undo"
                | "redo"
                | "project.info"
                | "workspace.read"
                | "workspace.search"
                | "script.create"
                | "script.attach"
                | "script.detach"
                | "script.list"
                | "script.validate"
                | "script.run"
                | "script.compile_nodes"
        ),
        _ => false,
    }
}

fn domain_allowed(domain: &str, project: Option<&AgentProjectContext>) -> bool {
    match domain {
        "shared" => true,
        "game" => project.is_some_and(|project| project.project_type == ProjectType::Game),
        "electronics" => {
            project.is_some_and(|project| project.project_type == ProjectType::Electronics)
        }
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
        function: OpenAiFunction {
            name: sanitized_name.to_string(),
            description: Some(localized_tool_description(command, language)),
            parameters: serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required,
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
    let mut parts = vec![format!("/{name}")];
    if let Some(object) = arguments.as_object() {
        for (key, value) in object {
            if let Some(text) = value.as_str() {
                let escaped = text.replace('"', "\\\"");
                if text.contains(char::is_whitespace) {
                    parts.push(format!(r#"{key}=\"{escaped}\""#));
                } else {
                    parts.push(format!("{key}={escaped}"));
                }
            } else {
                parts.push(format!("{key}={value}"));
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
