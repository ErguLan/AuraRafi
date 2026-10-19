//! Transient workbench state and translation to the canonical Lighting commands.
use super::*;
use crate::panels::lighting_surface::{self, value_key};
use serde_json::{json, Value};
impl NativeGameWorkbench {
    pub(super) fn seed_lighting_values(&mut self, scene: &SceneGraph) {
        let focused = self.host.session().interaction.focus.focused.clone();
        let controls = &mut self.host.session_mut().interaction.controls;
        for p in raf_core::environment::ENVIRONMENT_PROPERTIES {
            if !matches!(
                p.kind,
                raf_core::environment::EnvironmentPropertyKind::Number { .. }
                    | raf_core::environment::EnvironmentPropertyKind::Color
            ) {
                continue;
            }
            let key = value_key(p.path);
            if focused.as_deref() != Some(key.as_str()) || !controls.has_text(&key) {
                controls.set_text(
                    key,
                    lighting_surface::display_value(scene.environment(), p.path),
                    32,
                );
            }
        }
    }
    pub(super) fn handle_lighting_action(
        &mut self,
        action: &UiAction,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        let patch: Option<Result<(String, Value), String>> = match action {
            UiAction::SetToggle { key, value } if key == "lighting.preview_neutral" => {
                self.lighting_preview_neutral = *value;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "viewport.lighting-neutral:{value}"
                )));
                return true;
            }
            UiAction::SetToggle { key, value } if key.starts_with("lighting.value.") => {
                Some(Ok((key[15..].into(), json!(value))))
            }
            UiAction::Command { name } if name.starts_with("lighting.section:") => {
                let section = &name[17..];
                if lighting_surface::SECTIONS.contains(&section) {
                    self.lighting_section = section.into();
                    self.lighting_error = false;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("lighting.commit:") => {
                let path = &name[16..];
                let text = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text(&value_key(path));
                Some(lighting_surface::parse_value(path, text).map(|v| (path.into(), v)))
            }
            UiAction::Command { name } if name.starts_with("lighting.choose:") => Some(
                name[16..]
                    .split_once(':')
                    .map(|(p, v)| {
                        (
                            p.into(),
                            if p == "shadows.resolution" {
                                json!(v.parse::<u32>().unwrap_or(0))
                            } else {
                                json!(v)
                            },
                        )
                    })
                    .ok_or_else(|| "Invalid Lighting choice".into()),
            ),
            UiAction::Command { name } if name.starts_with("lighting.reset:") => {
                let path = &name[15..];
                Some(
                    raf_core::environment::EnvironmentSettings::default()
                        .get_property(path)
                        .map(|v| (path.into(), v)),
                )
            }
            _ => None,
        };
        let Some(patch) = patch else {
            return false;
        };
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        match patch.and_then(|(p, v)| {
            scene
                .environment()
                .patched(&serde_json::Map::from_iter([(p.clone(), v.clone())]))?;
            Ok((p, v))
        }) {
            Ok((path, value)) => {
                self.lighting_error = false;
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "console.submit:/lighting.set {}",
                    json!({"property":path,"value":value})
                )));
            }
            Err(error) => {
                self.lighting_error = true;
                tracing::debug!(%error, "Invalid Lighting field");
            }
        }
        true
    }
}
