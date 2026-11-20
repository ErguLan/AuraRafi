//! Parsed command execution for the Agent tool gateway.

use raf_core::scene::SceneGraph;

use crate::commands::game::{
    self, GameCommandContext, GameViewportPort, HeadlessGameViewportPort, SceneSelectionState,
};
use crate::commands::gateway::ParsedCommandExecutor;
use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::electronics_controller::NativeElectronicsEditor;
use serde_json::Value;

use super::scripting;
use super::{AgentEditorAction, AgentProjectContext};
pub(super) struct AgentParsedExecutor<'a> {
    pub(super) scene: &'a mut SceneGraph,
    pub(super) selection: &'a mut SceneSelectionState,
    pub(super) viewport: &'a mut dyn GameViewportPort,
    pub(super) electronics: Option<&'a mut NativeElectronicsEditor>,
    pub(super) project: Option<&'a AgentProjectContext>,
    pub(super) editor_actions: &'a mut Vec<AgentEditorAction>,
}

impl AgentParsedExecutor<'_> {
    fn execute_live(&mut self, command: &ParsedCommand) -> CommandOutput {
        let _project_root = crate::commands::game::bind_project_root(
            self.project.map(|project| project.root.clone()),
        );
        if crate::commands::ui::supports(&command.name) {
            let assets=self.project.map(|p|p.root.join("assets"));
            return crate::commands::ui::execute(command,assets.as_deref(),false);
        }
        if command.name.starts_with("lighting.") {
            return crate::commands::lighting::execute(command, self.scene);
        }
        if command.name.starts_with("game.") {
            let mut context = GameCommandContext {
                scene: self.scene,
                selection: self.selection,
                viewport: self.viewport as &mut dyn GameViewportPort,
            };
            game::execute(&command.name, command, &mut context)
        } else if command.name.starts_with("electronics.") || command.name.starts_with("pcb.") {
            self.electronics
                .as_deref_mut()
                .map(|editor| editor.execute_catalog_command(&command.name, command))
                .unwrap_or_else(|| {
                    CommandOutput::error(
                        "Agent command",
                        "The native Electronics document is not mounted for this session.",
                    )
                })
        } else if let Some(output) = scripting::execute(
            command,
            self.scene,
            self.project.map(|project| project.root.join("assets")).as_deref(),
        ) {
            output
        } else {
            match command.name.as_str() {
                "undo" => {
                    self.editor_actions.push(AgentEditorAction::Undo);
                    CommandOutput::info(
                        "Undo",
                        vec!["Undo queued.".to_string()],
                        serde_json::json!({"ok": true, "queued": true}),
                    )
                }
                "redo" => {
                    self.editor_actions.push(AgentEditorAction::Redo);
                    CommandOutput::info(
                        "Redo",
                        vec!["Redo queued.".to_string()],
                        serde_json::json!({"ok": true, "queued": true}),
                    )
                }
                _ => CommandOutput::error(
                    "Agent command",
                    format!("Command '{}' is not mounted.", command.name),
                ),
            }
        }
    }
}

impl ParsedCommandExecutor for AgentParsedExecutor<'_> {
    fn scene_entity_count(&self) -> Option<usize> {
        Some(self.scene.all_live_ids().len())
    }

    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        self.execute_live(command)
    }

    fn preview_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        let _project_root = crate::commands::game::bind_project_root(
            self.project.map(|project| project.root.clone()),
        );
        if crate::commands::ui::supports(&command.name) {
            let assets=self.project.map(|p|p.root.join("assets"));
            return crate::commands::ui::execute(command,assets.as_deref(),true);
        }
        if command.name.starts_with("lighting.") {
            return crate::commands::lighting::execute(command, &mut self.scene.clone());
        }
        if let Some(output) = scripting::preview(
            command,
            self.scene,
            self.project.map(|project| project.root.join("assets")).as_deref(),
        ) {
            return output;
        }
        if command.name.starts_with("world.brief") {
            let action = command
                .structured_arg("action")
                .and_then(Value::as_str)
                .unwrap_or_else(|| {
                    if command.name.ends_with(".write") {
                        "write"
                    } else {
                        "read"
                    }
                });
            if action.eq_ignore_ascii_case("write") || action.eq_ignore_ascii_case("set") {
                return CommandOutput::info(
                    "World brief preview",
                    vec!["No project file was written.".to_string()],
                    serde_json::json!({
                        "ok": true,
                        "preview": true,
                        "changed": false,
                        "action": "write",
                        "brief": command.structured_arg("brief").cloned().or_else(|| command.structured_arg("content").cloned()),
                    }),
                );
            }
        }
        if command.name == "game.commit" {
            let plan_id = command
                .structured_arg("plan_id")
                .and_then(Value::as_str)
                .or_else(|| command.arg("plan_id"))
                .unwrap_or_default();
            return CommandOutput::info(
                "World plan commit preview",
                vec![
                    "No scene state was changed.".to_string(),
                    format!("plan: {plan_id}"),
                ],
                serde_json::json!({
                    "ok": true,
                    "preview": true,
                    "changed": false,
                    "plan_id": plan_id,
                    "next": "Call scene_commit with Apply permission to execute the compiled plan.",
                }),
            );
        }
        if command.name.starts_with("game.") {
            let mut scene = self.scene.clone();
            let mut selection = self.selection.clone();
            let mut viewport = HeadlessGameViewportPort::default();
            viewport.set_selected_ids(selection.selected_nodes.clone());
            let mut context = GameCommandContext {
                scene: &mut scene,
                selection: &mut selection,
                viewport: &mut viewport,
            };
            game::execute(&command.name, command, &mut context)
        } else {
            CommandOutput::changed(
                format!("Preview {}", command.name),
                vec!["No project state was changed.".to_string()],
                serde_json::json!({
                    "command": command.name,
                    "params": command.args,
                    "preview": true
                }),
            )
        }
    }
}
