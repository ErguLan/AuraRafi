use super::*;

impl NativeGameWorkbench {
    pub(super) fn handle_set_text(&mut self, key: &String, value: &String, selected: &[SceneNodeId], intents: &mut Vec<NativeWorkbenchIntent>) {
        match key.as_str() {
            "electronics.footprint.query" => {
                self.footprint_chooser.set_query(value.clone());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "electronics.footprint.import_path" => {
                self.footprint_chooser.set_import_path(value.clone());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "application-bar.command-search.value" => {
                // The title-bar field is a live entry point into the
                // Search everywhere modal, never a silent sink.
                self.open_search_with_query(value.clone());
            }
            "assets.search" => {
                self.assets_surface.query = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "hierarchy.search" | "hierarchy.search.global" => {
                self.hierarchy_query = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "electronics.library.search" => {
                self.electronics_library_query = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            key if crate::electronics_controller::ComponentCreatorField::from_value_key(key).is_some() => {
                if let Some(field) = crate::electronics_controller::ComponentCreatorField::from_value_key(key) {
                    intents.insert(
                        0,
                        NativeWorkbenchIntent::Command(format!(
                            "{}{}:{}",
                            crate::electronics_controller::COMPONENT_CREATOR_FIELD_PREFIX,
                            field.slug(),
                            value
                        )),
                    );
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            "nodes.search" => {
                self.nodes_host.set_query(value.clone());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "nodes.palette-popup.search" => {
                self.nodes_host.set_palette_query(value.clone());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            key if key.starts_with("nodes.property.") => {
                self.nodes_host
                    .set_property_draft(key.to_string(), value.clone());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            key if key.starts_with("electronics.inspector.transform.") || key.starts_with("electronics.pcb.rule.") => {
                self.toolbar_revision=self.toolbar_revision.wrapping_add(1).max(1);
            }
            "electronics.inspector.value" => {
                if let Some(draft) = self.electronics_value_draft.as_mut() {
                    draft.value = value.clone();
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            "inspector.session.new_name" => {
                self.inspector_session_name = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "assets.file-name" => {
                self.assets_surface.file_name = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "assets.script-name" => {
                self.assets_surface.script_name = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "assets.rename-value" => {
                self.assets_surface.rename_value = value.clone();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "console.input" => {
                self.console.set_input(value.clone());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "agent.input" => {
                self.agent_panel.borrow_mut().apply_action(
                    AgentAction::SetInput(value.clone()),
                    &mut self.agent_settings,
                );
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "agent.new-model.id" => {
                self.agent_panel.borrow_mut().apply_action(
                    AgentAction::SetNewModelId(value.clone()),
                    &mut self.agent_settings,
                );
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            "inspector.name" => {
                if let Some(id) = selected.first().copied() {
                    self.inspector_name_editing = Some((id, value.clone()));
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            key if key.starts_with("hierarchy.rename.") => {
                if let Some((id, current)) = self.hierarchy_renaming.as_mut() {
                    let expected = format!("hierarchy.rename.{}", id.0);
                    if key == &expected {
                        *current = value.clone();
                        self.toolbar_revision =
                            self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
            }
            "project-settings.default_scene_name" => {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            key if key.starts_with("project-settings.runtime.") => {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            _ => {}
        }
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_inspector_dropdown_trigger_command(&mut self, name: &String) -> bool {
        if name.starts_with("inspector.primitive:")
            || name.starts_with("inspector.collider:")
            || name.starts_with("inspector.body-type:")
            || name.starts_with("inspector.base-color-texture:")
        {
            let trigger_id = if name.starts_with("inspector.primitive:") {
                "inspector.primitive.trigger"
            } else if name.starts_with("inspector.collider:") {
                "inspector.collider.trigger"
            } else if name.starts_with("inspector.body-type:") {
                "inspector.body-type.trigger"
            } else {
                "inspector.base-color-texture.trigger"
            };
            self.inspector_view.dropdown = None;
            self.inspector_view.color_picker = false;
            self.host
                .session_mut()
                .interaction
                .focus
                .request_focus(trigger_id);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_inspector_tab_command(&mut self, name: &String) -> bool {
        if let Some(slug) = name.strip_prefix("inspector.section.toggle:") {
            if let Some(section) = inspector_section_from_slug(slug) {
                let expanded = !self.inspector_view.section(section);
                set_inspector_section(&mut self.inspector_view, section, expanded);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if let Some(tab) = name.strip_prefix("inspector.tab:") {
            self.inspector_view.tab = match tab {
                "sessions" => InspectorTab::Sessions,
                _ => InspectorTab::Properties,
            };
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_inspector_color_command(&mut self, name: &String, selected: &[SceneNodeId], scene: &SceneGraph, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        if name == "inspector.color.toggle" {
            self.inspector_view.color_picker = !self.inspector_view.color_picker;
            self.inspector_view.dropdown = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(hue) = name.strip_prefix("inspector.color.hue:") {
            let Some(target) = selected.first().copied() else {
                return true;
            };
            intents.push(NativeWorkbenchIntent::Command(format!(
                "inspector.color.hue:{hue}:{}",
                target.0
            )));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(slug) = name.strip_prefix("inspector.dropdown.toggle:") {
            if let Some((dropdown, navigation)) = slug.rsplit_once(':') {
                if matches!(navigation, "next" | "previous" | "first" | "last") {
                    if matches!(
                        dropdown,
                        "primitive" | "collider" | "body-type" | "base-color-texture"
                    ) {
                        self.inspector_view.dropdown = match dropdown {
                            "primitive" => Some(InspectorDropdown::Primitive),
                            "collider" => Some(InspectorDropdown::Collider),
                            "body-type" => Some(InspectorDropdown::BodyType),
                            _ => Some(InspectorDropdown::Texture),
                        };
                        if let Some(command) = inspector_dropdown_navigation_command(
                            scene,
                            selected,
                            dropdown,
                            navigation,
                            self.project_catalog.assets(),
                        ) {
                            intents.push(NativeWorkbenchIntent::Command(command));
                            self.inspector_view.dropdown = None;
                        }
                        self.toolbar_revision =
                            self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    return true;
                }
            }
            let next = match slug {
                "primitive" => Some(InspectorDropdown::Primitive),
                "collider" => Some(InspectorDropdown::Collider),
                "body-type" => Some(InspectorDropdown::BodyType),
                "base-color-texture" => Some(InspectorDropdown::Texture),
                _ => None,
            };
            self.inspector_view.dropdown = (self.inspector_view.dropdown != next)
                .then_some(next)
                .flatten();
            self.inspector_view.color_picker = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_inspector_field_command(
        &mut self,
        name: &String,
        dispatched: &UiDispatchedAction,
        selected: &[SceneNodeId],
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(index) = name
            .strip_prefix("inspector.behavior.parameters:")
            .and_then(|index| index.parse::<usize>().ok())
        {
            self.inspector_view.behavior_parameters =
                (self.inspector_view.behavior_parameters != Some(index))
                    .then_some(index);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("inspector.parameter.") {
            if let Some(command) = crate::native_behavior_authoring::parameter_action(
                scene,
                selected,
                raw,
                |key| {
                    self.host
                        .session()
                        .interaction
                        .controls
                        .text(key)
                        .to_string()
                },
            ) {
                intents.push(NativeWorkbenchIntent::Command(command));
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("inspector.behavior.detach:") {
            if let Some(command) =
                crate::native_behavior_authoring::detach_action(scene, selected, raw)
            {
                intents.push(NativeWorkbenchIntent::Command(command));
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("inspector.prefab.capture:") {
            if let Some(command) = crate::native_behavior_authoring::capture_action(
                scene,
                selected,
                raw,
                |key| {
                    self.host
                        .session()
                        .interaction
                        .controls
                        .text(key)
                        .to_string()
                },
            ) {
                intents.push(NativeWorkbenchIntent::Command(command));
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw_id) = name.strip_prefix("inspector.rename.commit:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                let value = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text("inspector.name")
                    .to_string();
                intents.push(NativeWorkbenchIntent::InspectorCommit {
                    target: SceneNodeId(id),
                    field: "name".to_string(),
                    value,
                });
                self.inspector_name_editing = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.rename.commit:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                let value = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text(&format!("hierarchy.rename.{id}"))
                    .to_string();
                intents.push(NativeWorkbenchIntent::InspectorCommit {
                    target: SceneNodeId(id),
                    field: "name".to_string(),
                    value,
                });
                self.hierarchy_renaming = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if name == "inspector.color.hex.commit" {
            let Some(target) = selected.first().copied() else {
                return true;
            };
            let value = self
                .host
                .session()
                .interaction
                .controls
                .text("inspector.color.hex")
                .to_string();
            intents.push(NativeWorkbenchIntent::InspectorCommit {
                target,
                field: "color".to_string(),
                value,
            });
            self.inspector_view.color_picker = false;
            self.inspector_view.dropdown = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(hex) = name.strip_prefix("inspector.color.preset:") {
            let Some(target) = selected.first().copied() else {
                return true;
            };
            intents.push(NativeWorkbenchIntent::InspectorCommit {
                target,
                field: "color".to_string(),
                value: hex.to_string(),
            });
            self.inspector_view.color_picker = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(hex) = name.strip_prefix("inspector.color.apply:") {
            let Some(target) = selected.first().copied() else {
                return true;
            };
            intents.push(NativeWorkbenchIntent::InspectorCommit {
                target,
                field: "color".to_string(),
                value: hex.to_string(),
            });
            // Hue changes are iterative: leave the picker open so
            // the user can refine channels in one pass.
            self.inspector_view.dropdown = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(kind) = name.strip_prefix("inspector.physics.add-joint:") {
            if let [owner, target, ..] = selected {
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "inspector.physics.joint.add:{}:{}:{kind}", owner.0, target.0
                )));
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(field) = name.strip_prefix("inspector.physics.commit:") {
            if let Some(target) = selected.first().copied() {
                let value = self.host.session().interaction.controls
                    .text(&format!("inspector.physics.{field}.text"))
                    .to_string();
                intents.push(NativeWorkbenchIntent::InspectorCommit {
                    target,
                    field: format!("physics.{field}"),
                    value,
                });
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if let Some(field) = name.strip_prefix("inspector.camera.commit:") {
            if let Some(target) = selected.first().copied() {
                let value = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text(&format!("inspector.camera.{field}.text"))
                    .to_string();
                intents.push(NativeWorkbenchIntent::InspectorCommit {
                    target,
                    field: format!("camera.{field}"),
                    value,
                });
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if name == "inspector.numeric.commit" {
            let Some(target) = selected.first().copied() else {
                return true;
            };
            let Some((field, key)) = numeric_commit_field(&dispatched.target_id) else {
                return true;
            };
            let value = self
                .host
                .session()
                .interaction
                .controls
                .text(&key)
                .to_string();
            let value = if field.starts_with("position.") {
                value
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| {
                        self.agent_settings
                            .display_unit
                            .to_meters(value)
                            .to_string()
                    })
                    .unwrap_or(value)
            } else {
                value
            };
            intents.push(NativeWorkbenchIntent::InspectorCommit {
                target,
                field,
                value,
            });
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}

pub(super) fn inspector_dropdown_from_trigger(id: &str) -> Option<InspectorDropdown> {
    if let Some(dropdown) =
        crate::panels::inspector_surface::behavior_surface::dropdown_from_trigger(id)
    {
        return Some(dropdown);
    }
    match id {
        "inspector.primitive.trigger" => Some(InspectorDropdown::Primitive),
        "inspector.collider.trigger" => Some(InspectorDropdown::Collider),
        "inspector.body-type.trigger" => Some(InspectorDropdown::BodyType),
        "inspector.base-color-texture.trigger" => Some(InspectorDropdown::Texture),
        _ => None,
    }
}

fn inspector_dropdown_navigation_command(
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    dropdown: &str,
    navigation: &str,
    project_assets: &[String],
) -> Option<String> {
    let id = selected.first().copied()?;
    let node = scene.get(id)?;
    if dropdown == "base-color-texture" {
        let choices = std::iter::once("")
            .chain(
                project_assets
                    .iter()
                    .filter(|asset| is_supported_texture_asset(asset))
                    .take(512)
                    .map(String::as_str),
            )
            .collect::<Vec<_>>();
        let current = node.base_color_texture.as_deref().unwrap_or_default();
        let current_index = choices.iter().position(|value| *value == current)?;
        let next_index = match navigation {
            "first" => 0,
            "last" => choices.len().saturating_sub(1),
            "next" => (current_index + 1) % choices.len(),
            "previous" => (current_index + choices.len() - 1) % choices.len(),
            _ => return None,
        };
        return Some(format!(
            "inspector.base-color-texture:{}:{}",
            id.0, choices[next_index]
        ));
    }
    let (current, labels): (&str, &[&str]) = match dropdown {
        "primitive" => (
            match node.primitive {
                raf_core::scene::Primitive::Empty => "Empty",
                raf_core::scene::Primitive::Cube => "Cube",
                raf_core::scene::Primitive::Sphere => "Sphere",
                raf_core::scene::Primitive::Plane => "Plane",
                raf_core::scene::Primitive::Cylinder => "Cylinder",
                raf_core::scene::Primitive::Capsule => "Capsule",
            },
            &["Empty", "Cube", "Sphere", "Plane", "Cylinder", "Capsule"],
        ),
        "collider" => (
            match node.collider.collider_type {
                raf_core::scene::ColliderType::None => "None",
                raf_core::scene::ColliderType::Aabb => "Aabb",
                raf_core::scene::ColliderType::ConvexHull => "ConvexHull",
                raf_core::scene::ColliderType::MeshCollider => "MeshCollider",
            },
            &["None", "Aabb", "ConvexHull", "MeshCollider"],
        ),
        "body-type" => (
            match node.rigid_body.body_type {
                raf_core::scene::RigidBodyType::Static => "Static",
                raf_core::scene::RigidBodyType::Dynamic => "Dynamic",
                raf_core::scene::RigidBodyType::Kinematic => "Kinematic",
            },
            &["Static", "Dynamic", "Kinematic"],
        ),
        _ => return None,
    };
    let current_index = labels.iter().position(|label| *label == current)?;
    let next_index = match navigation {
        "first" => 0,
        "last" => labels.len().saturating_sub(1),
        "next" => (current_index + 1) % labels.len(),
        "previous" => (current_index + labels.len() - 1) % labels.len(),
        _ => return None,
    };
    let value = labels[next_index];
    Some(match dropdown {
        "primitive" => format!("inspector.primitive:{}:{value}", id.0),
        "collider" => format!("inspector.collider:{}:{value}", id.0),
        "body-type" => format!("inspector.body-type:{}:{value}", id.0),
        _ => return None,
    })
}

fn is_supported_texture_asset(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return false;
    }
    matches!(
        std::path::Path::new(&normalized)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "bmp" | "tga" | "webp")
    )
}
