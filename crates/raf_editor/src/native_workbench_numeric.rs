//! Project settings numeric editing over the shared RafUI transient contract.

use super::*;
use raf_ui::numeric::UiNumericEditAction;

impl NativeGameWorkbench {
    pub(super) fn sync_project_numeric_fields(&mut self, surface: &mut UiSurface) {
        let Some(root) = surface.root.find_mut("project-settings.root") else {
            return;
        };
        // Units belong to the form's authored meaning; numeric bounds still
        // come exclusively from its range controls.
        for (key, unit) in [
            (
                "project-settings.building-snap-step",
                raf_ui::UiNumericUnit::Meters,
            ),
            (
                "project-settings.stream-region-size",
                raf_ui::UiNumericUnit::Meters,
            ),
            (
                "project-settings.progress-interval",
                raf_ui::UiNumericUnit::Seconds,
            ),
        ] {
            self.project_numeric_edits.set_unit(key, unit);
        }
        self.project_numeric_edits.merge_tree(root);
        for binding in self.project_numeric_edits.bindings() {
            if let Some(node) = root.find_mut(&binding.node_id) {
                node.invalid = self.project_numeric_edits.is_invalid(&binding.key);
            }
            if let Some(text) = self.project_numeric_edits.draft_text(&binding.key) {
                self.host
                    .session_mut()
                    .interaction
                    .controls
                    .set_text(&binding.text_key, text, 32);
            }
        }
    }

    /// Consumes only numeric project-settings actions. All accepted changes
    /// still travel through the existing project intent/domain boundary.
    pub(super) fn handle_project_numeric_action(
        &mut self,
        action: &UiAction,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        match action {
            UiAction::SetText { key, value } if key.starts_with("project-settings.") => {
                let Some(binding) = self
                    .project_numeric_edits
                    .binding_for_text_key(key)
                    .cloned()
                else {
                    return false;
                };
                let invalid_before = self.project_numeric_edits.is_invalid(&binding.key);
                if self
                    .project_numeric_edits
                    .edit_text(key, value.clone())
                    .is_none()
                {
                    return false;
                }
                // The retained text control already owns the raw text. Rebuild
                // its projection only when validation presentation changes.
                if invalid_before != self.project_numeric_edits.is_invalid(&binding.key) {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
            UiAction::SetRange { key, value } if key.starts_with("project-settings.") => {
                let Some(text) = self.project_numeric_edits.format_value(key, *value) else {
                    return false;
                };
                if !value.is_finite() {
                    return true;
                }
                self.project_numeric_edits.cancel(key);
                self.project_numeric_edits.sync_committed(key, *value, text);
                intents.push(NativeWorkbenchIntent::ProjectSettingRange {
                    key: key.clone(),
                    value: *value,
                });
            }
            UiAction::Command { name } if name.starts_with("project-settings.commit_numeric:") => {
                // Unknown/disabled pairs cannot fall through to the old raw
                // float parser and bypass the authored range validation.
                if let Some(UiNumericEditAction::Committed { key, value }) =
                    self.project_numeric_edits.commit_for_submit(name)
                {
                    intents.push(NativeWorkbenchIntent::ProjectSettingRange { key, value });
                }
            }
            _ => return false,
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        true
    }

    pub(super) fn cancel_project_numeric_input(&mut self, focused: Option<&str>) -> bool {
        let Some(binding) = focused
            .and_then(|id| self.project_numeric_edits.binding_for_node(id))
            .cloned()
        else {
            return false;
        };
        if let Some(UiNumericEditAction::Cancelled { restored_text, .. }) =
            self.project_numeric_edits.cancel(&binding.key)
        {
            self.host.session_mut().interaction.controls.set_text(
                &binding.text_key,
                restored_text,
                32,
            );
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        true
    }
}
