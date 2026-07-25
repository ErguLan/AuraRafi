//! Live RafUI adapter for the center-surface context strip.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_ui::StudioUiPalette;

use crate::editor_shell_surface::{
    build_editor_context_tabs_surface, editor_shell_intent, EditorCenterSurface, EditorShellIntent,
};
use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct EditorContextTabsHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for EditorContextTabsHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_context_tabs"),
        }
    }
}

impl EditorContextTabsHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        project_type: ProjectType,
        active: EditorCenterSurface,
    ) -> Option<EditorCenterSurface> {
        let surface = build_editor_context_tabs_surface(palette, project_type, active);
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, language))
            .into_iter()
            .filter_map(|action| editor_shell_intent(action.action))
            .find_map(|intent| match intent {
                EditorShellIntent::SelectCenter(surface) => Some(surface),
                _ => None,
            })
    }
}
