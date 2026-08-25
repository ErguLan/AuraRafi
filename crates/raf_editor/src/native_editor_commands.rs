//! Command translation for the native RafUI editor.
//!
//! This module is intentionally a boundary, not a second editor runtime. RafUI,
//! menus and context actions emit the same small set of intents; this file
//! translates those intents into operations owned by [`NativeEditorRuntime`].

use raf_core::project::Project;
use raf_core::scene::{Primitive, SceneGraph, SceneNodeId};

use crate::commands::game::{GameCommandContext, GameViewportPort, SceneSelectionState};
use crate::commands::output::{CommandLevel, CommandOutput};
use crate::commands::parser::{parse_console_input, ParsedInput};
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_project_controller::save_project_document;
use crate::native_workbench::NativeWorkbenchIntent;

fn apply_inspector_commit(
    runtime: &mut NativeEditorRuntime,
    scene: &mut SceneGraph,
    target: SceneNodeId,
    field: &str,
    value: &str,
) {
    if !scene.is_valid_node(target) {
        return;
    }
    let parsed = value.trim().parse::<f32>().ok();
    runtime.mutate_scene(scene, |scene| {
        let Some(node) = scene.get_mut(target) else {
            return;
        };
        match field {
            "name" => {
                let name = value.trim();
                if !name.is_empty() {
                    node.name = name.to_string();
                }
            }
            "color" => {
                if let Some(color) = parse_node_color(value) {
                    node.color = color;
                }
            }
            "position.x" => {
                if let Some(value) = parsed {
                    node.position.x = value;
                }
            }
            "position.y" => {
                if let Some(value) = parsed {
                    node.position.y = value;
                }
            }
            "position.z" => {
                if let Some(value) = parsed {
                    node.position.z = value;
                }
            }
            "rotation.x" => {
                if let Some(value) = parsed {
                    node.rotation.x = value;
                }
            }
            "rotation.y" => {
                if let Some(value) = parsed {
                    node.rotation.y = value;
                }
            }
            "rotation.z" => {
                if let Some(value) = parsed {
                    node.rotation.z = value;
                }
            }
            "scale.x" => {
                if let Some(value) = parsed.filter(|value| value.abs() > f32::EPSILON) {
                    node.scale.x = value;
                }
            }
            "scale.y" => {
                if let Some(value) = parsed.filter(|value| value.abs() > f32::EPSILON) {
                    node.scale.y = value;
                }
            }
            "scale.z" => {
                if let Some(value) = parsed.filter(|value| value.abs() > f32::EPSILON) {
                    node.scale.z = value;
                }
            }
            _ => {}
        }
    });
}

fn parse_node_color(value: &str) -> Option<raf_core::scene::NodeColor> {
    let hex = value.trim().trim_start_matches('#');
    let digits = match hex.len() {
        6 | 8 => hex,
        _ => return None,
    };
    let parse = |range: std::ops::Range<usize>| u8::from_str_radix(&digits[range], 16).ok();
    Some(raf_core::scene::NodeColor::rgba(
        parse(0..2)?,
        parse(2..4)?,
        parse(4..6)?,
        if digits.len() == 8 { parse(6..8)? } else { 255 },
    ))
}

fn apply_console_command(
    runtime: &mut NativeEditorRuntime,
    scene: &mut SceneGraph,
    _project: Option<&Project>,
    raw: &str,
) -> CommandOutput {
    let parsed = match parse_console_input(raw) {
        Ok(ParsedInput::Command(command)) => command,
        Ok(ParsedInput::Message(message)) => {
            return CommandOutput::info(
                "Console",
                vec![message],
                serde_json::json!({"ok": true, "kind": "message"}),
            );
        }
        Err(error) => return CommandOutput::error("Console command", error),
    };

    if matches!(parsed.name.as_str(), "help" | "commands") {
        return CommandOutput::info(
            "Console commands",
            vec![
                "/game.describe_scene".to_string(),
                "/game.add primitive=cube".to_string(),
                "/game.select name=Player".to_string(),
                "/undo".to_string(),
                "/redo".to_string(),
            ],
            serde_json::json!({"ok": true}),
        );
    }

    let mut selection = SceneSelectionState {
        selected_node: runtime.game_viewport().selected.first().copied(),
        selected_nodes: runtime.game_viewport().selected.clone(),
    };
    let output = if parsed.name.starts_with("game.") {
        let viewport = runtime.game_viewport_mut();
        let mut context = GameCommandContext {
            scene,
            selection: &mut selection,
            viewport: viewport as &mut dyn GameViewportPort,
        };
        crate::commands::game::execute(&parsed.name, &parsed, &mut context)
    } else {
        CommandOutput::error(
            "Console command",
            format!("Unknown command '{}'. Use /help.", parsed.name),
        )
    };

    runtime.game_viewport_mut().selected = selection.selected_nodes;
    if let Some(selected) = selection.selected_node {
        if runtime.game_viewport().selected.is_empty() {
            runtime.game_viewport_mut().selected.push(selected);
        }
    }
    if output.changed || !matches!(output.level, CommandLevel::Error) {
        runtime.request_ui_frame();
    }
    output
}

pub(crate) fn apply_workbench_intents(
    runtime: &mut NativeEditorRuntime,
    scene: &mut SceneGraph,
    project: Option<&Project>,
    intents: Vec<NativeWorkbenchIntent>,
) -> (Vec<CommandOutput>, bool) {
    let mut console_outputs = Vec::new();
    let mut saved_document = false;
    for intent in intents {
        match intent {
            NativeWorkbenchIntent::Window(_) => {
                // The native application executes window commands after the
                // workbench releases its retained-input borrow.
            }
            NativeWorkbenchIntent::OpenSettings { .. } => {
                // Settings is a top-level RafUI overlay handled by the native
                // application, not a workbench domain command.
            }
            NativeWorkbenchIntent::ReturnToHub { .. } => {
                // The native application handles this transition after the
                // current input frame releases its runtime borrow.
            }
            NativeWorkbenchIntent::ProjectSettingToggle { .. } => {
                // Project settings are persisted by the native application
                // after the workbench releases its project borrow.
            }
            NativeWorkbenchIntent::ProjectSettingRange { .. } => {
                // Same ownership boundary as toggles: numeric project settings
                // are applied and saved by the native application.
            }
            NativeWorkbenchIntent::Viewport(action) => {
                use crate::panels::viewport_toolbar_surface::ViewportToolbarAction;
                use raf_render::gizmo::GizmoMode;
                match action {
                    ViewportToolbarAction::Select => {
                        runtime.game_viewport_mut().bridge_mut().gizmo_mut().visible = false;
                    }
                    ViewportToolbarAction::Move => runtime
                        .game_viewport_mut()
                        .set_gizmo_mode(GizmoMode::Translate),
                    ViewportToolbarAction::Rotate => runtime
                        .game_viewport_mut()
                        .set_gizmo_mode(GizmoMode::Rotate),
                    ViewportToolbarAction::Scale => {
                        runtime.game_viewport_mut().set_gizmo_mode(GizmoMode::Scale)
                    }
                    ViewportToolbarAction::Focus => runtime.focus_selection(scene),
                    ViewportToolbarAction::Solid => {
                        runtime.game_viewport_mut().render_style =
                            crate::panels::viewport_controller::NativeViewportRenderStyle::Solid
                    }
                    ViewportToolbarAction::Wireframe => {
                        runtime.game_viewport_mut().render_style =
                            crate::panels::viewport_controller::NativeViewportRenderStyle::Wireframe
                    }
                    ViewportToolbarAction::Preview => {
                        runtime.game_viewport_mut().render_style =
                            crate::panels::viewport_controller::NativeViewportRenderStyle::Preview
                    }
                    ViewportToolbarAction::TogglePolygons => {
                        let viewport = runtime.game_viewport_mut();
                        viewport.solid_show_surface_edges = !viewport.solid_show_surface_edges
                    }
                    ViewportToolbarAction::ToggleGrid => {
                        let viewport = runtime.game_viewport_mut();
                        viewport.grid_visible = !viewport.grid_visible;
                    }
                    ViewportToolbarAction::ToggleLabels => {
                        let viewport = runtime.game_viewport_mut();
                        viewport.show_labels = !viewport.show_labels;
                    }
                    ViewportToolbarAction::View2d => {
                        runtime.game_viewport_mut().mode =
                            crate::panels::viewport_controller::NativeViewportMode::View2d
                    }
                    ViewportToolbarAction::View3d => {
                        runtime.game_viewport_mut().mode =
                            crate::panels::viewport_controller::NativeViewportMode::View3d
                    }
                    ViewportToolbarAction::ResetView => {
                        runtime
                            .game_viewport_mut()
                            .bridge_mut()
                            .reset_isometric_view();
                    }
                    ViewportToolbarAction::CreatePrimitive(primitive) => {
                        runtime.create_primitive(scene, primitive);
                    }
                    // Building style changes persist through the project
                    // settings path; the workbench already updated its own
                    // toolbar state before emitting this intent.
                    ViewportToolbarAction::SetBuildingStyle(_) => {}
                }
                runtime.request_ui_frame();
            }
            NativeWorkbenchIntent::InspectorCommit {
                target,
                field,
                value,
            } => apply_inspector_commit(runtime, scene, target, &field, &value),
            NativeWorkbenchIntent::Command(command) => {
                if let Some(raw_resize) = command.strip_prefix("layout.resize.") {
                    if let Some((side, raw_value)) = raw_resize.split_once(':') {
                        if let Ok(value) = raw_value.parse::<f32>() {
                            match side {
                                "left" => runtime.resize_left_panel(value),
                                "right" => runtime.resize_right_panel(value),
                                "bottom" => runtime.resize_bottom_dock(value),
                                _ => {}
                            }
                        }
                    }
                } else if let Some(raw_command) = command.strip_prefix("console.submit:") {
                    console_outputs.push(apply_console_command(
                        runtime,
                        scene,
                        project,
                        raw_command,
                    ));
                } else if command == "bottom.toggle-collapsed" {
                    runtime.toggle_bottom_dock();
                } else if command == crate::application_menu::command::VIEW_HIERARCHY {
                    runtime.toggle_left_panel();
                } else if command == crate::application_menu::command::VIEW_INSPECTOR {
                    runtime.toggle_right_panel();
                } else if let Some(raw) = command.strip_prefix("hierarchy.select.range:") {
                    let mut parts = raw.splitn(3, ':');
                    let Some(anchor) = parts.next().and_then(|value| value.parse::<usize>().ok())
                    else {
                        continue;
                    };
                    let Some(target) = parts.next().and_then(|value| value.parse::<usize>().ok())
                    else {
                        continue;
                    };
                    let ordered = parts
                        .next()
                        .unwrap_or_default()
                        .split(',')
                        .filter_map(|value| value.parse::<usize>().ok())
                        .map(SceneNodeId)
                        .collect::<Vec<_>>();
                    runtime.select_node_range(
                        scene,
                        &ordered,
                        SceneNodeId(anchor),
                        SceneNodeId(target),
                    );
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.select:") {
                    let (raw_id, mode) = raw_id.split_once(':').unwrap_or((raw_id, "replace"));
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.select_node_with_modifier(scene, SceneNodeId(id), mode == "shift");
                    }
                } else if command == "hierarchy.clear-selection" {
                    runtime.game_viewport_mut().selected.clear();
                    runtime.request_ui_frame();
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.focus:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.select_node(scene, SceneNodeId(id));
                        runtime.focus_selection(scene);
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.delete:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.select_node(scene, SceneNodeId(id));
                        runtime.delete_selected(scene);
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.visibility:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                node.visible = !node.visible;
                            }
                        });
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.lock:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                node.locked = !node.locked;
                            }
                        });
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.copy:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.copy_node(SceneNodeId(id));
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.paste:") {
                    let parent = raw_id
                        .parse::<usize>()
                        .ok()
                        .map(SceneNodeId)
                        .filter(|id| scene.is_valid_node(*id));
                    runtime.paste_into(scene, parent);
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.select-children:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.select_children(scene, SceneNodeId(id));
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.reparent-root:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.reparent_to_root(scene, SceneNodeId(id));
                    }
                } else if let Some(raw) = command.strip_prefix("hierarchy.reparent:") {
                    if let Some((raw_ids, raw_parent)) = raw.split_once(':') {
                        let ids = raw_ids
                            .split(',')
                            .filter_map(|value| value.parse::<usize>().ok())
                            .map(SceneNodeId)
                            .collect::<Vec<_>>();
                        if !ids.is_empty() {
                            let parent = (raw_parent != "root")
                                .then(|| raw_parent.parse::<usize>().ok().map(SceneNodeId))
                                .flatten()
                                .filter(|id| scene.is_valid_node(*id));
                            runtime.reparent_nodes(scene, &ids, parent);
                        }
                    }
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.ungroup:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.ungroup(scene, SceneNodeId(id));
                    }
                } else if let Some(raw) = command.strip_prefix("hierarchy.create-primitive:root:") {
                    if let Some(primitive) = parse_primitive_slug(raw) {
                        runtime.create_primitive(scene, primitive);
                    }
                } else if let Some(raw) = command.strip_prefix("hierarchy.create-primitive:") {
                    if let Some((raw_id, raw_primitive)) = raw.split_once(':') {
                        if let (Ok(id), Some(primitive)) =
                            (raw_id.parse::<usize>(), parse_primitive_slug(raw_primitive))
                        {
                            runtime.create_primitive_under(scene, SceneNodeId(id), primitive);
                        }
                    }
                } else if command == "hierarchy.create-entity:root" {
                    runtime.create_entity(scene);
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.create-entity:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.create_entity_under(scene, SceneNodeId(id));
                    }
                } else if command == "hierarchy.create-folder:root" {
                    runtime.create_folder(scene);
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.create-folder:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.create_folder_under(scene, SceneNodeId(id));
                    }
                } else if let Some(raw) = command.strip_prefix("inspector.primitive:") {
                    if let Some((raw_id, raw_primitive)) = raw.split_once(':') {
                        if let (Ok(id), Some(primitive)) =
                            (raw_id.parse::<usize>(), parse_primitive_slug(raw_primitive))
                        {
                            let id = SceneNodeId(id);
                            runtime.mutate_scene(scene, |scene| {
                                if let Some(node) = scene.get_mut(id) {
                                    node.primitive = primitive;
                                    node.source_asset = Some(format!(
                                        "builtin://primitive/{}",
                                        raw_primitive.to_ascii_lowercase()
                                    ));
                                }
                            });
                        }
                    }
                } else if let Some(raw_id) = command.strip_prefix("inspector.visibility:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                node.visible = !node.visible;
                            }
                        });
                    }
                } else if let Some(raw) = command.strip_prefix("inspector.toggle:") {
                    let mut parts = raw.splitn(3, ':');
                    let key = parts.next().unwrap_or_default();
                    let value = parts
                        .next()
                        .is_some_and(|value| matches!(value, "true" | "1" | "yes" | "on"));
                    let Some(raw_id) = parts.next() else {
                        continue;
                    };
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            let Some(node) = scene.get_mut(id) else {
                                return;
                            };
                            match key {
                                "inspector.audio.enabled" => node.audio_source.enabled = value,
                                "inspector.audio.autoplay" => node.audio_source.autoplay = value,
                                "inspector.audio.looping" => node.audio_source.looping = value,
                                "inspector.physics.enabled" => node.rigid_body.enabled = value,
                                "inspector.physics.gravity" => node.rigid_body.use_gravity = value,
                                "inspector.physics.trigger" => node.rigid_body.is_trigger = value,
                                _ => {}
                            }
                        });
                    }
                } else if let Some(raw) = command.strip_prefix("inspector.color.range:") {
                    let mut parts = raw.splitn(3, ':');
                    let key = parts.next().unwrap_or_default();
                    let value = parts
                        .next()
                        .and_then(|value| value.parse::<f32>().ok())
                        .map(|value| value.round().clamp(0.0, 255.0) as u8);
                    let Some(raw_id) = parts.next() else {
                        continue;
                    };
                    let Some(channel) = key.strip_prefix("inspector.color.") else {
                        continue;
                    };
                    if let (Some(value), Ok(id)) = (value, raw_id.parse::<usize>()) {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                match channel {
                                    "r" => node.color.r = value,
                                    "g" => node.color.g = value,
                                    "b" => node.color.b = value,
                                    "a" => node.color.a = value,
                                    _ => {}
                                }
                            }
                        });
                    }
                } else if let Some(raw) = command.strip_prefix("inspector.range:") {
                    let mut parts = raw.splitn(3, ':');
                    let key = parts.next().unwrap_or_default();
                    let value = parts
                        .next()
                        .and_then(|value| value.parse::<f32>().ok())
                        .unwrap_or_default()
                        .clamp(0.0, 1.0);
                    let Some(raw_id) = parts.next() else {
                        continue;
                    };
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                if key == "inspector.audio.volume" {
                                    node.audio_source.volume = value;
                                }
                            }
                        });
                    }
                } else if let Some(raw_id) = command.strip_prefix("inspector.lock:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                node.locked = !node.locked;
                            }
                        });
                    }
                } else if let Some(raw_id) = command.strip_prefix("inspector.reset-transform:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                node.position = glam::Vec3::ZERO;
                                node.rotation = glam::Vec3::ZERO;
                                node.scale = glam::Vec3::ONE;
                            }
                        });
                    }
                } else if let Some(raw_id) = command.strip_prefix("inspector.reset-all:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        let id = SceneNodeId(id);
                        runtime.mutate_scene(scene, |scene| {
                            if let Some(node) = scene.get_mut(id) {
                                node.position = glam::Vec3::ZERO;
                                node.rotation = glam::Vec3::ZERO;
                                node.scale = glam::Vec3::ONE;
                                node.color =
                                    raf_core::scene::NodeColor::for_primitive(node.primitive);
                            }
                        });
                    }
                } else if command == crate::application_menu::command::EDIT_UNDO {
                    runtime.undo(scene);
                } else if command == crate::application_menu::command::EDIT_REDO {
                    runtime.redo(scene);
                } else if command == crate::application_menu::command::EDIT_DUPLICATE {
                    runtime.duplicate_selected(scene);
                } else if command == crate::application_menu::command::EDIT_COPY {
                    runtime.copy_selected();
                } else if command == crate::application_menu::command::EDIT_PASTE {
                    runtime.paste_selected(scene);
                } else if command == crate::application_menu::command::EDIT_DELETE {
                    runtime.delete_selected(scene);
                } else if command == crate::application_menu::command::EDIT_SELECT_ALL {
                    runtime.select_all(scene);
                } else if command == crate::application_menu::command::PROJECT_SAVE {
                    if let Some(project) = project {
                        if let Err(error) =
                            save_project_document(project, scene, runtime.node_graph())
                        {
                            tracing::warn!(%error, "native project save failed");
                        } else {
                            saved_document = true;
                        }
                    }
                } else if command == crate::application_menu::command::VIEW_GRID {
                    let viewport = runtime.game_viewport_mut();
                    viewport.grid_visible = !viewport.grid_visible;
                } else if let Some(raw_id) = command.strip_prefix("hierarchy.duplicate:") {
                    if let Ok(id) = raw_id.parse::<usize>() {
                        runtime.select_node(scene, SceneNodeId(id));
                        runtime.duplicate_selected(scene);
                    }
                } else if let Some(slug) = command.strip_prefix("assets.create-primitive:") {
                    if let Some(primitive) = parse_primitive_slug(slug) {
                        runtime.create_primitive(scene, primitive);
                    }
                } else if let Some(raw) = command.strip_prefix("assets.create-script:") {
                    let mut parts = raw.splitn(2, ':');
                    let language = parts.next().unwrap_or("rhai");
                    let name = sanitize_asset_name(parts.next().unwrap_or("new_script"));
                    if let Some(project) = project {
                        if let Err(error) = create_asset_script(&project.path, language, &name) {
                            tracing::warn!(%error, "native asset script creation failed");
                        }
                    }
                } else if let Some(raw) = command.strip_prefix("assets.create-file:") {
                    if let Some(project) = project {
                        let name = sanitize_asset_name(raw);
                        let path = project.path.join("assets").join(format!("{name}.txt"));
                        if let Err(error) = std::fs::create_dir_all(project.path.join("assets"))
                            .and_then(|_| std::fs::write(&path, ""))
                        {
                            tracing::warn!(%error, path = %path.display(), "native asset file creation failed");
                        }
                    }
                } else if let Some(slug) = command.strip_prefix("nodes.add.") {
                    if let Some(node) = node_from_slug(slug) {
                        runtime.add_graph_node(node);
                    }
                } else if let Some(raw) = command.strip_prefix("nodes.drag.start:") {
                    let mut parts = raw.split(':');
                    let (Some(raw_id), Some(raw_x), Some(raw_y)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        continue;
                    };
                    if let (Ok(id), Ok(x), Ok(y)) = (
                        uuid::Uuid::parse_str(raw_id),
                        raw_x.parse::<f32>(),
                        raw_y.parse::<f32>(),
                    ) {
                        runtime.begin_graph_node_drag(raf_nodes::NodeId(id), [x, y]);
                    }
                } else if let Some(raw) = command.strip_prefix("nodes.drag.move:") {
                    let mut parts = raw.split(':');
                    let (Some(raw_id), Some(raw_x), Some(raw_y)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        continue;
                    };
                    if let (Ok(id), Ok(x), Ok(y)) = (
                        uuid::Uuid::parse_str(raw_id),
                        raw_x.parse::<f32>(),
                        raw_y.parse::<f32>(),
                    ) {
                        runtime.move_graph_node_drag(raf_nodes::NodeId(id), [x, y]);
                    }
                } else if command == "nodes.drag.end" {
                    runtime.end_graph_node_drag();
                } else if let Some(raw_id) = command.strip_prefix("nodes.select.") {
                    if let Ok(id) = uuid::Uuid::parse_str(raw_id) {
                        runtime.select_graph_node(raf_nodes::NodeId(id));
                    }
                } else if let Some(raw_id) = command.strip_prefix("nodes.delete.") {
                    if let Ok(id) = uuid::Uuid::parse_str(raw_id) {
                        runtime.delete_graph_node(raf_nodes::NodeId(id));
                    }
                } else if command == "nodes.new-graph" {
                    runtime.reset_graph();
                } else if command == "nodes.compile" {
                    tracing::info!(
                        nodes = runtime.node_graph().nodes.len(),
                        links = runtime.node_graph().connections.len(),
                        "native node graph validation requested"
                    );
                } else {
                    tracing::debug!(command = %command, "native workbench command queued");
                }
            }
        }
    }
    (console_outputs, saved_document)
}

fn node_from_slug(slug: &str) -> Option<raf_nodes::Node> {
    Some(match slug {
        "on-start" => raf_nodes::Node::on_start(),
        "on-update" => raf_nodes::Node::on_update(),
        "print" => raf_nodes::Node::print_action(),
        "if" => raf_nodes::Node::if_branch(),
        "add" => raf_nodes::Node::add_math(),
        "for-loop" => raf_nodes::flow_nodes::FlowNodes::for_loop(),
        "while-loop" => raf_nodes::flow_nodes::FlowNodes::while_loop(),
        "greater-than" => raf_nodes::math_nodes::MathNodes::compare(">"),
        "less-than" => raf_nodes::math_nodes::MathNodes::compare("<"),
        "equals" => raf_nodes::math_nodes::MathNodes::compare("=="),
        "spawn-entity" => raf_nodes::entity_nodes::EntityNodes::spawn_entity(),
        "destroy-entity" => raf_nodes::entity_nodes::EntityNodes::destroy_entity(),
        "set-position" => raf_nodes::entity_nodes::EntityNodes::set_position(),
        "key-press" => raf_nodes::input_nodes::InputNodes::key_press(),
        "mouse-click" => raf_nodes::input_nodes::InputNodes::mouse_click(),
        "delay" => raf_nodes::input_nodes::InputNodes::timer_delay(),
        _ => return None,
    })
}

fn sanitize_asset_name(value: &str) -> String {
    let value = value.trim();
    let sanitized: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(64)
        .collect();
    if sanitized.is_empty() {
        "new_asset".to_string()
    } else {
        sanitized
    }
}

fn create_asset_script(
    project_root: &std::path::Path,
    language: &str,
    name: &str,
) -> Result<(), String> {
    let (folder, extension, body) = match language {
        "rust" => (
            "scripts",
            "rs",
            "// AuraRafi script\npub fn on_update(delta_seconds: f32) { let _ = delta_seconds; }\n",
        ),
        "cpp" => (
            "scripts",
            "cpp",
            "// AuraRafi script\nvoid on_update(float delta_seconds) { (void)delta_seconds; }\n",
        ),
        _ => (
            "scripts",
            "rhai",
            "// AuraRafi script\nfn on_update(delta_seconds) { let _ = delta_seconds; }\n",
        ),
    };
    let directory = project_root.join("assets").join(folder);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{name}.{extension}"));
    if path.exists() {
        return Err(format!("asset already exists: {}", path.display()));
    }
    std::fs::write(path, body).map_err(|error| error.to_string())
}

pub(crate) fn parse_primitive_slug(value: &str) -> Option<Primitive> {
    match value.trim().to_ascii_lowercase().as_str() {
        "empty" | "entity" => Some(Primitive::Empty),
        "cube" | "box" | "block" => Some(Primitive::Cube),
        "sphere" | "ball" => Some(Primitive::Sphere),
        "plane" | "floor" => Some(Primitive::Plane),
        "cylinder" => Some(Primitive::Cylinder),
        _ => None,
    }
}
