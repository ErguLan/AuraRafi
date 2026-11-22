use super::*;

use crate::console::LogLevel;

impl NativeGameWorkbench {
    pub(super) fn handle_set_toggle(
        &mut self,
        key: &String,
        value: &bool,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        if key.starts_with("inspector.") {
            if let Some(target) = selected.first().copied() {
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "inspector.toggle:{key}:{value}:{}",
                    target.0
                )));
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        } else if key.starts_with("project-settings.") {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            intents.push(NativeWorkbenchIntent::ProjectSettingToggle {
                key: key.clone(),
                value: *value,
            });
        } else if key == "console.auto-scroll" {
            if self.console.set_auto_scroll(*value) {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        } else if apply_settings_toggle(&mut self.agent_settings, key, *value) {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_settings_command(
        &mut self,
        name: &String,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(key) = name.strip_prefix("project-settings.commit_text:") {
            let value_key = format!("project-settings.{key}");
            let value = self
                .host
                .session()
                .interaction
                .controls
                .text(&value_key)
                .to_string();
            intents.push(NativeWorkbenchIntent::ProjectSettingText {
                key: value_key,
                value,
            });
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name.starts_with("project-settings.") {
            intents.push(NativeWorkbenchIntent::ProjectSettingCommand(name.clone()));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_console_command(
        &mut self,
        name: &String,
        project: Option<&Project>,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if name == "console.filters.toggle" || name == "console.filters.close" {
            self.console.filters_open =
                name == "console.filters.toggle" && !self.console.filters_open;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name.starts_with("console.filter.") {
            self.console.filters_open = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        match name.as_str() {
            "console.filter.all" => {
                if self.console.set_filter_level(None) {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            "console.filter.info" => {
                if self.console.set_filter_level(Some(LogLevel::Info)) {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            "console.filter.warning" => {
                if self.console.set_filter_level(Some(LogLevel::Warning)) {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            "console.filter.error" => {
                if self.console.set_filter_level(Some(LogLevel::Error)) {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            "console.submit" => {
                if let Some(submission) = self.console.submit_input() {
                    let text = submission.text;
                    self.console.log_user("User", &text);
                    self.host.session_mut().interaction.controls.set_text(
                        "console.input",
                        "",
                        4096,
                    );
                    if !self.performance_console_submission(&text, project) {
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "console.submit:{text}"
                        )));
                    }
                }
                return true;
            }
            _ if name.starts_with("console.block.toggle:") => {
                if let Some(raw_id) = name.strip_prefix("console.block.toggle:") {
                    if let Ok(id) = raw_id.parse::<u64>() {
                        if self.console.toggle_json_disclosure(id) {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                    }
                }
                return true;
            }
            "console.clear" => {
                self.console.clear_entries();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "console.history.previous" => {
                self.console.select_previous_history();
                let input = self.console.input().to_string();
                self.host
                    .session_mut()
                    .interaction
                    .controls
                    .set_text("console.input", input, 4096);
                return true;
            }
            "console.history.next" => {
                self.console.select_next_history();
                let input = self.console.input().to_string();
                self.host
                    .session_mut()
                    .interaction
                    .controls
                    .set_text("console.input", input, 4096);
                return true;
            }
            "console.autocomplete" => {
                let names = self
                    .agent_catalog
                    .commands
                    .iter()
                    .map(|command| format!("/{}", command.name))
                    .collect::<Vec<_>>();
                self.console.autocomplete_command(&names);
                let input = self.console.input().to_string();
                self.host
                    .session_mut()
                    .interaction
                    .controls
                    .set_text("console.input", input, 4096);
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_project_command(
        &mut self,
        name: &String,
        router: &mut InputRouter,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        match name.as_str() {
            "editor.settings" => {
                intents.push(NativeWorkbenchIntent::OpenSettings {
                    section: self.settings_section,
                });
                return true;
            }
            "project.settings" => {
                if self.bottom_dock.select("project-settings") {
                    self.reset_input_state(router);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            "application.exit_to_hub" | "project.close" => {
                intents.push(NativeWorkbenchIntent::ReturnToHub { open_create: false });
                return true;
            }
            "project.new" => {
                intents.push(NativeWorkbenchIntent::ReturnToHub { open_create: true });
                return true;
            }
            "project.open" => {
                intents.push(NativeWorkbenchIntent::ReturnToHub { open_create: false });
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_settings_section_command(
        &mut self,
        name: &String,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(section) = SettingsSection::from_command(name) {
            let section_changed = self.settings_section != section;
            self.settings_section = section;
            if section_changed {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            intents.push(NativeWorkbenchIntent::OpenSettings { section });
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_session_command(
        &mut self,
        name: &String,
        project: Option<&Project>,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(intent) = self.process_session_command(&name, project) {
            intents.push(intent);
            return true;
        }
        false
    }
}
