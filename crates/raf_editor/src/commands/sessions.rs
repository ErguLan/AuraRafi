//! Session registry commands.

use std::fs;

use raf_core::project::Project;
use raf_core::session::{ProjectSessionKind, ProjectSessionRegistry, SessionId};
use serde_json::json;
use uuid::Uuid;

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCommandEvent {
    Activate(SessionId),
}

pub struct SessionCommandContext<'a> {
    pub project: Option<&'a Project>,
    pub registry: &'a mut ProjectSessionRegistry,
    pub events: &'a mut Vec<SessionCommandEvent>,
}

pub fn execute(
    name: &str,
    command: &ParsedCommand,
    ctx: &mut SessionCommandContext<'_>,
) -> CommandOutput {
    match name {
        "session.list" => list(ctx),
        "session.create" => create(command, ctx),
        "session.open" => open(command, ctx),
        "session.rename" => rename(command, ctx),
        "session.duplicate" => duplicate(command, ctx),
        "session.remove" => remove(command, ctx),
        _ => CommandOutput::error("Session command", format!("Unknown command: {name}")),
    }
}

fn list(ctx: &SessionCommandContext<'_>) -> CommandOutput {
    let sessions = ctx
        .registry
        .sessions
        .iter()
        .map(|session| {
            json!({
                "id": session.id.0,
                "name": session.name,
                "kind": format!("{:?}", session.kind),
                "active": session.id == ctx.registry.active_session,
            })
        })
        .collect::<Vec<_>>();
    CommandOutput::info(
        "Sessions",
        vec![format!("count: {}", sessions.len())],
        json!({"ok": true, "active_session": ctx.registry.active_session.0, "sessions": sessions}),
    )
}

fn create(command: &ParsedCommand, ctx: &mut SessionCommandContext<'_>) -> CommandOutput {
    let Some(project) = ctx.project else {
        return CommandOutput::error("Create session", "No active project.");
    };
    let Some(name) = command.arg("name").or_else(|| command.first_positional()) else {
        return CommandOutput::error("Create session", "Missing name=<session-name>.");
    };
    let kind = match command.arg("kind") {
        Some("interface") => ProjectSessionKind::Interface,
        Some("electronics") => ProjectSessionKind::ElectronicsDesign,
        Some("world") | None => ProjectSessionKind::World,
        Some(_) => {
            return CommandOutput::error(
                "Create session",
                "kind must be world, interface, or electronics.",
            )
        }
    };
    let id = ctx.registry.create(name, kind);
    if let Some(session) = ctx
        .registry
        .sessions
        .iter()
        .find(|session| session.id == id)
    {
        if let Err(error) = session.ensure_storage(&project.path) {
            return CommandOutput::error("Create session", format!("Session storage: {error}"));
        }
    }
    if let Err(error) = ctx.registry.save(&project.path) {
        return CommandOutput::error("Create session", error);
    }
    CommandOutput::changed(
        "Create session",
        vec![format!("session_id: {}", id.0), format!("name: {name}")],
        json!({"ok": true, "session_id": id.0, "name": name}),
    )
}

fn open(command: &ParsedCommand, ctx: &mut SessionCommandContext<'_>) -> CommandOutput {
    let Some(id) = find_session_id(command, ctx.registry) else {
        return CommandOutput::error("Open session", "Session was not found.");
    };
    ctx.events.push(SessionCommandEvent::Activate(id));
    CommandOutput::info(
        "Open session",
        vec![
            format!("session_id: {}", id.0),
            "status: scheduled".to_string(),
        ],
        json!({"ok": true, "session_id": id.0, "status": "scheduled"}),
    )
}

fn rename(command: &ParsedCommand, ctx: &mut SessionCommandContext<'_>) -> CommandOutput {
    let Some(project) = ctx.project else {
        return CommandOutput::error("Rename session", "No active project.");
    };
    let Some(id) = find_session_id(command, ctx.registry) else {
        return CommandOutput::error("Rename session", "Session was not found.");
    };
    let Some(name) = command.arg("name") else {
        return CommandOutput::error("Rename session", "Missing name=<session-name>.");
    };
    if let Err(error) = ctx.registry.rename(id, name) {
        return CommandOutput::error("Rename session", error);
    }
    if let Err(error) = ctx.registry.save(&project.path) {
        return CommandOutput::error("Rename session", error);
    }
    CommandOutput::changed(
        "Rename session",
        vec![format!("session_id: {}", id.0), format!("name: {name}")],
        json!({"ok": true, "session_id": id.0, "name": name}),
    )
}

fn duplicate(command: &ParsedCommand, ctx: &mut SessionCommandContext<'_>) -> CommandOutput {
    let Some(project) = ctx.project else {
        return CommandOutput::error("Duplicate session", "No active project.");
    };
    let Some(source_id) = find_session_id(command, ctx.registry) else {
        return CommandOutput::error("Duplicate session", "Source session was not found.");
    };
    let Some(name) = command.arg("name") else {
        return CommandOutput::error("Duplicate session", "Missing name=<session-name>.");
    };
    let Some(source) = ctx
        .registry
        .sessions
        .iter()
        .find(|session| session.id == source_id)
        .cloned()
    else {
        return CommandOutput::error("Duplicate session", "Source session was not found.");
    };
    let Some(duplicate_id) = ctx.registry.duplicate(source_id, name) else {
        return CommandOutput::error("Duplicate session", "Could not create duplicate session.");
    };
    let Some(target) = ctx
        .registry
        .sessions
        .iter()
        .find(|session| session.id == duplicate_id)
        .cloned()
    else {
        return CommandOutput::error("Duplicate session", "Duplicate session was not registered.");
    };
    if let Err(error) = copy_session_files(project, &source, &target) {
        return CommandOutput::error("Duplicate session", error);
    }
    if let Err(error) = ctx.registry.save(&project.path) {
        return CommandOutput::error("Duplicate session", error);
    }
    CommandOutput::changed(
        "Duplicate session",
        vec![
            format!("source_id: {}", source_id.0),
            format!("session_id: {}", duplicate_id.0),
        ],
        json!({"ok": true, "source_id": source_id.0, "session_id": duplicate_id.0}),
    )
}

fn remove(command: &ParsedCommand, ctx: &mut SessionCommandContext<'_>) -> CommandOutput {
    let Some(project) = ctx.project else {
        return CommandOutput::error("Remove session", "No active project.");
    };
    let Some(id) = find_session_id(command, ctx.registry) else {
        return CommandOutput::error("Remove session", "Session was not found.");
    };
    if !ctx.registry.remove(id) {
        return CommandOutput::error(
            "Remove session",
            "The active or final remaining session cannot be removed.",
        );
    }
    if let Err(error) = ctx.registry.save(&project.path) {
        return CommandOutput::error("Remove session", error);
    }
    CommandOutput::changed(
        "Remove session",
        vec![
            format!("session_id: {id:?}"),
            "storage retained for recovery".to_string(),
        ],
        json!({"ok": true, "session_id": id.0, "storage_retained": true}),
    )
}

fn find_session_id(
    command: &ParsedCommand,
    registry: &ProjectSessionRegistry,
) -> Option<SessionId> {
    let target = command
        .arg("session")
        .or_else(|| command.arg("source"))
        .or_else(|| command.first_positional())?;
    if let Ok(uuid) = Uuid::parse_str(target) {
        let id = SessionId(uuid);
        return registry
            .sessions
            .iter()
            .any(|session| session.id == id)
            .then_some(id);
    }
    registry
        .sessions
        .iter()
        .find(|session| session.name.eq_ignore_ascii_case(target))
        .map(|session| session.id)
}

fn copy_session_files(
    project: &Project,
    source: &raf_core::session::ProjectSession,
    target: &raf_core::session::ProjectSession,
) -> Result<(), String> {
    target
        .ensure_storage(&project.path)
        .map_err(|error| format!("target storage: {error}"))?;
    for (source_file, target_file) in [
        (&source.scene_file, &target.scene_file),
        (&source.nodes_file, &target.nodes_file),
        (&source.ui_document_file, &target.ui_document_file),
        (&source.editor_camera_file(), &target.editor_camera_file()),
        (&source.schematic_file, &target.schematic_file),
        (&source.pcb_file, &target.pcb_file),
    ] {
        let source_path = source.path(&project.path, source_file);
        if !source_path.is_file() {
            continue;
        }
        let target_path = target.path(&project.path, target_file);
        fs::copy(&source_path, &target_path).map_err(|error| {
            format!(
                "copy {} to {}: {error}",
                source_file.display(),
                target_file.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::project::ProjectType;

    #[test]
    fn session_lookup_accepts_case_insensitive_names() {
        let registry = ProjectSessionRegistry::new(ProjectType::Game);
        let command = ParsedCommand {
            raw: String::new(),
            name: "session.open".to_string(),
            args: [("session".to_string(), "main".to_string())]
                .into_iter()
                .collect(),
            positional: Vec::new(),
            structured_args: None,
        };
        assert_eq!(
            find_session_id(&command, &registry),
            Some(registry.active_session)
        );
    }
}
