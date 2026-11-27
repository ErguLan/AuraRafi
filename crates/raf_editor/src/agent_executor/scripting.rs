//! Rhai script, behavior, and prefab authoring and validation routing.

use std::path::Path;

use raf_ai::agent_runtime::AgentToolKind;
use raf_ai::provider::AgentMode;
use raf_core::scene::SceneGraph;
use raf_core::Language;

use crate::commands::catalog::CommandCatalog;
use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::commands::script::{self, ScriptCommandContext};

use super::AgentToolPack;

pub(super) fn execute(
    command: &ParsedCommand,
    scene: &mut SceneGraph,
    assets_root: Option<&Path>,
) -> Option<CommandOutput> {
    if command.name.starts_with("behavior.") || command.name.starts_with("prefab.") {
        let mut context = ScriptCommandContext {
            scene,
            assets_root,
            editor_view: None,
        };
        Some(crate::commands::behavior::execute(
            command,
            &mut context,
            false,
        ))
    } else if command.name.starts_with("script.") {
        let mut context = ScriptCommandContext {
            scene,
            assets_root,
            editor_view: None,
        };
        Some(script::execute(&command.name, command, &mut context))
    } else {
        None
    }
}

pub(super) fn preview(
    command: &ParsedCommand,
    scene: &SceneGraph,
    assets_root: Option<&Path>,
) -> Option<CommandOutput> {
    if command.name.starts_with("behavior.") || command.name.starts_with("prefab.") {
        let mut scene = (*scene).clone();
        let mut context = ScriptCommandContext {
            scene: &mut scene,
            assets_root,
            editor_view: None,
        };
        return Some(crate::commands::behavior::execute(
            command,
            &mut context,
            true,
        ));
    }
    if command.name == "script.write" {
        return Some(crate::commands::script::preview_write(command));
    }
    None
}

pub(super) fn add_game_scripting_tools(
    pack: &mut AgentToolPack,
    catalog: &CommandCatalog,
    language: Language,
    mode: AgentMode,
) {
    for name in [
        "script.create",
        "script.write",
        "script.attach",
        "script.detach",
        "script.validate",
        "script.run",
        "behavior.attach",
        "behavior.detach",
        "behavior.parameter",
        "prefab.capture",
        "prefab.instantiate",
        "assembly.revert",
        "assembly.update",
        "assembly.make_unique",
    ] {
        if let Some(command) = catalog.find(name) {
            let kind = if matches!(name, "script.validate" | "script.run") {
                AgentToolKind::Read
            } else {
                AgentToolKind::Mutation
            };
            if kind == AgentToolKind::Read || mode != AgentMode::Inspect {
                pack.push_catalog(command, language, kind);
            }
        }
    }
}
