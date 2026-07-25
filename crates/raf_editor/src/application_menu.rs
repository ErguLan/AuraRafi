//! Shared editor application-menu declaration and temporary eframe fallback.
//!
//! Platform shells consume `UiApplicationMenu`; only the application boundary
//! performs the command side effects.

use eframe::egui;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_ui::{UiApplicationMenu, UiMenu, UiMenuCommand, UiMenuItem};

pub mod command {
    pub const PROJECT_NEW: &str = "project.new";
    pub const PROJECT_SAVE: &str = "project.save";
    pub const EDITOR_SETTINGS: &str = "editor.settings";
    pub const PROJECT_EXIT_TO_HUB: &str = "project.exit_to_hub";
    pub const EDIT_UNDO: &str = "edit.undo";
    pub const EDIT_REDO: &str = "edit.redo";
    pub const EDIT_DUPLICATE: &str = "edit.duplicate";
    pub const EDIT_DELETE: &str = "edit.delete";
    pub const EDIT_SELECT_ALL: &str = "edit.select_all";
    pub const VIEW_GRID: &str = "view.grid";
    pub const VIEW_SCENE: &str = "view.scene";
    pub const VIEW_SCHEMATIC: &str = "view.schematic";
    pub const VIEW_PCB: &str = "view.pcb";
    pub const PROJECT_OPEN_FOLDER: &str = "project.open_folder";
    pub const PROJECT_CLOSE: &str = "project.close";
    pub const RAFUI_STUDIO_PREVIEW: &str = "rafui.studio.preview";
    pub const HELP_KEYBOARD_SHORTCUTS: &str = "help.keyboard_shortcuts";
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EditorApplicationMenuState {
    pub electronics_project: bool,
    pub project_open: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub grid_visible: bool,
    pub scene_active: bool,
    pub schematic_active: bool,
    pub pcb_active: bool,
    pub undo_count: usize,
    pub redo_count: usize,
}

pub fn build_editor_application_menu(state: EditorApplicationMenuState) -> UiApplicationMenu {
    let command = |id: &str, key: &str| UiMenuItem::Command(UiMenuCommand::new(id, key));
    let configured = |id: &str, key: &str, enabled: bool, checked: bool| {
        UiMenuItem::Command(
            UiMenuCommand::new(id, key)
                .with_enabled(enabled)
                .with_checked(checked),
        )
    };

    UiApplicationMenu {
        menus: vec![
            UiMenu::new("file", "app.file")
                .with_item(command(command::PROJECT_NEW, "app.new_project"))
                .with_item(command(command::PROJECT_SAVE, "app.save_menu"))
                .with_item(UiMenuItem::Separator)
                .with_item(command(command::EDITOR_SETTINGS, "app.settings_menu"))
                .with_item(UiMenuItem::Separator)
                .with_item(command(command::PROJECT_EXIT_TO_HUB, "app.exit_to_hub")),
            UiMenu::new("edit", "app.edit_menu")
                .with_item(configured(
                    command::EDIT_UNDO,
                    "app.undo_menu",
                    state.can_undo,
                    false,
                ))
                .with_item(configured(
                    command::EDIT_REDO,
                    "app.redo_menu",
                    state.can_redo,
                    false,
                ))
                .with_item(UiMenuItem::Separator)
                .with_item(command(command::EDIT_DUPLICATE, "app.duplicate_menu"))
                .with_item(command(command::EDIT_DELETE, "app.delete_menu"))
                .with_item(UiMenuItem::Separator)
                .with_item(command(command::EDIT_SELECT_ALL, "app.select_all_menu")),
            UiMenu::new("view", "app.view_menu")
                .with_item(configured(
                    command::VIEW_GRID,
                    "app.grid_menu",
                    true,
                    state.grid_visible,
                ))
                .with_item(UiMenuItem::Separator)
                .with_item(configured(
                    command::VIEW_SCENE,
                    "app.scene_view",
                    !state.electronics_project,
                    state.scene_active,
                ))
                .with_item(configured(
                    command::VIEW_SCHEMATIC,
                    "app.schematic_view",
                    state.electronics_project,
                    state.schematic_active,
                ))
                .with_item(configured(
                    command::VIEW_PCB,
                    "app.pcb_view",
                    state.electronics_project,
                    state.pcb_active,
                )),
            UiMenu::new("project", "app.project_menu")
                .with_item(configured(
                    command::PROJECT_OPEN_FOLDER,
                    "app.open_folder",
                    state.project_open && cfg!(target_os = "windows"),
                    false,
                ))
                .with_item(command(command::PROJECT_CLOSE, "app.close_project")),
            UiMenu::new("rafui_studio", "app.rafui_studio_menu").with_item(command(
                command::RAFUI_STUDIO_PREVIEW,
                "app.rafui_studio_preview",
            )),
            UiMenu::new("help", "app.help_menu").with_item(command(
                command::HELP_KEYBOARD_SHORTCUTS,
                "app.keyboard_shortcuts",
            )),
        ],
    }
}

/// Temporary eframe presentation of the retained application-menu model.
/// Native shells install the same `UiApplicationMenu` through their platform
/// adapter and return the same command IDs to the caller.
pub fn show_eframe_application_menu<F>(
    ui: &mut egui::Ui,
    menu: &UiApplicationMenu,
    language: Language,
    state: EditorApplicationMenuState,
    on_activation: &mut F,
) where
    F: FnMut(&str),
{
    for section in menu.menus.iter().rev() {
        let label = t(&section.label_key, language);
        ui.menu_button(label, |ui| {
            show_eframe_menu_section(ui, &section.items, language, state, on_activation)
        });
    }
}

fn show_eframe_menu_section<F>(
    ui: &mut egui::Ui,
    items: &[UiMenuItem],
    language: Language,
    state: EditorApplicationMenuState,
    on_activation: &mut F,
) where
    F: FnMut(&str),
{
    for item in items {
        match item {
            UiMenuItem::Separator => {
                ui.separator();
            }
            UiMenuItem::Submenu(menu) => {
                let label = t(&menu.label_key, language);
                ui.menu_button(label, |ui| {
                    show_eframe_menu_section(ui, &menu.items, language, state, on_activation)
                });
            }
            UiMenuItem::Command(command) => {
                let label = command_label(command, language, state);
                if ui
                    .add_enabled(command.enabled, egui::Button::new(label))
                    .clicked()
                {
                    on_activation(&command.id);
                    ui.close_menu();
                }
            }
        }
    }
}

fn command_label(
    command: &UiMenuCommand,
    language: Language,
    state: EditorApplicationMenuState,
) -> String {
    let mut label = t(&command.label_key, language);
    if command.checked {
        label = format!("[x] {label}");
    }
    if command.id == command::EDIT_UNDO {
        label = format!("{label}  [{}]", state.undo_count);
    } else if command.id == command::EDIT_REDO {
        label = format!("{label}  [{}]", state.redo_count);
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_model_keeps_platform_independent_command_ids() {
        let menu = build_editor_application_menu(EditorApplicationMenuState {
            electronics_project: true,
            project_open: true,
            can_undo: true,
            ..EditorApplicationMenuState::default()
        });

        assert!(menu.contains_command(command::PROJECT_SAVE));
        assert!(menu.contains_command(command::VIEW_SCHEMATIC));
        assert!(menu.contains_command(command::VIEW_PCB));
        assert!(menu.contains_command(command::HELP_KEYBOARD_SHORTCUTS));
    }
}
