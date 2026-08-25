//! Native host for the Game viewport toolbar.

use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;

use super::viewport_toolbar_surface::{
    build_viewport_toolbar_surface, parse_viewport_toolbar_action, ViewportToolbarAction,
    ViewportToolbarState,
};

pub struct ViewportToolbarSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    state: ViewportToolbarState,
}

impl ViewportToolbarSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        Self {
            region: InputRegionId::from_static("native.editor.viewport-toolbar"),
            rect,
            host: graphics.create_ui_host(
                build_viewport_toolbar_surface(palette, default_toolbar_state()),
                [0, 0, 0, 0],
            ),
            state: default_toolbar_state(),
        }
    }

    pub fn sync(&mut self, palette: StudioUiPalette, rect: EditorRect) {
        self.rect = rect;
        self.host
            .set_surface(build_viewport_toolbar_surface(palette, self.state));
    }

    pub fn state(&self) -> ViewportToolbarState {
        self.state
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<ViewportToolbarAction> {
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, raf_core::Language::English),
            input,
            router,
            InputOwner::RetainedUi(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        actions
            .into_iter()
            .filter_map(|action| match action.action {
                UiAction::Command { .. } => parse_viewport_toolbar_action(&action.action),
                _ => None,
            })
            .collect()
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }
}

fn default_toolbar_state() -> ViewportToolbarState {
    ViewportToolbarState {
        select_mode: true,
        tool: super::viewport_toolbar_surface::ViewportTool::Select,
        render_style: super::viewport_toolbar_surface::ViewportRenderStyle::Solid,
        polygons_visible: false,
        grid_visible: true,
        labels_visible: true,
        view_mode: super::viewport_toolbar_surface::ViewportViewMode::View3d,
        view_menu_open: false,
        shading_menu_open: false,
        primitive_menu_open: false,
        building_style: raf_core::project::BuildingStyle::Free,
        building_menu_open: false,
        compact: false,
    }
}
