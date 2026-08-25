//! Native host/controller for project-local settings.

use raf_core::config::Language;
use raf_core::project::Project;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;
use crate::project_settings_surface::build_project_settings_surface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSettingsSurfaceIntent {
    Changed,
    Close,
}

pub struct ProjectSettingsSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
}

impl ProjectSettingsSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
        project: &Project,
    ) -> Self {
        Self {
            region: InputRegionId::from_static("native.editor.project-settings"),
            rect,
            host: graphics.create_ui_host(
                build_project_settings_surface(palette, project, false),
                [0, 0, 0, 0],
            ),
        }
    }

    pub fn sync(&mut self, palette: StudioUiPalette, project: &Project, rect: EditorRect) {
        self.rect = rect;
        self.host
            .set_surface(build_project_settings_surface(palette, project, false));
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<UiDispatchedAction> {
        self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, Language::English),
            input,
            router,
            InputOwner::RetainedUi(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        )
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
