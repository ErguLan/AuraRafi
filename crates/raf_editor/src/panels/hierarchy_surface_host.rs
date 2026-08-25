//! Native controller for the retained Hierarchy document.

use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;

use super::hierarchy_model::HierarchyModel;
use super::hierarchy_surface::build_hierarchy_surface;

pub struct HierarchySurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    model: HierarchyModel,
    search: String,
}

impl HierarchySurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let mut model = HierarchyModel::default();
        let view = model.refresh(&SceneGraph::new(), "", false, 0.0, rect.height, 26.0);
        Self {
            region: InputRegionId::from_static("native.editor.hierarchy"),
            rect,
            host: graphics.create_ui_host(
                build_hierarchy_surface(
                    palette,
                    &view,
                    &[],
                    None,
                    None,
                    false,
                    false,
                    None,
                    None,
                    [rect.width, rect.height],
                    26.0,
                    18.0,
                    true,
                    true,
                    true,
                    1.0,
                    None,
                    None,
                    None,
                    false,
                    rect.width < 444.0,
                    "hierarchy",
                    [false; 3],
                    None,
                ),
                [0, 0, 0, 0],
            ),
            model,
            search: String::new(),
        }
    }

    pub fn sync(&mut self, scene: &SceneGraph, selected: &[SceneNodeId], palette: StudioUiPalette) {
        let view = self
            .model
            .refresh(scene, &self.search, false, 0.0, self.rect.height, 26.0);
        self.host.set_surface(build_hierarchy_surface(
            palette,
            &view,
            selected,
            None,
            None,
            false,
            false,
            None,
            None,
            [self.rect.width, self.rect.height],
            26.0,
            18.0,
            true,
            true,
            true,
            1.0,
            None,
            None,
            None,
            false,
            self.rect.width < 444.0,
            "hierarchy",
            [false; 3],
            None,
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
