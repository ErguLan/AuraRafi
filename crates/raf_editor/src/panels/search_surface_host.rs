//! Native host for the global retained search overlay.

use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;

use super::search_surface::{build_search_surface_with_state, SearchResult, SearchSurfaceState};

#[derive(Debug, Clone, PartialEq)]
pub enum SearchIntent {
    QueryChanged(String),
    Activate(SearchResult),
    Close,
}

pub struct SearchSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    query: String,
    results: Vec<SearchResult>,
    state: SearchSurfaceState,
    open: bool,
}

impl SearchSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        Self {
            region: InputRegionId::from_static("native.editor.search"),
            rect,
            host: graphics.create_ui_host(
                build_search_surface_with_state(palette, "", &[], &SearchSurfaceState::Ready),
                [0, 0, 0, 0],
            ),
            query: String::new(),
            results: Vec::new(),
            state: SearchSurfaceState::Ready,
            open: false,
        }
    }

    pub fn set_results(&mut self, results: Vec<SearchResult>, state: SearchSurfaceState) {
        self.results = results;
        self.state = state;
    }

    pub fn sync(&mut self, palette: StudioUiPalette, rect: EditorRect) {
        self.rect = rect;
        self.host.set_surface(build_search_surface_with_state(
            palette,
            &self.query,
            &self.results,
            &self.state,
        ));
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<SearchIntent> {
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
                UiAction::SetText { key, value } if key == "search.query" => {
                    self.query = value.clone();
                    Some(SearchIntent::QueryChanged(value))
                }
                UiAction::Command { name } if name == "search.close" => {
                    self.open = false;
                    Some(SearchIntent::Close)
                }
                UiAction::Command { name } => {
                    search_result_from_command(&name, &self.results).map(SearchIntent::Activate)
                }
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

fn search_result_from_command(name: &str, results: &[SearchResult]) -> Option<SearchResult> {
    let index = name.strip_prefix("search.result.")?.parse::<usize>().ok()?;
    results.get(index).cloned()
}
