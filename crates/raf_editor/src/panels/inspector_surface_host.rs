//! Native controller for the retained Inspector surface.

use raf_core::project::ProjectType;
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::ProjectSessionRegistry;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;

use super::inspector_surface::{build_inspector_surface, InspectorViewState};

pub struct InspectorSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    view: InspectorViewState,
}

impl InspectorSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let sessions = ProjectSessionRegistry::new(ProjectType::Game);
        Self {
            region: InputRegionId::from_static("native.editor.inspector"),
            rect,
            host: graphics.create_ui_host(
                build_inspector_surface(
                    palette,
                    &SceneGraph::new(),
                    None,
                    &sessions,
                    1.0,
                    InspectorViewState::default(),
                ),
                [0, 0, 0, 0],
            ),
            view: InspectorViewState::default(),
        }
    }

    pub fn sync(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        project_type: ProjectType,
        palette: StudioUiPalette,
        rect: EditorRect,
    ) {
        self.rect = rect;
        let sessions = ProjectSessionRegistry::new(project_type);
        self.host.set_surface(build_inspector_surface(
            palette, scene, selected, &sessions, 1.0, self.view,
        ));
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<UiDispatchedAction> {
        self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, raf_core::Language::English),
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
