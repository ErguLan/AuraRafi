//! Declarative commands for user-authored `UiDocument` data.

use raf_ui::{
    RafUiStudio, UiAlign, UiColorMode, UiCompactMode, UiDocument, UiDocumentSpace, UiEnvironment,
    UiFlow, UiImage, UiImageSource, UiJustify, UiNode, UiNodeKind, UiResponsiveRule, UiScrollAxis,
    UiSkeleton, UiSkeletonShape, UiTextInput,
};
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
        "rafui.studio.preview" => studio_preview(command, ctx),
        _ => CommandOutput::error("UI document", format!("Unknown command: {name}")),
    }
}

/// Executes the read-only RafUI Studio preview against a standalone blank
/// document. The editor binary uses this same path for CMD calls, so the
/// external and internal command contracts cannot drift.
pub fn standalone_studio_preview(command: &ParsedCommand) -> CommandOutput {
    let mut document = UiDocument::default();
    let mut context = UiDocumentCommandContext {
        document: &mut document,
    };
    studio_preview(command, &mut context)
}

fn studio_preview(command: &ParsedCommand, ctx: &UiDocumentCommandContext<'_>) -> CommandOutput {
    let scale_factor = command
        .arg("dpi")
        .or_else(|| command.arg("scale"))
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0)
        .clamp(1.0, 4.0);
    let color_mode = command
        .arg("theme")
        .or_else(|| command.arg("mode"))
        .and_then(parse_color_mode)
        .unwrap_or(UiColorMode::Dark);
    let format = command.arg("format").unwrap_or("text").to_ascii_lowercase();
    let environment = UiEnvironment {
        viewport_size: [1280.0, 720.0],
        scale_factor,
        color_mode,
        ..UiEnvironment::default()
    };
    let preview = RafUiStudio::new([1280, 720]).text_preview(ctx.document, environment);
    let json_value = serde_json::to_value(&preview).unwrap_or_else(|_| {
        serde_json::json!({
            "ok": false,
            "error": "Unable to serialize RafUI Studio preview."
        })
    });
    let lines = if format == "json" {
        vec![serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| json_value.to_string())]
    } else {
        preview.lines()
    };
    CommandOutput::info("RafUI Studio preview", lines, json_value)
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
    let mut node = match kind {
        UiNodeKind::Image => {
            let Some(source) = command.arg("source") else {
                return CommandOutput::error(
                    "Add UI node",
                    "Image nodes require source=<resource-key>.",
                );
            };
            UiNode::image(
                id,
                UiImage {
                    source: UiImageSource::new(source),
                    fit: command
                        .arg("fit")
                        .and_then(parse_image_fit)
                        .unwrap_or_default(),
                    tint: None,
                },
            )
        }
        UiNodeKind::TextInput => {
            let value_key = command.arg("value_key").unwrap_or(id);
            let mut input = UiTextInput::new(value_key);
            input.placeholder_key = command.arg("placeholder_key").map(str::to_string);
            input.multiline = command.bool_arg("multiline");
            input.password = command.bool_arg("password");
            input.submit_command = command.arg("submit_command").map(str::to_string);
            UiNode::text_input(id, input)
        }
        UiNodeKind::ScrollView => UiNode::scroll_view(
            id,
            command
                .arg("axis")
                .and_then(parse_scroll_axis)
                .unwrap_or_default(),
        ),
        UiNodeKind::Grid => UiNode::grid(id),
        UiNodeKind::Skeleton => UiNode::skeleton(
            id,
            UiSkeleton {
                shape: command
                    .arg("shape")
                    .and_then(parse_skeleton_shape)
                    .unwrap_or_default(),
                ..UiSkeleton::default()
            },
        ),
        _ => UiNode::new(id, kind),
    };
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
    apply_layout_args(command, &mut node);
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
        "image" => Some(UiNodeKind::Image),
        "text_input" | "input" => Some(UiNodeKind::TextInput),
        "scroll_view" | "scroll" => Some(UiNodeKind::ScrollView),
        "grid" => Some(UiNodeKind::Grid),
        "skeleton" => Some(UiNodeKind::Skeleton),
        _ => None,
    }
}

fn apply_layout_args(command: &ParsedCommand, node: &mut UiNode) {
    if let Some(flow) = command.arg("flow").and_then(parse_flow) {
        node.layout.flow = flow;
    }
    if node.kind == UiNodeKind::Grid {
        node.layout.flow = UiFlow::Grid;
    }
    if let Some(value) = command
        .arg("grow")
        .and_then(|value| value.parse::<f32>().ok())
    {
        node.layout.grow = value.max(0.0);
    }
    if let Some(value) = command
        .arg("width")
        .and_then(|value| value.parse::<f32>().ok())
    {
        node.layout.basis[0] = value.max(0.0);
    }
    if let Some(value) = command
        .arg("height")
        .and_then(|value| value.parse::<f32>().ok())
    {
        node.layout.basis[1] = value.max(0.0);
    }
    if let Some(value) = command
        .arg("gap")
        .and_then(|value| value.parse::<f32>().ok())
    {
        node.layout.gap = value.max(0.0);
    }
    if let Some(value) = command.arg("justify").and_then(parse_justify) {
        node.layout.justify_content = value;
    }
    if let Some(value) = command.arg("align").and_then(parse_align) {
        node.layout.align_items = value;
    }
    if let Some(value) = command.arg("compact").and_then(parse_compact) {
        node.layout.compact = value;
    }
    if let Some(value) = command
        .arg("columns")
        .and_then(|value| value.parse::<u16>().ok())
    {
        node.layout.grid.columns = value;
    }
    if let Some(value) = command
        .arg("min_column_width")
        .and_then(|value| value.parse::<f32>().ok())
    {
        node.layout.grid.min_column_width = value.max(1.0);
    }
    if let Some(max_width) = command
        .arg("responsive_max_width")
        .and_then(|value| value.parse::<f32>().ok())
    {
        node.layout.responsive.push(UiResponsiveRule {
            max_width: max_width.max(1.0),
            flow: command.arg("responsive_flow").and_then(parse_flow),
            basis: None,
            padding: None,
            gap: None,
            compact: command.arg("responsive_compact").and_then(parse_compact),
            grid_columns: command
                .arg("responsive_columns")
                .and_then(|value| value.parse::<u16>().ok()),
        });
        node.layout
            .responsive
            .sort_by(|left, right| left.max_width.total_cmp(&right.max_width));
    }
}

fn parse_flow(raw: &str) -> Option<UiFlow> {
    match raw.to_ascii_lowercase().as_str() {
        "none" => Some(UiFlow::None),
        "row" | "flex_row" => Some(UiFlow::Row),
        "column" | "flex_column" => Some(UiFlow::Column),
        "wrap" | "row_wrap" => Some(UiFlow::RowWrap),
        "grid" => Some(UiFlow::Grid),
        _ => None,
    }
}

fn parse_justify(raw: &str) -> Option<UiJustify> {
    match raw.to_ascii_lowercase().as_str() {
        "start" => Some(UiJustify::Start),
        "center" | "middle" => Some(UiJustify::Center),
        "end" => Some(UiJustify::End),
        "between" | "space_between" => Some(UiJustify::SpaceBetween),
        "around" | "space_around" => Some(UiJustify::SpaceAround),
        "evenly" | "space_evenly" => Some(UiJustify::SpaceEvenly),
        _ => None,
    }
}

fn parse_align(raw: &str) -> Option<UiAlign> {
    match raw.to_ascii_lowercase().as_str() {
        "start" => Some(UiAlign::Start),
        "center" | "middle" => Some(UiAlign::Center),
        "end" => Some(UiAlign::End),
        "stretch" => Some(UiAlign::Stretch),
        _ => None,
    }
}

fn parse_compact(raw: &str) -> Option<UiCompactMode> {
    match raw.to_ascii_lowercase().as_str() {
        "none" => Some(UiCompactMode::None),
        "wrap" => Some(UiCompactMode::Wrap),
        "stack" => Some(UiCompactMode::Stack),
        "auto" => Some(UiCompactMode::Auto),
        _ => None,
    }
}

fn parse_scroll_axis(raw: &str) -> Option<UiScrollAxis> {
    match raw.to_ascii_lowercase().as_str() {
        "vertical" | "y" => Some(UiScrollAxis::Vertical),
        "horizontal" | "x" => Some(UiScrollAxis::Horizontal),
        "both" | "xy" => Some(UiScrollAxis::Both),
        _ => None,
    }
}

fn parse_image_fit(raw: &str) -> Option<raf_ui::UiImageFit> {
    match raw.to_ascii_lowercase().as_str() {
        "contain" => Some(raf_ui::UiImageFit::Contain),
        "cover" => Some(raf_ui::UiImageFit::Cover),
        "stretch" => Some(raf_ui::UiImageFit::Stretch),
        _ => None,
    }
}

fn parse_skeleton_shape(raw: &str) -> Option<UiSkeletonShape> {
    match raw.to_ascii_lowercase().as_str() {
        "text" => Some(UiSkeletonShape::Text),
        "rectangle" | "rect" => Some(UiSkeletonShape::Rectangle),
        "circle" => Some(UiSkeletonShape::Circle),
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

fn parse_color_mode(raw: &str) -> Option<UiColorMode> {
    match raw.to_ascii_lowercase().as_str() {
        "system" => Some(UiColorMode::System),
        "dark" => Some(UiColorMode::Dark),
        "light" => Some(UiColorMode::Light),
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

    #[test]
    fn studio_preview_uses_the_same_internal_command_for_text_and_json() {
        let ParsedInput::Command(command) =
            parse_console_input("/rafui.studio.preview format=json dpi=1.25").unwrap()
        else {
            panic!("expected command");
        };
        let output = standalone_studio_preview(&command);
        assert_eq!(output.title, "RafUI Studio preview");
        assert!(output.lines[0].contains("RafUI Studio"));
        assert_eq!(output.json["recipe_version"], 1);
        assert_eq!(
            output.json["selected_density"]["contract"]["geometry_scale"],
            1.25
        );
    }
}
