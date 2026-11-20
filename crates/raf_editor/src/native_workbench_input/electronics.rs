use super::*;

impl NativeGameWorkbench {
    pub(super) fn handle_electronics_commit_command(&mut self, name: &String, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        if let Some(target) = name.strip_prefix("electronics.pcb.rule.commit:") {
            let key = format!("electronics.pcb.rule.{target}");
            let raw = self.host.session().interaction.controls.text(&key).trim().replace(',', ".");
            if let Ok(value) = raw.parse::<f32>() {
                if value.is_finite() && value > 0.0 && value <= 10.0 {
                    intents.push(NativeWorkbenchIntent::Command(format!("electronics.pcb.rule:{target}:{value}")));
                    self.host.session_mut().interaction.focus.clear_focus();
                }
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(target)=name.strip_prefix("electronics.inspector.transform.commit:") {
            let key=format!("electronics.inspector.transform.{target}");
            let raw=self.host.session().interaction.controls.text(&key).trim().replace(',',".");
            if let Ok(value)=raw.parse::<f32>() {
                if value.is_finite() && value.abs()<=1_000_000.0 {
                    intents.push(NativeWorkbenchIntent::Command(format!("electronics.transform:{target}:{value}")));
                    self.host.session_mut().interaction.focus.clear_focus();
                }
            }
            self.toolbar_revision=self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_electronics_project_command(&mut self, name: &String, project: Option<&Project>, router: &mut InputRouter, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        if self.project_type == ProjectType::Electronics {
            if matches!(name.as_str(), "electronics.analysis.drc" | "electronics.analysis.simulation" | "electronics.fabrication.export" | "electronics.fabrication.deliver") {
                crate::native_workbench::electronics_model::reveal_analysis(&mut self.bottom_dock, name);
                self.host.session_mut().interaction.controls.reset_scroll("electronics.analysis.lines");
                self.reset_input_state(router);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            let workspace_command = if name == crate::application_menu::command::VIEW_HIERARCHY {
                Some("electronics.workspace.library.toggle")
            } else if name == crate::application_menu::command::VIEW_INSPECTOR {
                Some("electronics.workspace.inspector.toggle")
            } else { None };
            if workspace_command.is_some_and(|command| self.handle_electronics_workspace_command(command)) {
                return true;
            }
        }
        if self.handle_electronics_workspace_command(name) {
            return true;
        }
        if self.handle_footprint_chooser_command(name, project, intents) {
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_electronics_navigator_command(&mut self, name: &String) -> bool {
        if let Some(tab) = name.strip_prefix("electronics.navigator.tab:") {
            if matches!(
                tab,
                "project" | "library" | "components" | "wires" | "objects" | "layers"
            ) {
                if self.footprint_chooser.is_open() {self.footprint_chooser.close();}
                self.electronics_navigator_tab = tab.to_string();
                if self.last_layout.is_some_and(|layout| layout.window.width < 980.0) {
                    self.electronics_workspace.inspector_hidden = true;
                }
                if self.electronics_workspace.pcb_active {
                    self.electronics_workspace.pcb_open = true;
                } else {
                    self.electronics_workspace.schematic_collapsed = false;
                }
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_electronics_value_commit_command(&mut self, name: &String, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        match name.as_str() {
            "electronics.inspector.value.commit" => {
                let value = self
                    .host
                    .session()
                    .interaction
                    .controls
                    .text("electronics.inspector.value")
                    .to_string();
                if let Some(mut draft) = self.electronics_value_draft.take() {
                    draft.value = value;
                    if let electronics_model::InspectorValueEditEnd::Apply {
                        source_id,
                        value,
                    } = draft.finish()
                    {
                        intents.push(NativeWorkbenchIntent::Command(
                            crate::electronics_action::ElectronicsAction::CommitInspectorValueFor {
                                source_id,
                                value,
                            }
                            .to_command_string(),
                        ));
                    }
                }
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            _ => {}
        }
        false
    }
}

/// True for a retained target that belongs to the Electronics context menu.
///
/// The menu is published as a root node plus one node per item, so both prefixes
/// have to be recognized. A press on the menu itself must not be treated as the
/// outside click that dismisses it; the item commands already close the menu.
pub(super) fn is_electronics_context_menu_target(target: &str) -> bool {
    target == "electronics.context-menu" || target.starts_with("electronics.context.")
}
