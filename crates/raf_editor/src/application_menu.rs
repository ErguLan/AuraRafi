//! Shared application-menu command tree for the RafUI shell.
//!
//! The retained application bar and future native window adapters consume the
//! same state-aware command model. The application boundary remains the only
//! place that executes project, editor, or window behavior.

use raf_core::project::ProjectType;
use raf_ui::{UiApplicationMenu, UiMenu, UiMenuCommand, UiMenuItem};

pub mod command {
    pub const PROJECT_NEW: &str = "project.new";
    pub const PROJECT_OPEN: &str = "project.open";
    pub const PROJECT_SAVE: &str = "project.save";
    pub const EDITOR_SETTINGS: &str = "editor.settings";
    pub const PROJECT_SETTINGS: &str = "project.settings";
    pub const EXIT_TO_HUB: &str = "application.exit_to_hub";
    pub const EDIT_UNDO: &str = "edit.undo";
    pub const EDIT_REDO: &str = "edit.redo";
    pub const EDIT_DUPLICATE: &str = "edit.duplicate";
    pub const EDIT_COPY: &str = "edit.copy";
    pub const EDIT_PASTE: &str = "edit.paste";
    pub const EDIT_DELETE: &str = "edit.delete";
    pub const EDIT_SELECT_ALL: &str = "edit.select_all";
    pub const SEARCH_OPEN: &str = "search.open";
    pub const VIEW_GRID: &str = "view.grid";
    pub const VIEW_SCENE: &str = "editor.scene_view";
    pub const VIEW_HIERARCHY: &str = "editor.hierarchy_view";
    pub const VIEW_INSPECTOR: &str = "editor.inspector_view";
    pub const VIEW_SCHEMATIC: &str = "electronics.mode.schematic";
    pub const VIEW_PCB: &str = "electronics.mode.pcb";
    pub const PROJECT_OPEN_FOLDER: &str = "project.open_folder";
    pub const PROJECT_CLOSE: &str = "project.close";
    pub const HELP_KEYBOARD_SHORTCUTS: &str = "help.keyboard_shortcuts";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationView {
    Scene,
    Schematic,
    Pcb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationMenuState {
    pub project_type: ProjectType,
    pub active_view: ApplicationView,
    pub grid_visible: bool,
    pub hierarchy_visible: bool,
    pub inspector_visible: bool,
    pub undo_available: bool,
    pub redo_available: bool,
    pub selection_available: bool,
    pub select_all_available: bool,
    pub copy_available: bool,
    pub paste_available: bool,
}

pub fn build_application_menu(state: ApplicationMenuState) -> UiApplicationMenu {
    let edit_commands_available = state.selection_available;
    let mut view_menu = UiMenu::new("view", "app.view_menu").with_item(configured(
        command::VIEW_GRID,
        "app.grid_menu",
        true,
        state.grid_visible,
    ));
    view_menu = match state.project_type {
        ProjectType::Game => view_menu
            .with_item(configured(
                command::VIEW_SCENE,
                "app.scene_view",
                true,
                state.active_view == ApplicationView::Scene,
            ))
            .with_item(configured(
                command::VIEW_HIERARCHY,
                "app.hierarchy",
                true,
                state.hierarchy_visible,
            ))
            .with_item(configured(
                command::VIEW_INSPECTOR,
                "app.inspector",
                true,
                state.inspector_visible,
            )),
        ProjectType::Electronics => view_menu
            .with_item(UiMenuItem::Separator)
            .with_item(configured(
                command::VIEW_SCHEMATIC,
                "app.schematic_view",
                true,
                state.active_view == ApplicationView::Schematic,
            ))
            .with_item(configured(
                command::VIEW_PCB,
                "app.pcb_view",
                true,
                state.active_view == ApplicationView::Pcb,
            )),
    };

    UiApplicationMenu {
        menus: vec![
            UiMenu::new("file", "app.file")
                .with_item(command(command::PROJECT_NEW, "app.new_project"))
                .with_item(command(command::PROJECT_OPEN, "app.open_project"))
                .with_item(command_with_accelerator(
                    command::PROJECT_SAVE,
                    "app.save_menu",
                    crate::editor_shortcuts::accelerator_for(command::PROJECT_SAVE),
                ))
                .with_item(UiMenuItem::Separator)
                .with_item(command(command::EDITOR_SETTINGS, "app.settings_menu"))
                .with_item(command(
                    command::PROJECT_SETTINGS,
                    "app.project_settings_tab",
                ))
                .with_item(UiMenuItem::Separator)
                .with_item(command(command::EXIT_TO_HUB, "app.exit_to_hub")),
            UiMenu::new("edit", "app.edit_menu")
                .with_item(configured_with_accelerator(
                    command::EDIT_UNDO,
                    "app.undo_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_UNDO),
                    state.undo_available,
                    state.undo_available,
                ))
                .with_item(configured_with_accelerator(
                    command::EDIT_REDO,
                    "app.redo_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_REDO),
                    state.redo_available,
                    state.redo_available,
                ))
                .with_item(UiMenuItem::Separator)
                .with_item(configured_with_accelerator(
                    command::EDIT_DUPLICATE,
                    "app.duplicate_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_DUPLICATE),
                    edit_commands_available,
                    false,
                ))
                .with_item(configured_with_accelerator(
                    command::EDIT_COPY,
                    "app.copy_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_COPY),
                    state.copy_available,
                    false,
                ))
                .with_item(configured_with_accelerator(
                    command::EDIT_PASTE,
                    "app.paste_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_PASTE),
                    state.paste_available,
                    false,
                ))
                .with_item(configured_with_accelerator(
                    command::EDIT_DELETE,
                    "app.delete_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_DELETE),
                    edit_commands_available,
                    false,
                ))
                .with_item(configured_with_accelerator(
                    command::EDIT_SELECT_ALL,
                    "app.select_all_menu",
                    crate::editor_shortcuts::accelerator_for(command::EDIT_SELECT_ALL),
                    state.select_all_available,
                    false,
                )),
            view_menu,
            UiMenu::new("project", "app.project_menu")
                .with_item(configured(
                    command::PROJECT_OPEN_FOLDER,
                    "app.open_folder",
                    cfg!(target_os = "windows"),
                    false,
                ))
                .with_item(command(
                    command::PROJECT_SETTINGS,
                    "app.project_settings_tab",
                ))
                .with_item(command(command::PROJECT_CLOSE, "app.close_project")),
            UiMenu::new("help", "app.help_menu")
                .with_item(UiMenuItem::Submenu(keyboard_shortcuts_menu())),
        ],
    }
}

/// Reference-only shortcuts shown from Help. These are intentionally disabled
/// menu rows: the editor shortcut registry remains the executable source of
/// truth, while this submenu only exposes the same metadata to users.
fn keyboard_shortcuts_menu() -> UiMenu {
    let mut menu = UiMenu::new(command::HELP_KEYBOARD_SHORTCUTS, "app.keyboard_shortcuts");
    for (index, command_id) in [
        command::PROJECT_SAVE,
        command::SEARCH_OPEN,
        command::EDIT_UNDO,
        command::EDIT_REDO,
        command::EDIT_DUPLICATE,
        command::EDIT_COPY,
        command::EDIT_PASTE,
        command::EDIT_DELETE,
        command::EDIT_SELECT_ALL,
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(shortcut) = crate::editor_shortcuts::EDITOR_SHORTCUTS
            .iter()
            .find(|shortcut| shortcut.command == command_id)
        {
            menu = menu.with_item(shortcut_reference_item(index, shortcut));
        }
    }

    menu = menu.with_item(UiMenuItem::Separator);
    for (index, command_id) in [
        "viewport.move",
        "viewport.rotate",
        "viewport.scale",
        "viewport.select",
        "viewport.focus",
        "viewport.edit_mode",
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(shortcut) = crate::editor_shortcuts::VIEWPORT_SHORTCUTS
            .iter()
            .find(|shortcut| shortcut.command == command_id)
        {
            menu = menu.with_item(shortcut_reference_item(index + 100, shortcut));
        }
    }
    menu
}

fn shortcut_reference_item(
    index: usize,
    shortcut: &crate::editor_shortcuts::ShortcutSpec,
) -> UiMenuItem {
    let label_key = match shortcut.command {
        command::SEARCH_OPEN => "app.search_menu",
        "viewport.edit_mode" => "app.edit_mode",
        _ => shortcut.label_key,
    };
    UiMenuItem::Command(
        UiMenuCommand::new(format!("help.shortcut.{index}"), label_key)
            .with_accelerator(shortcut.accelerator)
            .with_enabled(false),
    )
}

fn command(id: &str, label_key: &str) -> UiMenuItem {
    UiMenuItem::Command(UiMenuCommand::new(id, label_key))
}

fn command_with_accelerator(id: &str, label_key: &str, accelerator: &str) -> UiMenuItem {
    UiMenuItem::Command(UiMenuCommand::new(id, label_key).with_accelerator(accelerator))
}

fn configured(id: &str, label_key: &str, enabled: bool, checked: bool) -> UiMenuItem {
    UiMenuItem::Command(
        UiMenuCommand::new(id, label_key)
            .with_enabled(enabled)
            .with_checked(checked),
    )
}

fn configured_with_accelerator(
    id: &str,
    label_key: &str,
    accelerator: &str,
    enabled: bool,
    checked: bool,
) -> UiMenuItem {
    UiMenuItem::Command(
        UiMenuCommand::new(id, label_key)
            .with_accelerator(accelerator)
            .with_enabled(enabled)
            .with_checked(checked),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_menu_has_no_electronics_or_runtime_commands() {
        let menu = build_application_menu(ApplicationMenuState {
            project_type: ProjectType::Game,
            active_view: ApplicationView::Scene,
            grid_visible: true,
            hierarchy_visible: true,
            inspector_visible: true,
            undo_available: false,
            redo_available: false,
            selection_available: false,
            select_all_available: false,
            copy_available: false,
            paste_available: false,
        });
        let commands = menu.command_ids();
        assert!(commands.contains(&command::VIEW_SCENE));
        assert!(!commands.contains(&command::VIEW_SCHEMATIC));
        assert!(!commands.contains(&command::VIEW_PCB));
        assert!(!commands.iter().any(|command| {
            command.contains("build") || command.contains("play") || command.contains("runtime")
        }));
    }

    #[test]
    fn electronics_menu_recovers_schematic_and_pcb_choices() {
        let menu = build_application_menu(ApplicationMenuState {
            project_type: ProjectType::Electronics,
            active_view: ApplicationView::Pcb,
            grid_visible: false,
            hierarchy_visible: false,
            inspector_visible: false,
            undo_available: false,
            redo_available: false,
            selection_available: false,
            select_all_available: false,
            copy_available: false,
            paste_available: false,
        });
        assert!(menu.contains_command(command::VIEW_SCHEMATIC));
        assert!(menu.contains_command(command::VIEW_PCB));
        assert!(!menu.contains_command(command::VIEW_SCENE));
    }

    #[test]
    fn unavailable_edit_commands_remain_visible_but_disabled() {
        let menu = build_application_menu(ApplicationMenuState {
            project_type: ProjectType::Game,
            active_view: ApplicationView::Scene,
            grid_visible: false,
            hierarchy_visible: false,
            inspector_visible: false,
            undo_available: false,
            redo_available: false,
            selection_available: false,
            select_all_available: false,
            copy_available: false,
            paste_available: false,
        });
        let edit = menu
            .menus
            .iter()
            .find(|menu| menu.id == "edit")
            .expect("edit menu");
        assert!(edit.items.iter().all(|item| match item {
            UiMenuItem::Command(command) => !command.enabled,
            UiMenuItem::Separator => true,
            UiMenuItem::Submenu(_) => false,
        }));
    }

    #[test]
    fn keyboard_shortcuts_live_under_help_as_an_expanded_submenu() {
        let menu = build_application_menu(ApplicationMenuState {
            project_type: ProjectType::Game,
            active_view: ApplicationView::Scene,
            grid_visible: true,
            hierarchy_visible: true,
            inspector_visible: true,
            undo_available: true,
            redo_available: true,
            selection_available: true,
            select_all_available: true,
            copy_available: true,
            paste_available: true,
        });

        let help = menu
            .menus
            .iter()
            .find(|menu| menu.id == "help")
            .expect("help menu");
        let shortcuts = help
            .items
            .iter()
            .find_map(|item| match item {
                UiMenuItem::Submenu(menu) if menu.id == command::HELP_KEYBOARD_SHORTCUTS => {
                    Some(menu)
                }
                _ => None,
            })
            .expect("keyboard shortcuts submenu");

        assert!(shortcuts.items.iter().any(
            |item| matches!(item, UiMenuItem::Command(command) if command.id == "help.shortcut.0")
        ));
        assert!(shortcuts.items.iter().any(
            |item| matches!(item, UiMenuItem::Command(command) if command.id == "help.shortcut.100")
        ));

        let edit = menu
            .menus
            .iter()
            .find(|menu| menu.id == "edit")
            .expect("edit menu");
        assert!(!edit.items.iter().any(
            |item| matches!(item, UiMenuItem::Command(command) if command.id.starts_with("help.shortcut."))
        ));
    }
}
