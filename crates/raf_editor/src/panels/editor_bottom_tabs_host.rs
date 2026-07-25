//! Live RafUI adapter for the editor's fixed bottom tab strip.
//!
//! Console, Assets, Project Settings, Node Editor, and Agent use the same
//! renderer-owned bottom navigation through ApiGraphicBasic.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_ui::StudioUiPalette;

use crate::editor_shell_surface::{
    build_editor_bottom_tabs_surface, editor_shell_intent, EditorBottomDockTab, EditorShellIntent,
};
use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct EditorBottomTabsHost {
    bridge: RafUiSurfaceBridge,
    icons_registered: bool,
}

impl Default for EditorBottomTabsHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_bottom_tabs"),
            icons_registered: false,
        }
    }
}

impl EditorBottomTabsHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        project_type: ProjectType,
        active: EditorBottomDockTab,
    ) -> Option<EditorBottomDockTab> {
        self.register_icons();
        let surface = build_editor_bottom_tabs_surface(
            palette,
            active,
            project_type == ProjectType::Electronics,
        );
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, language))
            .into_iter()
            .filter_map(|action| editor_shell_intent(action.action))
            .find_map(|intent| match intent {
                EditorShellIntent::SelectBottom(tab) => Some(tab),
                _ => None,
            })
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        for (key, bytes) in [
            (
                "editor.bottom.console",
                include_bytes!("../../../../editor/assets/ui_icons/console.png").as_slice(),
            ),
            (
                "editor.bottom.assets",
                include_bytes!("../../../../editor/assets/ui_icons/assets.png").as_slice(),
            ),
            (
                "editor.bottom.project-settings",
                include_bytes!("../../../../editor/assets/ui_icons/project_settings.png")
                    .as_slice(),
            ),
            (
                "editor.bottom.nodes",
                include_bytes!("../../../../editor/assets/ui_icons/node_editor.png").as_slice(),
            ),
            (
                "editor.bottom.agent",
                include_bytes!("../../../../editor/assets/ui_icons/ai_chat.png").as_slice(),
            ),
        ] {
            let _ = self.bridge.register_embedded_png(key, bytes);
        }
        self.icons_registered = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_host_uses_a_dedicated_retained_surface_bridge() {
        let _host = EditorBottomTabsHost::default();
    }
}
