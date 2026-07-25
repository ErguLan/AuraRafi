//! ApiGraphicBasic host for the retained Console surface.

use std::collections::BTreeSet;

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction, UiDispatchedAction};

use crate::console_surface::build_console_surface;
use crate::panels::console::{ConsolePanel, ConsoleSubmission, LogLevel};
use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct ConsoleSurfaceHost {
    bridge: RafUiSurfaceBridge,
    expanded_blocks: BTreeSet<usize>,
    icons_registered: bool,
}

impl Default for ConsoleSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_console_surface"),
            expanded_blocks: BTreeSet::new(),
            icons_registered: false,
        }
    }
}

impl ConsoleSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        console: &mut ConsolePanel,
        input_enabled: bool,
        command_names: &[String],
        language: Language,
    ) -> Vec<ConsoleSubmission> {
        self.register_icons();
        self.expanded_blocks
            .retain(|index| *index < console.entries.len());
        let input = console.input().to_string();
        let expanded_blocks = self.expanded_blocks.iter().copied().collect::<Vec<_>>();
        let auto_scroll = console.auto_scroll;
        let surface = build_console_surface(palette, console, input_enabled, &expanded_blocks);
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                controls.set_text("console.input", input.clone(), 4_096);
                if auto_scroll {
                    controls.scroll_by("console.entries", [0.0, 16_384.0]);
                }
            },
            |key| t(key, language),
        );
        self.apply_actions(actions, console, command_names)
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        let _ = self.bridge.register_embedded_png(
            "editor.console.send",
            include_bytes!("../../../../editor/assets/ui_icons/send.png"),
        );
        self.icons_registered = true;
    }

    fn apply_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        console: &mut ConsolePanel,
        command_names: &[String],
    ) -> Vec<ConsoleSubmission> {
        let mut submissions = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetToggle { key, value } if key == "console.auto-scroll" => {
                    console.auto_scroll = value;
                }
                UiAction::SetText { key, value } if key == "console.input" => {
                    console.set_input(value);
                }
                UiAction::Command { name } => match name.as_str() {
                    "console.clear" => {
                        console.clear_entries();
                        self.expanded_blocks.clear();
                    }
                    "console.filter.all" => console.filter_level = None,
                    "console.filter.info" => console.filter_level = Some(LogLevel::Info),
                    "console.filter.warning" => console.filter_level = Some(LogLevel::Warning),
                    "console.filter.error" => console.filter_level = Some(LogLevel::Error),
                    "console.submit" => {
                        if let Some(submission) = console.submit_input() {
                            submissions.push(submission);
                        }
                    }
                    "console.autocomplete" => console.autocomplete_command(command_names),
                    "console.history.previous" => console.select_previous_history(),
                    "console.history.next" => console.select_next_history(),
                    _ if name.starts_with("console.block.toggle.") => {
                        if let Some(index) = name
                            .strip_prefix("console.block.toggle.")
                            .and_then(|value| value.parse::<usize>().ok())
                        {
                            if !self.expanded_blocks.insert(index) {
                                self.expanded_blocks.remove(&index);
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        submissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_action_removes_entries_and_expanded_block_state() {
        let mut host = ConsoleSurfaceHost::default();
        host.expanded_blocks.insert(0);
        let mut console = ConsolePanel::default();
        let _ = host.apply_actions(
            vec![UiDispatchedAction {
                target_id: "console.clear".to_string(),
                event: raf_ui::UiEventKind::Click,
                action: UiAction::Command {
                    name: "console.clear".to_string(),
                },
            }],
            &mut console,
            &[],
        );
        assert!(console.entries.is_empty());
        assert!(host.expanded_blocks.is_empty());
    }

    #[test]
    fn submit_action_returns_a_typed_console_submission() {
        let mut host = ConsoleSurfaceHost::default();
        let mut console = ConsolePanel::default();
        console.set_input("/scene.list".to_string());
        let submissions = host.apply_actions(
            vec![UiDispatchedAction {
                target_id: "console.input".to_string(),
                event: raf_ui::UiEventKind::KeyPress("enter".to_string()),
                action: UiAction::Command {
                    name: "console.submit".to_string(),
                },
            }],
            &mut console,
            &[],
        );
        assert_eq!(submissions.len(), 1);
        assert_eq!(submissions[0].text, "/scene.list");
        assert!(console.input().is_empty());
    }
}
