//! Declarative application-menu model shared by native shells and RafUI.
//!
//! This is intentionally separate from `UiNodeKind::Menu`: retained nodes are
//! in-surface context menus, while this model represents the File/Edit/View
//! style command bar that a platform shell may install natively.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiApplicationMenu {
    pub menus: Vec<UiMenu>,
}

impl UiApplicationMenu {
    pub fn command_ids(&self) -> Vec<&str> {
        let mut commands = Vec::new();
        for menu in &self.menus {
            menu.collect_command_ids(&mut commands);
        }
        commands
    }

    pub fn contains_command(&self, id: &str) -> bool {
        self.command_ids().into_iter().any(|command| command == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiMenu {
    pub id: String,
    pub label_key: String,
    pub items: Vec<UiMenuItem>,
}

impl UiMenu {
    pub fn new(id: impl Into<String>, label_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label_key: label_key.into(),
            items: Vec::new(),
        }
    }

    pub fn with_item(mut self, item: UiMenuItem) -> Self {
        self.items.push(item);
        self
    }

    fn collect_command_ids<'a>(&'a self, commands: &mut Vec<&'a str>) {
        for item in &self.items {
            match item {
                UiMenuItem::Command(command) => commands.push(&command.id),
                UiMenuItem::Submenu(menu) => menu.collect_command_ids(commands),
                UiMenuItem::Separator => {}
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiMenuItem {
    Command(UiMenuCommand),
    Separator,
    Submenu(UiMenu),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiMenuCommand {
    /// Stable application command identifier. Native adapters return this to
    /// the application boundary; they never execute project logic directly.
    pub id: String,
    pub label_key: String,
    #[serde(default)]
    pub accelerator: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub checked: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl UiMenuCommand {
    pub fn new(id: impl Into<String>, label_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label_key: label_key.into(),
            accelerator: None,
            enabled: true,
            checked: false,
        }
    }

    pub fn with_accelerator(mut self, accelerator: impl Into<String>) -> Self {
        self.accelerator = Some(accelerator.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMenuActivation {
    pub command_id: String,
}

impl UiMenuActivation {
    pub fn new(command_id: impl Into<String>) -> Self {
        Self {
            command_id: command_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_nested_command_ids_without_rendering_a_surface_menu() {
        let menu = UiApplicationMenu {
            menus: vec![UiMenu::new("file", "app.file")
                .with_item(UiMenuItem::Command(UiMenuCommand::new(
                    "project.save",
                    "app.save_menu",
                )))
                .with_item(UiMenuItem::Submenu(
                    UiMenu::new("recent", "app.recent").with_item(UiMenuItem::Command(
                        UiMenuCommand::new("project.open-recent", "app.open_project"),
                    )),
                ))],
        };

        assert_eq!(
            menu.command_ids(),
            vec!["project.save", "project.open-recent"]
        );
        assert!(menu.contains_command("project.save"));
        assert!(!menu.contains_command("project.delete"));
    }
}
