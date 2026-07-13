//! Asset-generation commands backed by the isolated Python worker.

use raf_ai::{
    AssetImageGenerationMode, AssetImageGenerationQueue, AssetImageJobStatus, AssetImageRequest,
    AssetImageSize, AssetLocalPngStyle,
};
use serde_json::json;
use uuid::Uuid;

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;

pub struct AssetCommandContext<'a> {
    pub project_root: Option<&'a std::path::Path>,
    pub image_queue: &'a mut AssetImageGenerationQueue,
}

pub fn execute(
    name: &str,
    command: &ParsedCommand,
    ctx: &mut AssetCommandContext<'_>,
) -> CommandOutput {
    match name {
        "asset.generate_image" => generate_image(command, ctx),
        "asset.generate_local_png" => generate_local_png(command, ctx),
        "asset.image_status" => image_status(command, ctx),
        "asset.cancel_image" => cancel_image(command, ctx),
        _ => CommandOutput::error("Asset command", format!("Unknown command: {name}")),
    }
}

fn generate_image(command: &ParsedCommand, ctx: &mut AssetCommandContext<'_>) -> CommandOutput {
    let request = match request_from_command(
        command,
        AssetImageGenerationMode::Remote,
        AssetLocalPngStyle::Icon,
        "gpt-image-2",
        false,
    ) {
        Ok(request) => request,
        Err(output) => return output,
    };
    start_generation(ctx, request, "Generate image")
}

fn generate_local_png(
    command: &ParsedCommand,
    ctx: &mut AssetCommandContext<'_>,
) -> CommandOutput {
    let style = match command.arg("style") {
        Some("icon") | None => AssetLocalPngStyle::Icon,
        Some("badge") => AssetLocalPngStyle::Badge,
        Some("sprite") => AssetLocalPngStyle::Sprite,
        Some("texture") => AssetLocalPngStyle::Texture,
        Some(_) => {
            return CommandOutput::error(
                "Generate local PNG",
                "style must be icon, badge, sprite, or texture.",
            )
        }
    };
    let request = match request_from_command(
        command,
        AssetImageGenerationMode::LocalPng,
        style,
        "local-png",
        true,
    ) {
        Ok(request) => request,
        Err(output) => return output,
    };
    start_generation(ctx, request, "Generate local PNG")
}

fn request_from_command(
    command: &ParsedCommand,
    generation_mode: AssetImageGenerationMode,
    local_style: AssetLocalPngStyle,
    default_model: &str,
    default_transparent: bool,
) -> Result<AssetImageRequest, CommandOutput> {
    let title = match generation_mode {
        AssetImageGenerationMode::Remote => "Generate image",
        AssetImageGenerationMode::LocalPng => "Generate local PNG",
    };
    let Some(prompt) = command.arg("prompt").or_else(|| command.first_positional()) else {
        return Err(CommandOutput::error(title, "Missing prompt=<description>."));
    };
    let Some(output_name) = command.arg("name") else {
        return Err(CommandOutput::error(title, "Missing name=<asset-name>."));
    };
    let size = match command.arg("size") {
        Some("landscape") => AssetImageSize::Landscape,
        Some("portrait") => AssetImageSize::Portrait,
        Some("square") | None => AssetImageSize::Square,
        Some(_) => {
            return Err(CommandOutput::error(
                title,
                "size must be square, landscape, or portrait.",
            ))
        }
    };

    Ok(AssetImageRequest {
        prompt: prompt.to_string(),
        model: if generation_mode == AssetImageGenerationMode::LocalPng {
            default_model.to_string()
        } else {
            command.arg("model").unwrap_or(default_model).to_string()
        },
        generation_mode,
        local_style,
        size,
        transparent: command
            .arg("transparent")
            .map(|value| matches!(value, "true" | "1" | "yes" | "on"))
            .unwrap_or(default_transparent),
        output_name: output_name.to_string(),
    })
}

fn start_generation(
    ctx: &mut AssetCommandContext<'_>,
    request: AssetImageRequest,
    title: &str,
) -> CommandOutput {
    let Some(project_root) = ctx.project_root else {
        return CommandOutput::error(title, "No active project.");
    };
    let is_local = request.generation_mode == AssetImageGenerationMode::LocalPng;
    match ctx.image_queue.start(project_root, request) {
        Ok(job_id) => CommandOutput::info(
            title,
            vec![
                format!("job_id: {job_id}"),
                "status: queued".to_string(),
                format!(
                    "source: {}",
                    if is_local { "local_png" } else { "remote" }
                ),
                "output: assets/generated/<name>.png".to_string(),
            ],
            json!({"ok": true, "job_id": job_id, "status": "queued"}),
        ),
        Err(error) => CommandOutput::error(title, error),
    }
}

fn image_status(command: &ParsedCommand, ctx: &mut AssetCommandContext<'_>) -> CommandOutput {
    let Some(raw_id) = command.arg("job") else {
        return CommandOutput::error("Image status", "Missing job=<uuid>.");
    };
    let Ok(job_id) = Uuid::parse_str(raw_id) else {
        return CommandOutput::error("Image status", "job must be a UUID.");
    };
    let Some(snapshot) = ctx.image_queue.snapshot(job_id) else {
        return CommandOutput::error("Image status", "Image job was not found.");
    };
    let (status, details) = match snapshot.status {
        AssetImageJobStatus::Running => ("running", json!({})),
        AssetImageJobStatus::Ready(asset) => (
            "ready",
            json!({
                "image_path": asset.image_path,
                "metadata_path": asset.metadata_path,
                "generation_mode": asset.generation_mode,
                "local_style": asset.local_style,
                "model": asset.model,
            }),
        ),
        AssetImageJobStatus::Failed(error) => ("failed", json!({"error": error})),
        AssetImageJobStatus::Cancelled => ("cancelled", json!({})),
    };
    CommandOutput::info(
        "Image status",
        vec![
            format!("job_id: {}", snapshot.id),
            format!("status: {status}"),
        ],
        json!({"ok": true, "job_id": snapshot.id, "status": status, "details": details}),
    )
}

fn cancel_image(command: &ParsedCommand, ctx: &mut AssetCommandContext<'_>) -> CommandOutput {
    let Some(raw_id) = command.arg("job") else {
        return CommandOutput::error("Cancel image", "Missing job=<uuid>.");
    };
    let Ok(job_id) = Uuid::parse_str(raw_id) else {
        return CommandOutput::error("Cancel image", "job must be a UUID.");
    };
    match ctx.image_queue.cancel(job_id) {
        Ok(true) => CommandOutput::info(
            "Cancel image",
            vec![format!("job_id: {job_id}"), "status: cancelled".to_string()],
            json!({"ok": true, "job_id": job_id, "status": "cancelled"}),
        ),
        Ok(false) => CommandOutput::error("Cancel image", "Image job is not running."),
        Err(error) => CommandOutput::error("Cancel image", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::parser::{parse_console_input, ParsedInput};

    #[test]
    fn image_command_requires_a_project_root() {
        let ParsedInput::Command(command) =
            parse_console_input("/asset.generate_image prompt=rock name=rock").unwrap()
        else {
            panic!("expected command");
        };
        let mut queue = AssetImageGenerationQueue::default();
        let mut ctx = AssetCommandContext {
            project_root: None,
            image_queue: &mut queue,
        };
        assert_eq!(
            execute("asset.generate_image", &command, &mut ctx).changed,
            false
        );
    }
}
