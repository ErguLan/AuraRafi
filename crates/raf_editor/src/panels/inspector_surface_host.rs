//! Host/controller for the retained Inspector surface.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use eframe::{egui, egui_wgpu};
use glam::Vec3;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::{
    ColliderType, NodeColor, Primitive, RigidBodyType, SceneGraph, SceneNodeId, VariableValue,
};
use raf_core::session::{ProjectSessionRegistry, SessionId};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiControlState, UiDispatchedAction, UiSurface,
};
use raf_ui::{UiMotionSpec, UiTween};
use uuid::Uuid;

use super::inspector_surface::{
    build_inspector_surface, InspectorDropdown, InspectorSection, InspectorTab, InspectorViewState,
};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

const NAME_ID: &str = "inspector.name";

#[derive(Debug, Clone, PartialEq)]
pub enum InspectorIntent {
    Rename {
        id: SceneNodeId,
        name: String,
    },
    ToggleVisibility(SceneNodeId),
    ToggleLocked(SceneNodeId),
    SetPrimitive {
        id: SceneNodeId,
        primitive: Primitive,
    },
    SetColor {
        id: SceneNodeId,
        color: NodeColor,
    },
    SetAudioEnabled {
        id: SceneNodeId,
        enabled: bool,
    },
    SetAudioClip {
        id: SceneNodeId,
        clip: String,
    },
    SetAudioAutoplay {
        id: SceneNodeId,
        autoplay: bool,
    },
    SetAudioLooping {
        id: SceneNodeId,
        looping: bool,
    },
    SetAudioVolume {
        id: SceneNodeId,
        volume: f32,
    },
    SetRigidBodyEnabled {
        id: SceneNodeId,
        enabled: bool,
    },
    SetColliderType {
        id: SceneNodeId,
        collider_type: ColliderType,
    },
    SetRigidBodyType {
        id: SceneNodeId,
        body_type: RigidBodyType,
    },
    SetGravity {
        id: SceneNodeId,
        enabled: bool,
    },
    SetTrigger {
        id: SceneNodeId,
        enabled: bool,
    },
    SetDamping {
        id: SceneNodeId,
        damping: f32,
    },
    SetVelocity {
        id: SceneNodeId,
        velocity: Vec3,
    },
    SetVariableName {
        id: SceneNodeId,
        index: usize,
        name: String,
    },
    SetVariableValue {
        id: SceneNodeId,
        index: usize,
        value: String,
    },
    AddVariable(SceneNodeId),
    RemoveVariable {
        id: SceneNodeId,
        index: usize,
    },
    CycleVariableType {
        id: SceneNodeId,
        index: usize,
    },
    EndGesture,
    SetTransform {
        id: SceneNodeId,
        position: Vec3,
        rotation: Vec3,
        scale: Vec3,
    },
    ResetTransform(SceneNodeId),
    ResetAll(SceneNodeId),
    SelectTab(InspectorTab),
    CreateSession {
        name: String,
    },
    OpenSession(SessionId),
    DuplicateSession {
        source: SessionId,
        name: String,
    },
    RemoveSession(SessionId),
    TogglePanel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SurfaceKey {
    dark: bool,
    selected: Option<SceneNodeId>,
    scene_hash: u64,
    transition_bits: u32,
    sessions_hash: u64,
    view: InspectorViewState,
}

pub struct InspectorSurfaceHost {
    bridge: RafUiSurfaceBridge,
    cached_key: Option<SurfaceKey>,
    cached_surface: Option<UiSurface>,
    name_value: Option<(SceneNodeId, String)>,
    panel_motion: UiTween,
    last_motion_time: f64,
    closing: bool,
    view: InspectorViewState,
}

impl Default for InspectorSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_inspector"),
            cached_key: None,
            cached_surface: None,
            name_value: None,
            panel_motion: UiTween::new(0.0, UiMotionSpec::layout()),
            last_motion_time: 0.0,
            closing: false,
            view: InspectorViewState::default(),
        }
    }
}

impl InspectorSurfaceHost {
    pub fn reset_for_scene(&mut self) {
        self.cached_key = None;
        self.cached_surface = None;
        self.name_value = None;
        self.panel_motion.set_immediate(0.0);
        self.last_motion_time = 0.0;
        self.closing = false;
        self.view = InspectorViewState::default();
        self.bridge.reset_surface_interaction(None);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        scene: &SceneGraph,
        sessions: &ProjectSessionRegistry,
        selected: &[SceneNodeId],
        animations_enabled: bool,
    ) -> Vec<InspectorIntent> {
        let now = ui.ctx().input(|input| input.time);
        let delta = if self.last_motion_time <= 0.0 {
            0.0
        } else {
            (now - self.last_motion_time).clamp(0.0, 0.1) as f32
        };
        self.last_motion_time = now;
        self.panel_motion
            .set_target(if self.closing { 0.0 } else { 1.0 });
        let transition = self.panel_motion.advance(delta, !animations_enabled);
        if animations_enabled && !self.panel_motion.is_settled() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        let selected_id = selected
            .first()
            .copied()
            .filter(|id| scene.is_valid_node(*id));
        let scene_hash = selected_id
            .and_then(|id| scene.get(id))
            .map(|node| scene_node_surface_hash(scene, node))
            .unwrap_or(0);
        let sessions_hash = session_registry_hash(sessions);
        let key = SurfaceKey {
            dark: matches!(palette, StudioUiPalette::IndustrialDark),
            selected: selected_id,
            scene_hash,
            transition_bits: transition.to_bits(),
            sessions_hash,
            view: self.view,
        };
        if self.cached_key.as_ref() != Some(&key) {
            self.cached_surface = Some(build_inspector_surface(
                palette,
                scene,
                selected_id,
                sessions,
                transition,
                self.view,
            ));
            self.cached_key = Some(key);
        }
        if let Some(id) = selected_id {
            let current_name = scene
                .get(id)
                .map(|node| node.name.clone())
                .unwrap_or_default();
            if self
                .name_value
                .as_ref()
                .is_none_or(|(current_id, value)| *current_id != id || value != &current_name)
            {
                self.name_value = Some((id, current_name));
            }
        } else {
            self.name_value = None;
        }
        let Some(surface) = self.cached_surface.as_ref() else {
            return Vec::new();
        };
        let name = self.name_value.clone();
        let focused_control = self.bridge.focused_control_id().map(str::to_string);
        let default_session_name = next_session_name(sessions);
        let dynamic_values = selected_id.and_then(|id| scene.get(id)).map(|node| {
            (
                node.variables.clone(),
                node.audio_source.clip.clone(),
                node.position,
                node.rotation,
                node.scale,
                node.rigid_body.velocity,
                node.color,
            )
        });
        let actions = self.bridge.show_with_control_state_ref(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                if let Some((_, value)) = name.as_ref() {
                    seed_text_if_changed(controls, NAME_ID, value, 256);
                }
                if focused_control.as_deref() != Some("inspector.session.new_name") {
                    seed_text_if_changed(
                        controls,
                        "inspector.session.new_name",
                        &default_session_name,
                        128,
                    );
                }
                if let Some((variables, clip, position, rotation, scale, velocity, color)) =
                    dynamic_values.as_ref()
                {
                    seed_text_if_changed(controls, "inspector.audio.clip", clip, 256);
                    for (index, variable) in variables.iter().enumerate() {
                        seed_text_if_changed(
                            controls,
                            &format!("inspector.variable.{index}.name"),
                            &variable.name,
                            256,
                        );
                        seed_text_if_changed(
                            controls,
                            &format!("inspector.variable.{index}.value"),
                            &variable_value_text(&variable.value),
                            256,
                        );
                    }
                    for (field, vector) in [
                        ("position", *position),
                        ("rotation", *rotation),
                        ("scale", *scale),
                        ("velocity", *velocity),
                    ] {
                        for (axis, value) in [("x", vector.x), ("y", vector.y), ("z", vector.z)] {
                            let key = numeric_text_key(field, axis);
                            let control_id = numeric_control_id(field, axis);
                            if focused_control.as_deref() != Some(control_id.as_str()) {
                                seed_text_if_changed(controls, &key, &format_number(value), 24);
                            }
                        }
                    }
                    let hex = format!(
                        "#{:02X}{:02X}{:02X}{:02X}",
                        color.r, color.g, color.b, color.a
                    );
                    if focused_control.as_deref() != Some("inspector.color.hex.input") {
                        seed_text_if_changed(controls, "inspector.color.hex", &hex, 9);
                    }
                }
            },
            |key| t(key, language),
        );
        let mut intents = self.apply_actions(actions, scene, sessions, selected_id);
        if self.closing && animations_enabled && !self.panel_motion.is_settled() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if self.closing && self.panel_motion.is_settled() {
            self.closing = false;
            intents.push(InspectorIntent::TogglePanel);
        }
        intents
    }

    fn apply_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        scene: &SceneGraph,
        sessions: &ProjectSessionRegistry,
        selected: Option<SceneNodeId>,
    ) -> Vec<InspectorIntent> {
        let mut intents = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == NAME_ID => {
                    if let Some(id) = selected {
                        self.name_value = Some((id, value));
                    }
                }
                UiAction::SetText { key, value } => {
                    if let Some(id) = selected {
                        if key == "inspector.color.hex" {
                            if let Some(color) = parse_hex_color(&value) {
                                intents.push(InspectorIntent::SetColor { id, color });
                            }
                        } else if let Some((field, axis)) = parse_numeric_text_key(&key) {
                            if let Ok(value) = value.trim().parse::<f32>() {
                                if value.is_finite() {
                                    push_vector_intent(&mut intents, scene, id, field, axis, value);
                                }
                            }
                        } else if let Some(index) = parse_index_key(&key, "name") {
                            intents.push(InspectorIntent::SetVariableName {
                                id,
                                index,
                                name: value,
                            });
                        } else if let Some(index) = parse_index_key(&key, "value") {
                            intents.push(InspectorIntent::SetVariableValue { id, index, value });
                        } else if key == "inspector.audio.clip" {
                            intents.push(InspectorIntent::SetAudioClip { id, clip: value });
                        }
                    }
                }
                UiAction::SetToggle { key, value } => {
                    if let Some(id) = selected {
                        match key.as_str() {
                            "inspector.audio.enabled" => intents
                                .push(InspectorIntent::SetAudioEnabled { id, enabled: value }),
                            "inspector.audio.autoplay" => {
                                intents.push(InspectorIntent::SetAudioAutoplay {
                                    id,
                                    autoplay: value,
                                })
                            }
                            "inspector.audio.looping" => intents
                                .push(InspectorIntent::SetAudioLooping { id, looping: value }),
                            "inspector.physics.enabled" => intents
                                .push(InspectorIntent::SetRigidBodyEnabled { id, enabled: value }),
                            "inspector.physics.gravity" => {
                                intents.push(InspectorIntent::SetGravity { id, enabled: value })
                            }
                            "inspector.physics.trigger" => {
                                intents.push(InspectorIntent::SetTrigger { id, enabled: value })
                            }
                            _ => {}
                        }
                    }
                }
                UiAction::SetRange { key, value } => {
                    let Some(id) = selected else { continue };
                    if let Some((field, axis)) = parse_range_key(&key) {
                        let Some(node) = scene.get(id) else { continue };
                        let mut position = node.position;
                        let mut rotation = node.rotation;
                        let mut scale = node.scale;
                        match field {
                            "position" | "rotation" | "scale" => {
                                let vector = match field {
                                    "position" => &mut position,
                                    "rotation" => &mut rotation,
                                    _ => &mut scale,
                                };
                                set_axis(vector, axis, value);
                                intents.push(InspectorIntent::SetTransform {
                                    id,
                                    position,
                                    rotation,
                                    scale,
                                });
                            }
                            "velocity" => {
                                let mut velocity = node.rigid_body.velocity;
                                set_axis(&mut velocity, axis, value);
                                intents.push(InspectorIntent::SetVelocity { id, velocity });
                            }
                            _ => {}
                        }
                    } else if let Some(channel) = parse_color_key(&key) {
                        if let Some(node) = scene.get(id) {
                            let mut color = node.color;
                            set_color_channel(&mut color, channel, value);
                            intents.push(InspectorIntent::SetColor { id, color });
                        }
                    } else if key == "inspector.audio.volume" {
                        intents.push(InspectorIntent::SetAudioVolume {
                            id,
                            volume: value.clamp(0.0, 1.0),
                        });
                    } else if key == "inspector.physics.damping" {
                        intents.push(InspectorIntent::SetDamping {
                            id,
                            damping: value.clamp(0.0, 1.0),
                        });
                    }
                }
                UiAction::Command { name } => {
                    self.apply_command(&name, scene, sessions, selected, &mut intents);
                }
                _ => {}
            }
        }
        intents
    }

    fn apply_command(
        &mut self,
        command: &str,
        scene: &SceneGraph,
        sessions: &ProjectSessionRegistry,
        selected: Option<SceneNodeId>,
        intents: &mut Vec<InspectorIntent>,
    ) {
        if command == "inspector.panel.toggle" {
            self.closing = true;
            return;
        }
        match command {
            "inspector.tab:properties" => {
                self.view.tab = InspectorTab::Properties;
                self.invalidate_surface();
                intents.push(InspectorIntent::SelectTab(InspectorTab::Properties));
                return;
            }
            "inspector.tab:sessions" => {
                self.view.tab = InspectorTab::Sessions;
                self.invalidate_surface();
                intents.push(InspectorIntent::SelectTab(InspectorTab::Sessions));
                return;
            }
            _ => {}
        }
        if command == "inspector.session.create" {
            let name = self
                .bridge
                .with_control_state_read(|controls| {
                    controls.text("inspector.session.new_name").to_string()
                })
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| next_session_name(sessions));
            intents.push(InspectorIntent::CreateSession { name });
            return;
        }
        if let Some(id) = command
            .strip_prefix("inspector.session.open:")
            .and_then(parse_session_id)
        {
            intents.push(InspectorIntent::OpenSession(id));
            return;
        }
        if let Some(id) = command
            .strip_prefix("inspector.session.duplicate:")
            .and_then(parse_session_id)
        {
            intents.push(InspectorIntent::DuplicateSession {
                source: id,
                name: next_session_name(sessions),
            });
            return;
        }
        if let Some(id) = command
            .strip_prefix("inspector.session.remove:")
            .and_then(parse_session_id)
        {
            intents.push(InspectorIntent::RemoveSession(id));
            return;
        }
        if let Some(slug) = command.strip_prefix("inspector.section.toggle:") {
            if let Some(section) = inspector_section_from_slug(slug) {
                self.toggle_section(section);
                self.invalidate_surface();
            }
            return;
        }
        if let Some(slug) = command.strip_prefix("inspector.dropdown.toggle:") {
            if let Some(dropdown) = inspector_dropdown_from_slug(slug) {
                self.view.dropdown = if self.view.dropdown == Some(dropdown) {
                    None
                } else {
                    Some(dropdown)
                };
                self.invalidate_surface();
            }
            return;
        }
        if command == "inspector.color.toggle" {
            self.view.color_picker = !self.view.color_picker;
            self.view.dropdown = None;
            self.invalidate_surface();
            return;
        }
        if command == "inspector.color.hex.commit" {
            if let Some(id) = selected {
                if let Some(value) = self.bridge.with_control_state_read(|controls| {
                    controls.text("inspector.color.hex").to_string()
                }) {
                    if let Some(color) = parse_hex_color(&value) {
                        intents.push(InspectorIntent::SetColor { id, color });
                    }
                }
            }
            intents.push(InspectorIntent::EndGesture);
            return;
        }
        if let Some(value) = command.strip_prefix("inspector.color.preset:") {
            if let (Some(id), Some(color)) = (selected, parse_hex_color(value)) {
                intents.push(InspectorIntent::SetColor { id, color });
            }
            return;
        }
        if command == "inspector.range.end" {
            intents.push(InspectorIntent::EndGesture);
            return;
        }
        if command == "inspector.numeric.commit" {
            intents.push(InspectorIntent::EndGesture);
            return;
        }
        let Some(id) = selected else { return };
        if let Some(name) = command
            .strip_prefix("inspector.rename.commit:")
            .and_then(parse_id)
        {
            if name == id {
                if let Some((_, value)) = self.name_value.take() {
                    if !value.trim().is_empty() {
                        intents.push(InspectorIntent::Rename {
                            id,
                            name: value.trim().to_string(),
                        });
                    }
                }
            }
            return;
        }
        if command == format!("inspector.visibility:{}", id.0) {
            intents.push(InspectorIntent::ToggleVisibility(id));
        } else if command == format!("inspector.lock:{}", id.0) {
            intents.push(InspectorIntent::ToggleLocked(id));
        } else if command == format!("inspector.reset-transform:{}", id.0) {
            intents.push(InspectorIntent::ResetTransform(id));
        } else if command == format!("inspector.reset-all:{}", id.0) {
            intents.push(InspectorIntent::ResetAll(id));
        } else if let Some(label) = command.strip_prefix(&format!("inspector.primitive:{}:", id.0))
        {
            if let Some(primitive) = primitive_from_label(label) {
                intents.push(InspectorIntent::SetPrimitive { id, primitive });
                self.view.dropdown = None;
                self.invalidate_surface();
            }
        } else if command == format!("inspector.variable.add:{}", id.0) {
            intents.push(InspectorIntent::AddVariable(id));
        } else if let Some(index) = command
            .strip_prefix(&format!("inspector.variable.remove:{}:", id.0))
            .and_then(|value| value.parse::<usize>().ok())
        {
            intents.push(InspectorIntent::RemoveVariable { id, index });
        } else if let Some(index) = command
            .strip_prefix(&format!("inspector.variable.type:{}:", id.0))
            .and_then(|value| value.parse::<usize>().ok())
        {
            intents.push(InspectorIntent::CycleVariableType { id, index });
        } else if let Some(label) = command.strip_prefix(&format!("inspector.collider:{}:", id.0)) {
            if let Some(collider_type) = collider_type_from_label(label) {
                intents.push(InspectorIntent::SetColliderType { id, collider_type });
                self.view.dropdown = None;
                self.invalidate_surface();
            }
        } else if let Some(label) = command.strip_prefix(&format!("inspector.body-type:{}:", id.0))
        {
            if let Some(body_type) = body_type_from_label(label) {
                intents.push(InspectorIntent::SetRigidBodyType { id, body_type });
                self.view.dropdown = None;
                self.invalidate_surface();
            }
        }
        let _ = scene;
    }

    fn invalidate_surface(&mut self) {
        self.cached_key = None;
        self.cached_surface = None;
    }

    fn toggle_section(&mut self, section: InspectorSection) {
        let value = !self.view.section(section);
        match section {
            InspectorSection::Identity => self.view.identity = value,
            InspectorSection::Transform => self.view.transform = value,
            InspectorSection::Appearance => self.view.appearance = value,
            InspectorSection::Components => self.view.components = value,
            InspectorSection::Variables => self.view.variables = value,
            InspectorSection::Audio => self.view.audio = value,
            InspectorSection::Physics => self.view.physics = value,
            InspectorSection::Metadata => self.view.metadata = value,
            InspectorSection::Debug => self.view.debug = value,
        }
        self.view.dropdown = None;
        self.view.color_picker = false;
    }
}

fn parse_session_id(value: &str) -> Option<SessionId> {
    Uuid::parse_str(value).ok().map(SessionId)
}

fn next_session_name(registry: &ProjectSessionRegistry) -> String {
    let mut index = registry.sessions.len() + 1;
    loop {
        let candidate = format!("Session_{index}");
        if !registry
            .sessions
            .iter()
            .any(|session| session.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        index += 1;
    }
}

fn session_registry_hash(registry: &ProjectSessionRegistry) -> u64 {
    let mut hasher = DefaultHasher::new();
    registry.active_session.hash(&mut hasher);
    for session in &registry.sessions {
        session.id.hash(&mut hasher);
        session.name.hash(&mut hasher);
        format!("{:?}", session.kind).hash(&mut hasher);
    }
    hasher.finish()
}

fn seed_text_if_changed(controls: &mut UiControlState, key: &str, value: &str, max_length: usize) {
    if !controls.has_text(key) || controls.text(key) != value {
        controls.set_text(key, value, max_length);
    }
}

fn parse_id(value: &str) -> Option<SceneNodeId> {
    value.parse::<usize>().ok().map(SceneNodeId)
}

fn parse_range_key(key: &str) -> Option<(&str, usize)> {
    let mut pieces = key.strip_prefix("inspector.")?.split('.');
    let field = pieces.next()?;
    let axis = match pieces.next()? {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => return None,
    };
    Some((field, axis))
}

fn parse_numeric_text_key(key: &str) -> Option<(&str, usize)> {
    let mut pieces = key.strip_prefix("inspector.")?.split('.');
    let field = pieces.next()?;
    let axis = match pieces.next()? {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => return None,
    };
    (pieces.next()? == "text" && pieces.next().is_none()).then_some((field, axis))
}

fn numeric_text_key(field: &str, axis: &str) -> String {
    format!("inspector.{field}.{axis}.text")
}

fn numeric_control_id(field: &str, axis: &str) -> String {
    format!("inspector.{field}.{axis}.input")
}

fn format_number(value: f32) -> String {
    let mut value = format!("{value:.4}");
    while value.contains('.') && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    if value == "-0" {
        value = "0".to_string();
    }
    value
}

fn push_vector_intent(
    intents: &mut Vec<InspectorIntent>,
    scene: &SceneGraph,
    id: SceneNodeId,
    field: &str,
    axis: usize,
    value: f32,
) {
    let Some(node) = scene.get(id) else { return };
    match field {
        "position" | "rotation" | "scale" => {
            let mut position = node.position;
            let mut rotation = node.rotation;
            let mut scale = node.scale;
            let vector = match field {
                "position" => &mut position,
                "rotation" => &mut rotation,
                _ => &mut scale,
            };
            set_axis(vector, axis, value);
            intents.push(InspectorIntent::SetTransform {
                id,
                position,
                rotation,
                scale,
            });
        }
        "velocity" => {
            let mut velocity = node.rigid_body.velocity;
            set_axis(&mut velocity, axis, value);
            intents.push(InspectorIntent::SetVelocity { id, velocity });
        }
        _ => {}
    }
}

fn parse_index_key(key: &str, field: &str) -> Option<usize> {
    let value = key.strip_prefix("inspector.variable.")?;
    let mut pieces = value.split('.');
    let index = pieces.next()?.parse::<usize>().ok()?;
    (pieces.next()? == field).then_some(index)
}

fn parse_color_key(key: &str) -> Option<usize> {
    match key.strip_prefix("inspector.color.")? {
        "r" => Some(0),
        "g" => Some(1),
        "b" => Some(2),
        "a" => Some(3),
        _ => None,
    }
}

fn parse_hex_color(value: &str) -> Option<NodeColor> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if value.len() != 6 && value.len() != 8 {
        return None;
    }
    let channel = |start| u8::from_str_radix(&value[start..start + 2], 16).ok();
    Some(NodeColor {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
        a: if value.len() == 8 { channel(6)? } else { 255 },
    })
}

fn inspector_section_from_slug(slug: &str) -> Option<InspectorSection> {
    match slug {
        "identity" => Some(InspectorSection::Identity),
        "transform" => Some(InspectorSection::Transform),
        "appearance" => Some(InspectorSection::Appearance),
        "components" => Some(InspectorSection::Components),
        "variables" => Some(InspectorSection::Variables),
        "audio" => Some(InspectorSection::Audio),
        "physics" => Some(InspectorSection::Physics),
        "metadata" => Some(InspectorSection::Metadata),
        "debug" => Some(InspectorSection::Debug),
        _ => None,
    }
}

fn inspector_dropdown_from_slug(slug: &str) -> Option<InspectorDropdown> {
    match slug {
        "primitive" => Some(InspectorDropdown::Primitive),
        "collider" => Some(InspectorDropdown::Collider),
        "body-type" => Some(InspectorDropdown::BodyType),
        _ => None,
    }
}

fn set_color_channel(color: &mut NodeColor, channel: usize, value: f32) {
    let value = value.round().clamp(0.0, 255.0) as u8;
    match channel {
        0 => color.r = value,
        1 => color.g = value,
        2 => color.b = value,
        _ => color.a = value,
    }
}

fn set_axis(vector: &mut Vec3, axis: usize, value: f32) {
    match axis {
        0 => vector.x = value,
        1 => vector.y = value,
        _ => vector.z = value,
    }
}

fn primitive_from_label(label: &str) -> Option<Primitive> {
    match label {
        "Empty" => Some(Primitive::Empty),
        "Cube" => Some(Primitive::Cube),
        "Sphere" => Some(Primitive::Sphere),
        "Plane" => Some(Primitive::Plane),
        "Cylinder" => Some(Primitive::Cylinder),
        _ => None,
    }
}

fn variable_value_text(value: &VariableValue) -> String {
    match value {
        VariableValue::Bool(value) => value.to_string(),
        VariableValue::Number(value) => value.to_string(),
        VariableValue::Text(value) => value.clone(),
    }
}

fn scene_node_surface_hash(scene: &SceneGraph, node: &raf_core::scene::SceneNode) -> u64 {
    let mut hasher = DefaultHasher::new();
    node.uuid.hash(&mut hasher);
    node.name.hash(&mut hasher);
    node.parent.hash(&mut hasher);
    scene
        .get(node.parent.unwrap_or(SceneNodeId(usize::MAX)))
        .map(|parent| parent.name.as_str())
        .unwrap_or("")
        .hash(&mut hasher);
    node.children.hash(&mut hasher);
    node.visible.hash(&mut hasher);
    node.locked.hash(&mut hasher);
    node.entity_index.hash(&mut hasher);
    node.is_folder.hash(&mut hasher);
    node.source_asset.hash(&mut hasher);
    node.source_schema_version.hash(&mut hasher);
    node.primitive.label().hash(&mut hasher);
    for value in [
        node.position.x,
        node.position.y,
        node.position.z,
        node.rotation.x,
        node.rotation.y,
        node.rotation.z,
        node.scale.x,
        node.scale.y,
        node.scale.z,
        node.rigid_body.velocity.x,
        node.rigid_body.velocity.y,
        node.rigid_body.velocity.z,
    ] {
        value.to_bits().hash(&mut hasher);
    }
    [node.color.r, node.color.g, node.color.b, node.color.a].hash(&mut hasher);
    node.variables.len().hash(&mut hasher);
    for variable in &node.variables {
        variable.name.hash(&mut hasher);
        match &variable.value {
            VariableValue::Bool(value) => {
                0_u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
            VariableValue::Number(value) => {
                1_u8.hash(&mut hasher);
                value.to_bits().hash(&mut hasher);
            }
            VariableValue::Text(value) => {
                2_u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
        }
    }
    node.audio_source.enabled.hash(&mut hasher);
    node.audio_source.clip.hash(&mut hasher);
    node.audio_source.autoplay.hash(&mut hasher);
    node.audio_source.looping.hash(&mut hasher);
    node.audio_source.volume.to_bits().hash(&mut hasher);
    node.rigid_body.enabled.hash(&mut hasher);
    node.rigid_body.use_gravity.hash(&mut hasher);
    node.rigid_body.is_trigger.hash(&mut hasher);
    node.rigid_body.damping.to_bits().hash(&mut hasher);
    collider_type_key(node.collider.collider_type).hash(&mut hasher);
    node.scripts.len().hash(&mut hasher);
    hasher.finish()
}

fn collider_type_key(collider_type: ColliderType) -> &'static str {
    match collider_type {
        ColliderType::None => "none",
        ColliderType::Aabb => "aabb",
        ColliderType::ConvexHull => "convex_hull",
        ColliderType::MeshCollider => "mesh_collider",
    }
}

fn collider_type_from_label(label: &str) -> Option<ColliderType> {
    match label {
        "None" => Some(ColliderType::None),
        "Aabb" => Some(ColliderType::Aabb),
        "ConvexHull" => Some(ColliderType::ConvexHull),
        "MeshCollider" => Some(ColliderType::MeshCollider),
        _ => None,
    }
}

fn body_type_from_label(label: &str) -> Option<RigidBodyType> {
    match label {
        "Static" => Some(RigidBodyType::Static),
        "Dynamic" => Some(RigidBodyType::Dynamic),
        "Kinematic" => Some(RigidBodyType::Kinematic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_accepts_rgb_and_rgba() {
        assert_eq!(
            parse_hex_color("#EC7E14"),
            Some(NodeColor::rgba(236, 126, 20, 255))
        );
        assert_eq!(
            parse_hex_color("487ED680"),
            Some(NodeColor::rgba(72, 126, 214, 128))
        );
        assert_eq!(parse_hex_color("#BAD"), None);
    }

    #[test]
    fn inspector_sections_start_compact_below_components() {
        let view = InspectorViewState::default();
        assert!(view.identity && view.transform && view.appearance && view.components);
        assert!(!view.variables && !view.audio && !view.physics);
        assert!(!view.metadata && !view.debug && !view.color_picker);
    }
}
