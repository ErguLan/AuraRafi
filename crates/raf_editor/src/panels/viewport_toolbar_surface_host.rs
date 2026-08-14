//! Host/controller for the retained Game viewport toolbar.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::UiAction;

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::viewport_toolbar_surface::{
    build_viewport_toolbar_surface, parse_viewport_toolbar_action, ViewportToolbarAction,
    ViewportToolbarState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SurfaceKey {
    palette: StudioUiPalette,
    state: ViewportToolbarState,
}

pub struct ViewportToolbarSurfaceHost {
    bridge: RafUiSurfaceBridge,
    cached_key: Option<SurfaceKey>,
    cached_surface: Option<UiSurface>,
    view_menu_open: bool,
}

impl Default for ViewportToolbarSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_viewport_toolbar"),
            cached_key: None,
            cached_surface: None,
            view_menu_open: false,
        }
    }
}

impl ViewportToolbarSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        mut state: ViewportToolbarState,
    ) -> Vec<ViewportToolbarAction> {
        state.view_menu_open = self.view_menu_open;
        let key = SurfaceKey { palette, state };
        if self.cached_key != Some(key) {
            self.cached_surface = Some(build_viewport_toolbar_surface(palette, state));
            self.cached_key = Some(key);
        }
        let Some(surface) = self.cached_surface.as_ref() else {
            return Vec::new();
        };
        let dispatched =
            self.bridge
                .show_transparent_ref(ui, render_state, palette, surface, |key| {
                    raf_core::i18n::t(key, language)
                });
        let mut actions = Vec::new();
        for dispatched_action in dispatched {
            if let UiAction::Command { name } = &dispatched_action.action {
                if name == "viewport.dropdown.view.toggle" {
                    self.view_menu_open = !self.view_menu_open;
                    continue;
                }
                if name == "viewport.view-2d" || name == "viewport.view-3d" {
                    self.view_menu_open = false;
                }
            }
            if let Some(action) = parse_viewport_toolbar_action(&dispatched_action.action) {
                actions.push(action);
            }
        }
        actions
    }

    pub fn reset(&mut self) {
        self.cached_key = None;
        self.cached_surface = None;
        self.view_menu_open = false;
        self.bridge.reset_surface_interaction(None);
    }
}
