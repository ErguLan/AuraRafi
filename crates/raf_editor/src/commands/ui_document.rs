//! Declarative commands for user-authored `UiDocument` data.

use raf_ui::{UiDocument, UiDocumentSpace, UiNode, UiNodeKind};
use serde_json::json;

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;

pub struct UiDocumentCommandContext<'a> {
    pub document: &'a mut UiDocument,
}

pub fn execute(
    name: &str,
    command: &ParsedCommand,
    ctx: &mut UiDocumentCommandContext<'_>,
) -> CommandOutput {
    match name {
        "ui.document.describe" => describe(ctx),
        "ui.node.add" => add_node(command, ctx),
        "ui.node.remove" => remove_node(command, ctx),
        "ui.document.set_space" => set_space(command, ctx),
        "ui.document.bind_camera" => bind_camera(command, ctx),
        "ui.document.clear_camera" => clear_camera(ctx),
        _ => CommandOutput::error("UI document", format!("Unknown command: {name}")),
    }
}

fn describe(ctx: &UiDocumentCommandContext<'_>) -> CommandOutput {
    CommandOutput::info(
        "UI document",
        vec![
            format!("id: {}", ctx.document.id.0),
            format!("name: {}", ctx.document.name),
            format!("space: {:?}", ctx.document.space),
            format!("nodes: {}", node_count(&ctx.document.root)),
        ],
        json!({
            "ok": true,
            "id": ctx.document.id.0,
            "name": ctx.document.name,
            "space": format!("{:?}", ctx.document.space),
            "node_count": node_count(&ctx.document.root),
        }),
    )
}

fn add_node(command: &ParsedCommand, ctx: &mut UiDocumentCommandContext<'_>) -> CommandOutput {
    let Some(id) = command.arg("id") else {
        return CommandOutput::error("Add UI node", "Missing id=<node-id>.");
    };
    let Some(kind) = command.arg("kind").and_then(parse_kind) else {
        return CommandOutput::error("Add UI node", "Missing or invalid kind=<node-kind>.");
    };
    let parent = command.arg("parent").unwrap_or("root");
    let mut node = UiNode::new(id, kind);
    if let Some(text_key) = command.arg("text_key") {
        node.text_key = Some(text_key.to_string());
    }
    if command.bool_arg("interactive") {
        node.interactive = true;
    }
    if command.bool_arg("focusable") {
        node.focusable = true;
        node.interactive = true;
    }
    match ctx.document.add_node(parent, node) {
        Ok(()) => CommandOutput::changed(
            "Add UI node",
            vec![format!("id: {id}"), format!("parent: {parent}")],
            json!({"ok": true, "id": id, "parent": parent}),
        ),
        Err(error) => CommandOutput::error("Add UI node", error),
    }
}

fn remove_node(command: &ParsedCommand, ctx: &mut UiDocumentCommandContext<'_>) -> CommandOutput {
    let Some(id) = command.arg("id").or_else(|| command.first_positional()) else {
        return CommandOutput::error("Remove UI node", "Missing id=<node-id>.");
    };
    if ctx.document.remove_node(id).is_some() {
        CommandOutput::changed(
            "Remove UI node",
            vec![format!("id: {id}")],
            json!({"ok": true, "id": id}),
        )
    } else {
        CommandOutput::error("Remove UI node", "UI node was not found or is the root.")
    }
}

fn set_space(command: &ParsedCommand, ctx: &mut UiDocumentCommandContext<'_>) -> CommandOutput {
    let Some(space) = command.arg("space").and_then(parse_space) else {
        return CommandOutput::error("Set UI space", "space must be screen, world, or camera.");
    };
    ctx.document.space = space;
    if space != UiDocumentSpace::Camera {
        ctx.document.clear_camera_binding();
        ctx.document.space = space;
    }
    CommandOutput::changed(
        "Set UI space",
        vec![format!("space: {space:?}")],
        json!({"ok": true, "space": format!("{:?}", space)}),
    )
}

fn bind_camera(command: &ParsedCommand, ctx: &mut UiDocumentCommandContext<'_>) -> CommandOutput {
    let Some(camera) = command.arg("camera") else {
        return CommandOutput::error("Bind UI camera", "Missing camera=<camera-key>.");
    };
    ctx.document.bind_to_camera(camera);
    CommandOutput::changed(
        "Bind UI camera",
        vec![format!("camera: {camera}")],
        json!({"ok": true, "camera": camera, "document_id": ctx.document.id.0}),
    )
}

fn clear_camera(ctx: &mut UiDocumentCommandContext<'_>) -> CommandOutput {
    ctx.document.clear_camera_binding();
    CommandOutput::changed(
        "Clear UI camera",
        vec!["space: Screen".to_string()],
        json!({"ok": true, "space": "Screen"}),
    )
}

fn parse_kind(raw: &str) -> Option<UiNodeKind> {
    match raw.to_ascii_lowercase().as_str() {
        "panel" => Some(UiNodeKind::Panel),
        "toolbar" => Some(UiNodeKind::Toolbar),
        "button" => Some(UiNodeKind::Button),
        "canvas" => Some(UiNodeKind::Canvas),
        "overlay" => Some(UiNodeKind::Overlay),
        "label" => Some(UiNodeKind::Label),
        "separator" => Some(UiNodeKind::Separator),
        "dock_area" => Some(UiNodeKind::DockArea),
        "floating_panel" => Some(UiNodeKind::FloatingPanel),
        "menu" => Some(UiNodeKind::Menu),
        "tooltip" => Some(UiNodeKind::Tooltip),
        _ => None,
    }
}

fn parse_space(raw: &str) -> Option<UiDocumentSpace> {
    match raw.to_ascii_lowercase().as_str() {
        "screen" => Some(UiDocumentSpace::Screen),
        "world" => Some(UiDocumentSpace::World),
        "camera" => Some(UiDocumentSpace::Camera),
        _ => None,
    }
}

fn node_count(node: &UiNode) -> usize {
    1 + node.children.iter().map(node_count).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::parser::{parse_console_input, ParsedInput};

    #[test]
    fn ui_node_command_does_not_create_default_content() {
        let ParsedInput::Command(command) =
            parse_console_input("/ui.node.add id=start kind=button parent=root").unwrap()
        else {
            panic!("expected command");
        };
        let mut document = UiDocument::blank("Interface");
        let mut context = UiDocumentCommandContext {
            document: &mut document,
        };
        assert!(execute("ui.node.add", &command, &mut context).changed);
        assert_eq!(document.root.children.len(), 1);
    }
}
