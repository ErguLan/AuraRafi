//! Structured UI authoring; runtime scripts never enter this editor boundary.
use super::{parser::ParsedCommand, CommandOutput};
use crate::ui_document_store as store;
use std::path::Path;

pub fn execute(
    command: &ParsedCommand,
    assets_root: Option<&Path>,
    preview: bool,
) -> CommandOutput {
    match run(command, assets_root, preview) {
        Ok((changed, data)) => {
            if changed {
                CommandOutput::changed("ui.commands.title", Vec::new(), data)
            } else {
                CommandOutput::info("ui.commands.title", Vec::new(), data)
            }
        }
        Err(error) => CommandOutput::error("ui.commands.title", error),
    }
}
pub fn supports(name: &str) -> bool {
    matches!(
        name,
        "ui.document.read" | "ui.document.create" | "ui.document.apply" | "ui.document.undo"
    )
}
fn run(
    command: &ParsedCommand,
    assets_root: Option<&Path>,
    preview: bool,
) -> Result<(bool, serde_json::Value), String> {
    let root = assets_root
        .and_then(Path::parent)
        .ok_or("ui.project_required")?;
    let relative = command.arg("path").ok_or("ui.path_required")?;
    if !relative.ends_with(".ron") {
        return Err("ui.document_extension".into());
    }
    let path = store::document_path(root, Path::new(relative))?;
    let mut document = if command.name == "ui.document.create" {
        if path.exists() {
            return Err("ui.document_exists".into());
        }
        raf_ui::UiDocument::blank(command.arg("name").unwrap_or("Interface"))
    } else {
        store::load(&path)?
    };
    let before_revision = store::revision(&document);
    if matches!(
        command.name.as_str(),
        "ui.document.apply" | "ui.document.undo"
    ) && command.arg("revision").is_none()
    {
        return Err("ui.document_conflict".into());
    }
    if command
        .arg("revision")
        .is_some_and(|r| r.parse::<u64>().ok() != Some(before_revision))
    {
        return Err("ui.document_conflict".into());
    }
    let changed = match command.name.as_str() {
        "ui.document.read" => false,
        "ui.document.create" => true,
        "ui.document.apply" => {
            let raw = command
                .structured_arg("edits")
                .cloned()
                .or_else(|| {
                    command
                        .arg("edits")
                        .and_then(|s| serde_json::from_str(s).ok())
                })
                .ok_or("ui.edits_required")?;
            let edits: Vec<raf_ui::authoring::UiEdit> =
                serde_json::from_value(raw).map_err(|e| e.to_string())?;
            raf_ui::authoring::apply_ui_edits(&mut document, &edits)?
        }
        "ui.document.undo" => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("ui.path_required")?;
            document = store::load(&path.with_file_name(format!(".{name}.previous")))?;
            true
        }
        _ => return Err("ui.unknown_command".into()),
    };
    if changed && !preview {
        store::save(&path, &document)?;
    }
    Ok((
        changed,
        serde_json::json!({"path":relative,"revision":store::revision(&document).to_string(),
        "previous_revision":before_revision.to_string(),"preview":preview,"document":document}),
    ))
}
