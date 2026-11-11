//! On-demand cost snapshots; no diagnostic idle rendering loop.
use super::*;
use raf_ui::{UiPerformanceSnapshot, UiWorkCause};

#[derive(Default)]
pub(super) struct UiPerformanceView {
    pub open: bool,
    pub snapshot: UiPerformanceSnapshot,
    pub selected: Option<usize>,
}

impl NativeGameWorkbench {
    fn capture_ui_performance(&mut self) {
        self.ui_performance.snapshot = self.host.performance_snapshot();
        let extra = self.electronics_overlay_host.performance_snapshot();
        self.ui_performance.snapshot.append(extra);
        self.ui_performance.selected = None;
    }

    pub(super) fn handle_ui_performance_command(&mut self, command: &str) -> bool {
        match command {
            "ui.performance" => {
                self.ui_performance.open = !self.ui_performance.open;
                if self.ui_performance.open {
                    self.capture_ui_performance();
                }
            }
            "ui.performance.refresh" => self.capture_ui_performance(),
            "ui.performance.clear" => {
                self.host.session_mut().performance.clear();
                self.electronics_overlay_host
                    .session_mut()
                    .performance
                    .clear();
                self.host.images_mut().clear_performance();
                self.electronics_overlay_host
                    .images_mut()
                    .clear_performance();
                self.capture_ui_performance();
            }
            "ui.performance.close" => self.ui_performance.open = false,
            _ => {
                let Some(index) = command
                    .strip_prefix("ui.performance.region:")
                    .and_then(|i| i.parse::<usize>().ok())
                else {
                    return false;
                };
                if index >= self.ui_performance.snapshot.regions.len() {
                    return true;
                }
                self.ui_performance.selected = Some(index);
            }
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        true
    }

    pub(super) fn performance_console_submission(
        &mut self,
        text: &str,
        project: Option<&Project>,
    ) -> bool {
        let command = match text.trim() {
            "/ui.performance" => "ui.performance",
            "/ui.performance refresh" => "ui.performance.refresh",
            "/ui.performance clear" => "ui.performance.clear",
            "/ui.performance close" => "ui.performance.close",
            _ => return false,
        };
        let language = self.agent_settings.language;
        let title = raf_core::i18n::t("ui.performance.title", language);
        if !self.agent_settings.command_console_enabled
            || !project.is_some_and(|p| p.settings.enable_console_commands)
        {
            self.log_console_output(crate::commands::CommandOutput::error(
                title,
                raf_core::i18n::t("ui.performance.console_disabled", language),
            ));
            return true;
        }
        self.handle_ui_performance_command(command);
        self.log_console_output(crate::commands::CommandOutput::info(
            title,
            vec![raf_core::i18n::t("ui.performance.snapshot_hint", language)],
            serde_json::to_value(&self.ui_performance.snapshot).unwrap_or_default(),
        ));
        true
    }

    pub(super) fn workbench_change_cause(
        &self,
        layout: EditorFrameLayout,
        scene_revision: u64,
        selected: &[SceneNodeId],
    ) -> UiWorkCause {
        if self.last_layout != Some(layout) {
            UiWorkCause::Geometry
        } else if self.last_scene_fingerprint != scene_revision || self.last_selection != selected {
            UiWorkCause::Document
        } else if self.has_active_ui_motion() {
            UiWorkCause::Animation
        } else {
            UiWorkCause::Explicit
        }
    }
}
