//! Transient workbench state and translation to the canonical Lighting commands.
use super::*;
use crate::panels::lighting_surface::{self, value_key};
use raf_core::environment::{
    environment_presets, environment_property, EnvironmentPropertyKind, EnvironmentSettings,
    ENVIRONMENT_PROPERTIES,
};
use raf_ui::UiColorPicker;
use serde_json::{json, Map, Value};
use std::f32::consts::{FRAC_PI_2, TAU};

const COLOR_ACTION_PREFIX: &str = "lighting.color.";

impl NativeGameWorkbench {
    pub(super) fn seed_lighting_values(&mut self, scene: &SceneGraph) {
        let env = *scene.environment();
        let focused = self.host.session().interaction.focus.focused.clone();
        let controls = &mut self.host.session_mut().interaction.controls;
        for p in ENVIRONMENT_PROPERTIES {
            match p.kind {
                EnvironmentPropertyKind::Number { .. } => {
                    let key = value_key(p.path);
                    if focused.as_deref() != Some(key.as_str()) {
                        controls.set_text(key, lighting_surface::display_value(&env, p.path), 32);
                    }
                }
                EnvironmentPropertyKind::Color => {
                    let key = color_hex_key(p.path);
                    if focused.as_deref() != Some(key.as_str()) {
                        let value = env.get_property(p.path).ok();
                        if let Some(color) = value.as_ref().and_then(value_rgb) {
                            controls.set_text(key, color_hex(color), 9);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Drains host-owned char gestures and viewport-eyedropper samples into
    /// the same intent stream as normal retained UI actions.
    pub(super) fn flush_lighting_session(
        &mut self,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        let focused = self.host.session().interaction.focus.focused.clone();
        if let Some((_, focus_key)) = self.lighting_text_gesture.clone() {
            if focused.as_deref() != Some(focus_key.as_str()) {
                intents.push(NativeWorkbenchIntent::EnvironmentGesture { active: false });
                self.lighting_text_gesture = None;
            }
        }
        if let Some((path, color)) = self.lighting_color_sample.take() {
            self.end_text_gesture(intents);
            self.end_environment_gesture(intents);
            let mut patch = Map::new();
            patch.insert(path.clone(), json!(color));
            let _ = self.queue_lighting_patch(scene, patch, intents);
            self.remember_color(color);
            let key = color_hex_key(&path);
            self.host
                .session_mut()
                .interaction
                .controls
                .set_text(key, color_hex(color), 9);
        }
    }

    pub(crate) fn lighting_eyedropper_target(&self) -> Option<&str> {
        self.lighting_eyedropper.as_deref()
    }

    pub(crate) fn cancel_lighting_eyedropper(&mut self) {
        if self.lighting_eyedropper.take().is_some() {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }

    pub(crate) fn apply_lighting_color_sample(&mut self, color: [u8; 3]) {
        let Some(path) = self.lighting_eyedropper.take() else {
            return;
        };
        self.lighting_color_sample = Some((path, color));
        self.lighting_color_editor = None;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub(super) fn handle_lighting_action(
        &mut self,
        action: &UiAction,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        match action {
            UiAction::SetToggle { key, value } if key == "lighting.preview_neutral" => {
                self.lighting_preview_neutral = *value;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "viewport.lighting-neutral:{value}"
                )));
                return true;
            }
            UiAction::SetToggle { key, value } if key.starts_with("lighting.value.") => {
                if let Some(path) = key.strip_prefix("lighting.value.") {
                    self.end_text_gesture(intents);
                    self.end_environment_gesture(intents);
                    let mut patch = Map::new();
                    patch.insert(path.to_string(), json!(value));
                    self.queue_lighting_patch(scene, patch, intents);
                }
                return true;
            }
            UiAction::SetText { key, value } if key.starts_with("lighting.value.") => {
                if let Some(path) = key.strip_prefix("lighting.value.") {
                    if let Ok(next) = lighting_surface::parse_value_clamped(path, value) {
                        if scene.environment().get_property(path).ok().as_ref() != Some(&next) {
                            self.ensure_text_gesture(path, &value_key(path), intents);
                            let mut patch = Map::new();
                            patch.insert(path.to_string(), next);
                            self.queue_lighting_patch(scene, patch, intents);
                        }
                    }
                }
                return true;
            }
            UiAction::SetText { key, value }
                if key.starts_with(COLOR_ACTION_PREFIX) && key.ends_with(".hex") =>
            {
                if let Some(path) = color_path_from_hex_key(key) {
                    if let Ok(next) = lighting_surface::parse_value(path, value) {
                        if scene.environment().get_property(path).ok().as_ref() != Some(&next) {
                            self.ensure_text_gesture(path, key, intents);
                            let mut patch = Map::new();
                            patch.insert(path.to_string(), next);
                            if let Some(color) =
                                value_rgb(patch.get(path).expect("just inserted color value"))
                            {
                                self.remember_color(color);
                            }
                            self.queue_lighting_patch(scene, patch, intents);
                        }
                    }
                }
                return true;
            }
            UiAction::SetRange { key, value } if key.starts_with("lighting.value.") => {
                if let Some(path) = key.strip_prefix("lighting.value.") {
                    if let Ok(property) = environment_property(path) {
                        if let EnvironmentPropertyKind::Number { min, max, .. } = property.kind {
                            let mut patch = Map::new();
                            patch.insert(path.to_string(), json!(value.clamp(min, max)));
                            self.queue_lighting_patch(scene, patch, intents);
                        }
                    }
                }
                return true;
            }
            UiAction::SetColorHsv {
                key,
                hue,
                saturation,
                value,
            } if key.starts_with(COLOR_ACTION_PREFIX) => {
                if let Some(path) = key.strip_prefix(COLOR_ACTION_PREFIX) {
                    let rgb = UiColorPicker::hsv_to_rgb_bytes(*hue, *saturation, *value);
                    let mut patch = Map::new();
                    patch.insert(path.to_string(), json!(rgb));
                    self.queue_lighting_patch(scene, patch, intents);
                }
                return true;
            }
            UiAction::SetRange { key, value } if key.starts_with(COLOR_ACTION_PREFIX) => {
                if let Some((path, channel)) = color_channel_key(key) {
                    if let Ok(current) = scene.environment().get_property(path) {
                        if let Some(mut rgb) = value_rgb(&current) {
                            rgb[channel] = (*value).round().clamp(0.0, 255.0) as u8;
                            let mut patch = Map::new();
                            patch.insert(path.to_string(), json!(rgb));
                            self.queue_lighting_patch(scene, patch, intents);
                        }
                    }
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.section:") => {
                let section = name.strip_prefix("lighting.section:").unwrap_or_default();
                if lighting_surface::SECTIONS.contains(&section) {
                    self.lighting_section = section.into();
                    self.lighting_error = false;
                    self.lighting_color_editor = None;
                    self.lighting_eyedropper = None;
                    self.end_text_gesture(intents);
                    self.end_environment_gesture(intents);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.commit:") => {
                let path = name.strip_prefix("lighting.commit:").unwrap_or_default();
                let text = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text(&value_key(path))
                    .to_string();
                match lighting_surface::parse_value_clamped(path, &text) {
                    Ok(value) => {
                        let mut patch = Map::new();
                        patch.insert(path.to_string(), value);
                        self.queue_lighting_patch(scene, patch, intents);
                        self.end_text_gesture(intents);
                    }
                    Err(error) => {
                        self.lighting_error = true;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        tracing::debug!(%error, %path, "Invalid Lighting field");
                    }
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.choose:") => {
                let patch = name
                    .strip_prefix("lighting.choose:")
                    .and_then(|raw| raw.split_once(':'));
                if let Some((path, value)) = patch {
                    self.end_text_gesture(intents);
                    self.end_environment_gesture(intents);
                    let value = if path == "shadows.resolution" {
                        json!(value.parse::<u32>().unwrap_or(0))
                    } else {
                        json!(value)
                    };
                    let mut patch = Map::new();
                    patch.insert(path.to_string(), value);
                    self.queue_lighting_patch(scene, patch, intents);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.reset:") => {
                let path = name.strip_prefix("lighting.reset:").unwrap_or_default();
                if let Ok(value) = EnvironmentSettings::default().get_property(path) {
                    self.end_text_gesture(intents);
                    self.end_environment_gesture(intents);
                    let mut patch = Map::new();
                    patch.insert(path.to_string(), value);
                    self.queue_lighting_patch(scene, patch, intents);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.focus:") => {
                let path = name.strip_prefix("lighting.focus:").unwrap_or_default();
                self.host
                    .session_mut()
                    .interaction
                    .focus
                    .request_focus(value_key(path));
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.step:") => {
                if let Some(raw) = name.strip_prefix("lighting.step:") {
                    if let Some((path, direction)) = raw.rsplit_once(':') {
                        let direction = direction.parse::<f32>().unwrap_or(1.0);
                        self.step_lighting_value(scene, path, direction, intents);
                    }
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.scrub.begin:") => {
                let path = name
                    .strip_prefix("lighting.scrub.begin:")
                    .unwrap_or_default();
                self.begin_scrub(path, scene, intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.scrub.move:") => {
                let path = name
                    .strip_prefix("lighting.scrub.move:")
                    .unwrap_or_default();
                self.move_scrub(path, scene, intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.scrub.end:") => {
                self.lighting_scrub = None;
                self.end_environment_gesture(intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.gesture.begin:") => {
                self.end_text_gesture(intents);
                self.begin_environment_gesture(intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.gesture.end:") => {
                self.end_environment_gesture(intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.paste:") => {
                let path = name.strip_prefix("lighting.paste:").unwrap_or_default();
                self.paste_lighting_value(scene, path, intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.preset:") => {
                let id = name.strip_prefix("lighting.preset:").unwrap_or_default();
                if let Some(preset) = environment_presets().iter().find(|preset| preset.id == id) {
                    self.end_text_gesture(intents);
                    self.end_environment_gesture(intents);
                    intents.push(NativeWorkbenchIntent::ReplaceEnvironment {
                        settings: preset.settings,
                    });
                    self.lighting_section = "overview".to_string();
                    self.lighting_color_editor = None;
                    self.lighting_eyedropper = None;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.sky.edit:") => {
                let path = name.strip_prefix("lighting.sky.edit:").unwrap_or_default();
                if environment_property(path).is_ok() {
                    self.lighting_section = "sky".to_string();
                    self.lighting_color_editor = Some(path.to_string());
                    self.lighting_eyedropper = None;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.reset:") => {
                let path = name
                    .strip_prefix("lighting.color.reset:")
                    .unwrap_or_default();
                if let Ok(value) = EnvironmentSettings::default().get_property(path) {
                    self.end_text_gesture(intents);
                    self.end_environment_gesture(intents);
                    let mut patch = Map::new();
                    patch.insert(path.to_string(), value);
                    self.queue_lighting_patch(scene, patch, intents);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.toggle:") => {
                let path = name
                    .strip_prefix("lighting.color.toggle:")
                    .unwrap_or_default();
                self.toggle_color_editor(path, scene, intents);
                return true;
            }
            UiAction::Command { name }
                if name == "lighting.color.close" || name.starts_with("lighting.color.close:") =>
            {
                self.close_color_editor(scene, intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.hex.commit:") => {
                let path = name
                    .strip_prefix("lighting.color.hex.commit:")
                    .unwrap_or_default();
                let text = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text(&color_hex_key(path))
                    .to_string();
                if let Ok(value) = lighting_surface::parse_value(path, &text) {
                    let mut patch = Map::new();
                    patch.insert(path.to_string(), value);
                    self.queue_lighting_patch(scene, patch, intents);
                    self.end_text_gesture(intents);
                } else {
                    self.lighting_error = true;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.recent:") => {
                if let Some(raw) = name.strip_prefix("lighting.color.recent:") {
                    if let Some((path, index)) = raw.rsplit_once(':') {
                        if let Some(color) = index
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| self.lighting_recent_colors.get(index).copied())
                        {
                            self.end_text_gesture(intents);
                            self.end_environment_gesture(intents);
                            self.remember_color(color);
                            let mut patch = Map::new();
                            patch.insert(path.to_string(), json!(color));
                            self.queue_lighting_patch(scene, patch, intents);
                        }
                    }
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.paste:") => {
                let path = name
                    .strip_prefix("lighting.color.paste:")
                    .unwrap_or_default();
                self.paste_lighting_value(scene, path, intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.eyedropper:") => {
                let path = name
                    .strip_prefix("lighting.color.eyedropper:")
                    .unwrap_or_default();
                if self.lighting_eyedropper.as_deref() == Some(path) {
                    self.lighting_eyedropper = None;
                } else {
                    self.lighting_eyedropper = Some(path.to_string());
                    self.lighting_color_editor = None;
                }
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            UiAction::Command { name } if name == "lighting.eyedropper.cancel" => {
                self.lighting_eyedropper = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.gesture.begin:") => {
                self.end_text_gesture(intents);
                self.begin_environment_gesture(intents);
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.color.gesture.end:") => {
                self.end_environment_gesture(intents);
                return true;
            }
            UiAction::Command { name } if name == "lighting.sun.dial.press" => {
                self.end_text_gesture(intents);
                self.begin_environment_gesture(intents);
                self.sample_sun_dial(scene, intents);
                return true;
            }
            UiAction::Command { name } if name == "lighting.sun.dial.drag" => {
                self.sample_sun_dial(scene, intents);
                return true;
            }
            UiAction::Command { name } if name == "lighting.sun.dial.release" => {
                self.end_environment_gesture(intents);
                return true;
            }
            _ => return false,
        }
    }

    fn queue_lighting_patch(
        &mut self,
        scene: &SceneGraph,
        patch: Map<String, Value>,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if patch.is_empty() {
            return false;
        }
        let next = match scene.environment().patched(&patch) {
            Ok(next) => next,
            Err(error) => {
                self.lighting_error = true;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                tracing::debug!(%error, "Invalid Lighting patch");
                return false;
            }
        };
        self.lighting_error = false;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        for path in patch.keys() {
            self.sync_lighting_control(&next, path);
            if let Ok(value) = next.get_property(path) {
                if let Some(color) = value_rgb(&value) {
                    self.remember_color(color);
                }
            }
        }
        intents.push(NativeWorkbenchIntent::LightingPatch { patch });
        true
    }

    fn sync_lighting_control(&mut self, env: &EnvironmentSettings, path: &str) {
        let focused = self.host.session().interaction.focus.focused.clone();
        let controls = &mut self.host.session_mut().interaction.controls;
        match environment_property(path).map(|property| property.kind) {
            Ok(EnvironmentPropertyKind::Number { .. }) => {
                let key = value_key(path);
                if focused.as_deref() != Some(key.as_str()) {
                    controls.set_text(key, lighting_surface::display_value(env, path), 32);
                }
            }
            Ok(EnvironmentPropertyKind::Color) => {
                if let Ok(value) = env.get_property(path) {
                    if let Some(color) = value_rgb(&value) {
                        let key = color_hex_key(path);
                        if focused.as_deref() != Some(key.as_str()) {
                            controls.set_text(key, color_hex(color), 9);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn remember_color(&mut self, color: [u8; 3]) {
        self.lighting_recent_colors
            .retain(|existing| *existing != color);
        self.lighting_recent_colors.insert(0, color);
        self.lighting_recent_colors.truncate(8);
    }

    fn toggle_color_editor(
        &mut self,
        path: &str,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        if self.lighting_color_editor.as_deref() == Some(path) {
            self.close_color_editor(scene, intents);
            return;
        }
        self.lighting_color_editor = Some(path.to_string());
        self.lighting_eyedropper = None;
        if let Ok(value) = scene.environment().get_property(path) {
            if let Some(color) = value_rgb(&value) {
                self.remember_color(color);
            }
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    fn close_color_editor(&mut self, scene: &SceneGraph, intents: &mut Vec<NativeWorkbenchIntent>) {
        if let Some(path) = self.lighting_color_editor.take() {
            if let Ok(value) = scene.environment().get_property(&path) {
                if let Some(color) = value_rgb(&value) {
                    self.remember_color(color);
                }
            }
            self.end_text_gesture(intents);
            self.end_environment_gesture(intents);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }

    fn ensure_text_gesture(
        &mut self,
        path: &str,
        focus_key: &str,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        let next = (path.to_string(), focus_key.to_string());
        if self.lighting_text_gesture.as_ref() == Some(&next) {
            return;
        }
        if self.lighting_text_gesture.is_some() {
            intents.push(NativeWorkbenchIntent::EnvironmentGesture { active: false });
        }
        self.end_environment_gesture(intents);
        intents.push(NativeWorkbenchIntent::EnvironmentGesture { active: true });
        self.lighting_text_gesture = Some(next);
    }

    fn end_text_gesture(&mut self, intents: &mut Vec<NativeWorkbenchIntent>) {
        if self.lighting_text_gesture.take().is_some() {
            intents.push(NativeWorkbenchIntent::EnvironmentGesture { active: false });
        }
    }

    fn begin_environment_gesture(&mut self, intents: &mut Vec<NativeWorkbenchIntent>) {
        intents.push(NativeWorkbenchIntent::EnvironmentGesture { active: true });
    }

    fn end_environment_gesture(&mut self, intents: &mut Vec<NativeWorkbenchIntent>) {
        intents.push(NativeWorkbenchIntent::EnvironmentGesture { active: false });
    }

    fn step_lighting_value(
        &mut self,
        scene: &SceneGraph,
        path: &str,
        direction: f32,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        self.end_text_gesture(intents);
        self.end_environment_gesture(intents);
        let Ok(property) = environment_property(path) else {
            return;
        };
        let EnvironmentPropertyKind::Number {
            min,
            max,
            display_scale,
            step,
        } = property.kind
        else {
            return;
        };
        let current = scene
            .environment()
            .get_property(path)
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(min as f64) as f32;
        let stored_step = (step / display_scale.max(f32::EPSILON)).max(1e-6);
        let next = (current + stored_step * direction).clamp(min, max);
        let mut patch = Map::new();
        patch.insert(path.to_string(), json!(next));
        self.queue_lighting_patch(scene, patch, intents);
    }

    fn begin_scrub(
        &mut self,
        path: &str,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        let Some(pointer) = self.host.session().interaction.pointer_position() else {
            return;
        };
        let Ok(property) = environment_property(path) else {
            return;
        };
        if !matches!(property.kind, EnvironmentPropertyKind::Number { .. }) {
            return;
        }
        let current = scene
            .environment()
            .get_property(path)
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0) as f32;
        self.end_text_gesture(intents);
        self.begin_environment_gesture(intents);
        self.lighting_scrub = Some(LightingScrubState {
            path: path.to_string(),
            origin_pointer_x: pointer[0],
            start_value: current,
        });
    }

    fn move_scrub(
        &mut self,
        path: &str,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        let Some(scrub) = self.lighting_scrub.clone() else {
            return;
        };
        if scrub.path != path {
            return;
        }
        let Some(pointer) = self.host.session().interaction.pointer_position() else {
            return;
        };
        let Ok(property) = environment_property(path) else {
            return;
        };
        let EnvironmentPropertyKind::Number {
            min,
            max,
            display_scale,
            step,
        } = property.kind
        else {
            return;
        };
        let stored_step = (step / display_scale.max(f32::EPSILON)).max(1e-6);
        let raw = scrub.start_value + (pointer[0] - scrub.origin_pointer_x) * stored_step / 3.0;
        let next = ((raw / stored_step).round() * stored_step).clamp(min, max);
        let mut patch = Map::new();
        patch.insert(path.to_string(), json!(next));
        self.queue_lighting_patch(scene, patch, intents);
    }

    fn paste_lighting_value(
        &mut self,
        scene: &SceneGraph,
        path: &str,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        self.end_text_gesture(intents);
        self.end_environment_gesture(intents);
        let text = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }
        if let Ok(value) = lighting_surface::parse_value(path, &text) {
            let mut patch = Map::new();
            patch.insert(path.to_string(), value);
            self.queue_lighting_patch(scene, patch, intents);
        } else {
            self.lighting_error = true;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }

    fn sample_sun_dial(&mut self, scene: &SceneGraph, intents: &mut Vec<NativeWorkbenchIntent>) {
        let Some(rect) = self.host.layout_rect("lighting.sun.dial") else {
            return;
        };
        let Some(pointer) = self.host.session().interaction.pointer_position() else {
            return;
        };
        let pointer = [pointer[0] - self.rect.x, pointer[1] - self.rect.y];
        let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        let radius = (rect.width.min(rect.height) * 0.5 - 12.0).max(1.0);
        let dx = (pointer[0] - center[0]) / radius;
        let dy = (pointer[1] - center[1]) / radius;
        let (azimuth, elevation) = sun_dial_angles(dx, dy);
        let mut patch = Map::new();
        patch.insert("sun.azimuth".to_string(), json!(azimuth));
        patch.insert("sun.elevation".to_string(), json!(elevation));
        self.queue_lighting_patch(scene, patch, intents);
    }
}

fn sun_dial_angles(dx: f32, dy: f32) -> (f32, f32) {
    let distance = (dx * dx + dy * dy).sqrt().clamp(0.0, 1.0);
    let elevation = (FRAC_PI_2 * (1.0 - distance).sqrt()).clamp(0.0, FRAC_PI_2);
    let azimuth = dx.atan2(-dy).rem_euclid(TAU);
    (azimuth, elevation)
}

fn color_hex_key(path: &str) -> String {
    format!("{COLOR_ACTION_PREFIX}{path}.hex")
}

fn color_path_from_hex_key(key: &str) -> Option<&str> {
    key.strip_prefix(COLOR_ACTION_PREFIX)?.strip_suffix(".hex")
}

fn color_channel_key(key: &str) -> Option<(&str, usize)> {
    let rest = key.strip_prefix(COLOR_ACTION_PREFIX)?;
    let (path, channel) = rest.rsplit_once('.')?;
    let index = match channel {
        "r" => 0,
        "g" => 1,
        "b" => 2,
        _ => return None,
    };
    Some((path, index))
}

fn value_rgb(value: &Value) -> Option<[u8; 3]> {
    let array = value.as_array()?;
    if array.len() != 3 {
        return None;
    }
    Some([
        array[0].as_u64()? as u8,
        array[1].as_u64()? as u8,
        array[2].as_u64()? as u8,
    ])
}

fn color_hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_action_keys_round_trip_to_environment_paths() {
        let key = color_hex_key("sun.color");
        assert_eq!(key, "lighting.color.sun.color.hex");
        assert_eq!(color_path_from_hex_key(&key), Some("sun.color"));
        assert_eq!(
            color_channel_key("lighting.color.ambient_color.g"),
            Some(("ambient_color", 1))
        );
        assert_eq!(color_channel_key("lighting.color.ambient_color.hex"), None);
    }

    #[test]
    fn color_helpers_match_the_property_encoding() {
        assert_eq!(
            value_rgb(&serde_json::json!([255, 128, 64])),
            Some([255, 128, 64])
        );
        assert_eq!(value_rgb(&serde_json::json!([1, 2])), None);
        assert_eq!(color_hex([255, 128, 64]), "#FF8040");
        assert_eq!(
            color_hex_key("sky.top_color"),
            "lighting.color.sky.top_color.hex"
        );
    }

    #[test]
    fn sun_dial_maps_the_hemisphere_quadrants() {
        let (_, elevation) = sun_dial_angles(0.0, 0.0);
        assert!((elevation - FRAC_PI_2).abs() < 1e-5);
        let (azimuth_north, elevation_north) = sun_dial_angles(0.0, -1.0);
        assert!(azimuth_north.abs() < 1e-5);
        assert!(elevation_north.abs() < 1e-5);
        let (azimuth_east, _) = sun_dial_angles(1.0, 0.0);
        assert!((azimuth_east - FRAC_PI_2).abs() < 1e-5);
        let (azimuth_south, _) = sun_dial_angles(0.0, 1.0);
        assert!((azimuth_south - std::f32::consts::PI).abs() < 1e-5);
        let (azimuth_west, _) = sun_dial_angles(-1.0, 0.0);
        assert!((azimuth_west - (FRAC_PI_2 * 3.0)).abs() < 1e-5);
    }
}
