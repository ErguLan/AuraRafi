use super::*;

use crate::native_editor_commands::{
    asset_duplicate_row, asset_file_row, asset_particle_row, asset_rename_row, asset_script_row,
};
use crate::panels::assets_surface::{
    asset_file_name, is_builtin_asset_row, AssetFilter, AssetSort, AssetViewMode,
};
use crate::panels::assets_surface_host::AssetsContextMenu;

impl NativeGameWorkbench {
    pub(super) fn handle_assets_menu_command(&mut self, name: &String) -> bool {
        match name.as_str() {
            "assets.create-menu.toggle" => {
                let open = !self.assets_surface.create_menu_open;
                self.assets_surface.close_menus();
                self.assets_surface.create_menu_open = open;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.create-primitive.toggle" => {
                let open = !self.assets_surface.primitive_menu_open;
                self.assets_surface.close_menus();
                self.assets_surface.primitive_menu_open = open;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.create-primitive.cancel" => {
                self.assets_surface.primitive_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.sort.toggle" => {
                self.assets_surface.sort = match self.assets_surface.sort {
                    AssetSort::NameAsc => AssetSort::NameDesc,
                    AssetSort::NameDesc => AssetSort::NameAsc,
                };
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.view.toggle" => {
                self.assets_surface.view = match self.assets_surface.view {
                    AssetViewMode::Grid => AssetViewMode::List,
                    AssetViewMode::List => AssetViewMode::Grid,
                };
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.modal.close" => {
                let menus = self.assets_surface.close_menus();
                let modals = self.assets_surface.close_modals();
                if menus || modals {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            "assets.rename.cancel" | "assets.delete.cancel" => {
                let changed = self.assets_surface.close_modals();
                if changed {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_assets_create_menu_command(
        &mut self,
        name: &String,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        match name.as_str() {
            "assets.create-script" => {
                self.assets_surface.close_menus();
                self.assets_surface.script_menu_open = true;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.create-file" => {
                self.assets_surface.close_menus();
                self.assets_surface.file_menu_open = true;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.create-particle" => {
                self.assets_surface.close_menus();
                self.assets_surface.particle_menu_open = true;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.generate-local-icon" => {
                self.assets_surface.close_menus();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                intents.push(NativeWorkbenchIntent::Command(name.clone()));
                return true;
            }
            "assets.script.cancel" => {
                self.assets_surface.script_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.file.cancel" => {
                self.assets_surface.file_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.particle.cancel" => {
                self.assets_surface.particle_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "assets.refresh" => {
                self.project_catalog.refresh();
                return true;
            }
            "assets.open-folder" => {
                intents.push(NativeWorkbenchIntent::OpenProjectFolder);
                return true;
            }
            "project.open_folder" => {
                self.bottom_dock.select("assets");
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                intents.push(NativeWorkbenchIntent::OpenProjectFolder);
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_assets_create_primitive_command(
        &mut self,
        name: &String,
        scene: &SceneGraph,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(slug) = name.strip_prefix("assets.create-primitive:") {
            self.assets_surface.primitive_menu_open = false;
            if let Some(parent) = self.insertion_parent(scene) {
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "hierarchy.create-primitive:{}:{slug}",
                    parent.0
                )));
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            self.assets_surface.primitive_menu_open = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_assets_command(
        &mut self,
        name: &String,
        scene: &SceneGraph,
        native_input: &NativeUiInputBridge,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(row) = name.strip_prefix("assets.place-rig:") {
            let args = serde_json::json!({"asset": row, "parent": self.insertion_parent(scene).map(|id| id.0)});
            intents.push(NativeWorkbenchIntent::Command(format!(
                "animations.place-rig:{args}"
            )));
            self.assets_surface.close_menus();
            self.bottom_dock.select("animations");
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(filter) = name.strip_prefix("assets.filter.") {
            self.assets_surface.filter = match filter {
                "images" => AssetFilter::Images,
                "models" => AssetFilter::Models,
                "audio" => AssetFilter::Audio,
                "scripts" => AssetFilter::Scripts,
                "interfaces" => AssetFilter::Interfaces,
                "animations" => AssetFilter::Animations,
                _ => AssetFilter::All,
            };
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(extension) = name.strip_prefix("assets.script-ext:") {
            self.assets_surface.script_extension = match extension {
                "all" => String::new(),
                other => other.to_string(),
            };
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "assets.import-open" || name == "assets.import" {
            self.open_assets_import(None);
            return true;
        }
        if name == "assets.import.browse" || name == "assets.import.drop" {
            intents.push(NativeWorkbenchIntent::ImportAssetFile);
            return true;
        }
        if name == "assets.import.cancel" {
            self.close_assets_import();
            return true;
        }
        if name == "assets.import.confirm" {
            if let Err(error) = self.confirm_assets_import() {
                if let Some(state) = self.assets_import.as_mut() {
                    state.error = Some(error);
                }
            }
            return true;
        }
        if name == "assets.import.textures" {
            if let Some(state) = self.assets_import.as_mut() {
                state.options.import_textures = !state.options.import_textures;
            }
            return true;
        }
        if name == "assets.import.recenter" {
            if let Some(state) = self.assets_import.as_mut() {
                state.options.recenter = !state.options.recenter;
            }
            return true;
        }
        if name == "assets.import.collider" {
            if let Some(state) = self.assets_import.as_mut() {
                state.options.generate_collider = !state.options.generate_collider;
            }
            return true;
        }
        if let Some(row) = name.strip_prefix("assets.audio.preview:") {
            let Some(root) = self.assets_project_root.as_ref() else {
                self.assets_surface.status = Some(AssetsStatus::error("Project not found"));
                return true;
            };
            let path = root.join("assets").join(row);
            match self.audio_preview.preview_file(&path, 0.9, false) {
                Ok(()) => {
                    self.assets_surface.selected_asset = Some(row.to_string());
                    self.assets_surface.status = Some(AssetsStatus::pending(raf_core::i18n::t(
                        "app.audio_previewing",
                        self.agent_settings.language,
                    )));
                }
                Err(error) => {
                    self.assets_surface.status = Some(AssetsStatus::error(error));
                }
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "assets.audio.stop" {
            self.audio_preview.stop_preview();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "assets.drag-move" || name == "assets.drag-end" {
            return true;
        }
        if let Some(row) = name
            .strip_prefix("assets.insert:")
            .or_else(|| name.strip_prefix("assets.drag-start:"))
        {
            let entry = raf_assets::authoring_catalog::AuthoringAsset::from_path(row);
            if entry.class == raf_assets::authoring_catalog::AssetClass::Light {
                let kind = row
                    .strip_prefix("builtin://light/")
                    .unwrap_or("point")
                    .to_string();
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "hierarchy.create-light:{kind}"
                )));
            } else if entry.class == raf_assets::authoring_catalog::AssetClass::Interface {
                self.interface.diagnostic = self
                    .interface
                    .insert_asset(row, self.agent_settings.language)
                    .err();
                self.activate_interface_authoring();
            } else if entry.class == raf_assets::authoring_catalog::AssetClass::Model {
                let args = serde_json::json!({
                    "path": row,
                    "parent": self.insertion_parent(scene).map(|id| id.0),
                    "drag": name.starts_with("assets.drag-start:")
                });
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "authoring.asset:{args}"
                )));
                self.assets_surface.status = Some(AssetsStatus::pending(raf_core::i18n::t(
                    "assets.placement_hint",
                    self.agent_settings.language,
                )));
            } else if entry.class == raf_assets::authoring_catalog::AssetClass::Camera {
                let parent = self
                    .insertion_parent(scene)
                    .map(|id| id.0.to_string())
                    .unwrap_or_else(|| "root".into());
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "hierarchy.create-camera:{parent}"
                )));
            } else if entry.class == raf_assets::authoring_catalog::AssetClass::Particle {
                let parent = self
                    .insertion_parent(scene)
                    .map(|id| id.0.to_string())
                    .unwrap_or_else(|| "root".into());
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "hierarchy.create-particle:{row}:{parent}"
                )));
            } else if entry.insertable() {
                let args = serde_json::json!({"path":row,"parent":self.insertion_parent(scene).map(|id|id.0),"drag":name.starts_with("assets.drag-start:")});
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "authoring.asset:{args}"
                )));
                self.assets_surface.status = Some(AssetsStatus::pending(raf_core::i18n::t(
                    "assets.placement_hint",
                    self.agent_settings.language,
                )));
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(row) = name.strip_prefix("assets.usages:") {
            self.hierarchy_query = format!("asset:\"{}\"", row.replace('"', ""));
            self.hierarchy_scroll_offset = 0.0;
            self.hierarchy_model.invalidate();
            self.hierarchy_active_tab = "hierarchy".into();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(row) = name.strip_prefix("assets.select:") {
            self.assets_surface.selected_asset = Some(row.to_string());
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(row) = name.strip_prefix("assets.context-menu:") {
            let Some(position) = native_input.snapshot().pointer_position else {
                return true;
            };
            self.assets_surface.close_menus();
            self.assets_surface.close_modals();
            self.assets_surface.selected_asset = Some(row.to_string());
            self.assets_surface.context_menu = Some(AssetsContextMenu {
                row: row.to_string(),
                builtin: is_builtin_asset_row(row),
                position,
            });
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(row) = name.strip_prefix("assets.open:") {
            if raf_assets::animation_asset::is_animation_asset(row) {
                self.open_animation_asset(row);
            } else if row.starts_with("builtin://interface/")
                || row.to_ascii_lowercase().ends_with(".ui.ron")
            {
                self.open_interface_asset(row);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            } else if row == "builtin://camera" {
                self.assets_surface.close_menus();
                let parent = self
                    .insertion_parent(scene)
                    .map(|id| id.0.to_string())
                    .unwrap_or_else(|| "root".into());
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "hierarchy.create-camera:{parent}"
                )));
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            } else if let Some(slug) = row.strip_prefix("builtin://primitive/") {
                let command = if let Some(parent) = self.insertion_parent(scene) {
                    format!("hierarchy.create-primitive:{}:{slug}", parent.0)
                } else {
                    format!("assets.create-primitive:{slug}")
                };
                intents.push(NativeWorkbenchIntent::Command(command));
            } else {
                self.assets_surface.close_menus();
                self.assets_surface.close_modals();
                self.assets_surface.selected_asset = Some(row.to_string());
                self.assets_surface.open_asset = Some(row.to_string());
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if let Some((bare, row)) = name.split_once(':') {
            let row = row.to_string();
            let handled = match bare {
                "assets.rename.begin" => {
                    self.assets_surface.close_menus();
                    self.assets_surface.close_modals();
                    self.assets_surface.selected_asset = Some(row.clone());
                    self.assets_surface.rename_value = asset_file_name(&row);
                    self.assets_surface.rename_asset = Some(row);
                    self.host
                        .session_mut()
                        .interaction
                        .focus
                        .request_focus("assets.rename-value");
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    true
                }
                "assets.rename.confirm" => {
                    let value = self.assets_surface.rename_value.trim().to_string();
                    if !value.is_empty() {
                        self.begin_assets_operation(
                            asset_rename_row(&row, &value),
                            true,
                            "app.assets_status_renamed",
                            "app.assets_status_rename_failed",
                        );
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "assets.rename:{row}:{value}"
                        )));
                    }
                    self.assets_surface.rename_asset = None;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    true
                }
                "assets.delete.begin" => {
                    self.assets_surface.close_menus();
                    self.assets_surface.close_modals();
                    self.assets_surface.selected_asset = Some(row.clone());
                    self.assets_surface.delete_asset = Some(row);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    true
                }
                "assets.delete.confirm" => {
                    self.begin_assets_operation(
                        row.clone(),
                        false,
                        "app.assets_status_deleted",
                        "app.assets_status_delete_failed",
                    );
                    self.assets_surface.delete_asset = None;
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "assets.delete:{row}"
                    )));
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    true
                }
                "assets.duplicate" => {
                    self.assets_surface.close_menus();
                    self.begin_assets_operation(
                        asset_duplicate_row(&row),
                        true,
                        "app.assets_status_duplicated",
                        "app.assets_status_duplicate_failed",
                    );
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "assets.duplicate:{row}"
                    )));
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    true
                }
                "assets.reveal"
                | "assets.open.with"
                | "assets.open.yoll"
                | "assets.open.editor"
                | "assets.open.manager" => {
                    self.assets_surface.close_menus();
                    self.assets_surface.close_modals();
                    intents.push(NativeWorkbenchIntent::Command(name.clone()));
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    true
                }
                _ => false,
            };
            if handled {
                return true;
            }
        }
        if let Some(language) = name.strip_prefix("assets.script.create:") {
            self.assets_surface.script_menu_open = false;
            let asset_name = self.assets_surface.script_name.trim().to_string();
            self.begin_assets_operation(
                asset_script_row(language, &asset_name),
                true,
                "app.assets_status_script_created",
                "app.assets_status_script_create_failed",
            );
            intents.push(NativeWorkbenchIntent::Command(format!(
                "assets.create-script:{language}:{asset_name}"
            )));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "assets.file.create" {
            self.assets_surface.file_menu_open = false;
            let asset_name = self.assets_surface.file_name.trim().to_string();
            self.begin_assets_operation(
                asset_file_row(&asset_name),
                true,
                "app.assets_status_file_created",
                "app.assets_status_file_create_failed",
            );
            intents.push(NativeWorkbenchIntent::Command(format!(
                "assets.create-file:{asset_name}"
            )));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(template) = name.strip_prefix("assets.particle.create:") {
            self.assets_surface.particle_menu_open = false;
            let asset_name = self.assets_surface.particle_name.trim().to_string();
            self.begin_assets_operation(
                asset_particle_row(&asset_name),
                true,
                "app.assets_status_particle_created",
                "app.assets_status_particle_create_failed",
            );
            intents.push(NativeWorkbenchIntent::Command(format!(
                "assets.create-particle:{template}:{asset_name}"
            )));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}
