//! Live RafUI adapter for the Properties and Sessions selector.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_ui::StudioUiPalette;

use crate::editor_shell_surface::{
    build_editor_inspector_tabs_surface, editor_shell_intent, EditorInspectorTab, EditorShellIntent,
};
use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct EditorInspectorTabsHost {
    bridge: RafUiSurfaceBridge,
    icons_registered: bool,
}

impl Default for EditorInspectorTabsHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_inspector_tabs"),
            icons_registered: false,
        }
    }
}

impl EditorInspectorTabsHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        active: EditorInspectorTab,
    ) -> Option<EditorInspectorTab> {
        self.register_icons();
        let surface = build_editor_inspector_tabs_surface(palette, active);
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, language))
            .into_iter()
            .filter_map(|action| editor_shell_intent(action.action))
            .find_map(|intent| match intent {
                EditorShellIntent::SelectInspector(tab) => Some(tab),
                _ => None,
            })
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        for (key, bytes) in [
            (
                "editor.shell.inspector.properties-icon",
                include_bytes!("../../../../editor/assets/ui_icons/transform.png").as_slice(),
            ),
            (
                "editor.shell.inspector.sessions-icon",
                include_bytes!("../../../../editor/assets/ui_icons/scripts.png").as_slice(),
            ),
        ] {
            let _ = self.bridge.register_embedded_png(key, bytes);
        }
        self.icons_registered = true;
    }
}
