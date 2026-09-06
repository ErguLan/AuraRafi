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

    pub fn open(&mut self) {
        self.open = true;
        self.host
            .session_mut()
            .interaction
            .focus
            .request_focus("search.input");
    }

    pub fn close(&mut self) {
        self.open = false;
        self.host.session_mut().interaction.focus.clear_focus();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn owner(&self) -> InputOwner {
        InputOwner::RetainedUi(self.region)
    }

    pub fn rect(&self) -> EditorRect {
        self.rect
    }

    pub fn contains_point(&self, point: [f32; 2]) -> bool {
        self.rect.contains(point)
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    pub fn focused_text_rect(&self) -> Option<raf_ui::UiRect> {
        self.host.focused_text_rect()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat()
    }

    pub fn sync(&mut self, palette: StudioUiPalette, rect: EditorRect) {
        self.rect = rect;
        self.host.set_surface(build_search_surface_with_state(
            palette,
            &self.query,
            &self.results,
            &self.state,
        ));
        self.host
            .session_mut()
            .interaction
            .controls
            .set_text("search.query", &self.query, 256);
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
                    self.close();
                    Some(SearchIntent::Close)
                }
                UiAction::Command { name } if name == "search.activate-first" => {
                    self.results.first().cloned().map(SearchIntent::Activate)
                }
                UiAction::Command { name } if name == "search.focus-first" => {
                    if let Some(index) =
                        crate::panels::search_surface::search_result_order(&self.results)
                            .first()
                            .copied()
                    {
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(format!("search.result.{index}"));
                    }
                    None
                }
                UiAction::Command { name } if name.starts_with("search.focus-") => {
                    let (direction, raw_index) = name
                        .strip_prefix("search.focus-")
                        .and_then(|value| value.split_once(':'))
                        .unwrap_or(("", ""));
                    if let Ok(current) = raw_index.parse::<usize>() {
                        let ordered =
                            crate::panels::search_surface::search_result_order(&self.results);
                        if let Some(position) = ordered.iter().position(|index| *index == current) {
                            let next = match direction {
                                "next" => ordered.get(position + 1).or_else(|| ordered.first()),
                                "previous" => position
                                    .checked_sub(1)
                                    .and_then(|index| ordered.get(index))
                                    .or_else(|| ordered.last()),
                                _ => None,
                            };
                            if let Some(next) = next {
                                self.host
                                    .session_mut()
                                    .interaction
                                    .focus
                                    .request_focus(format!("search.result.{next}"));
                            }
                        }
                    }
                    None
                }
                UiAction::Command { name } if name == "search.focus-last" => {
                    if let Some(index) =
                        crate::panels::search_surface::search_result_order(&self.results)
                            .last()
                            .copied()
                    {
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(format!("search.result.{index}"));
                    }
                    None
                }
                UiAction::Command { name } if name.starts_with("search.activate:") => name
                    .strip_prefix("search.activate:")
                    .and_then(|value| value.parse::<usize>().ok())
                    .and_then(|index| self.results.get(index).cloned())
                    .map(SearchIntent::Activate),
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
