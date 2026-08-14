//! Application host for the retained global search overlay.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::search_surface::{
    build_search_surface_with_state, search_result_order, SearchResult, SearchSurfaceState,
};

#[derive(Debug, Clone, PartialEq)]
pub enum SearchIntent {
    QueryChanged(String),
    Activate(SearchResult),
    Close,
}

pub struct SearchSurfaceHost {
    bridge: RafUiSurfaceBridge,
    open: bool,
    query: String,
    results: Vec<SearchResult>,
    state: SearchSurfaceState,
    focus_input_on_open: bool,
    cached_key: Option<(
        StudioUiPalette,
        String,
        Vec<SearchResult>,
        SearchSurfaceState,
    )>,
    cached_surface: Option<raf_render::api_graphic_basic::ui_surface::UiSurface>,
}

impl Default for SearchSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_global_search"),
            open: false,
            query: String::new(),
            results: Vec::new(),
            state: SearchSurfaceState::Ready,
            focus_input_on_open: false,
            cached_key: None,
            cached_surface: None,
        }
    }
}

impl SearchSurfaceHost {
    pub fn open(&mut self, query: impl Into<String>) {
        let query = query.into().chars().take(256).collect::<String>();
        if self.query != query {
            self.query = query;
            self.results.clear();
            self.state = SearchSurfaceState::Loading;
            self.cached_key = None;
        }
        self.open = true;
        self.focus_input_on_open = true;
        self.bridge.cancel_pointer_gesture();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.focus_input_on_open = false;
        self.bridge.cancel_pointer_gesture();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Publishes one immutable result snapshot. An empty snapshot is ready and
    /// is rendered as a real no-results state, not as an indeterminate loader.
    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        if self.results != results || !matches!(self.state, SearchSurfaceState::Ready) {
            self.results = results;
            self.state = SearchSurfaceState::Ready;
            self.cached_key = None;
        }
    }

    pub fn set_loading(&mut self) {
        if !matches!(self.state, SearchSurfaceState::Loading) || !self.results.is_empty() {
            self.results.clear();
            self.state = SearchSurfaceState::Loading;
            self.cached_key = None;
        }
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.results.clear();
        self.state = SearchSurfaceState::Error(error.into());
        self.cached_key = None;
    }

    pub fn clear_error(&mut self) {
        if matches!(self.state, SearchSurfaceState::Error(_)) {
            self.state = SearchSurfaceState::Ready;
            self.cached_key = None;
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state, SearchSurfaceState::Loading)
    }

    pub fn error(&self) -> Option<&str> {
        match &self.state {
            SearchSurfaceState::Error(error) => Some(error.as_str()),
            _ => None,
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
    ) -> Vec<SearchIntent> {
        if !self.open {
            return Vec::new();
        }

        let key = (
            palette,
            self.query.clone(),
            self.results.clone(),
            self.state.clone(),
        );
        if self.cached_key.as_ref() != Some(&key) {
            self.cached_surface = Some(build_search_surface_with_state(
                palette,
                &self.query,
                &self.results,
                &self.state,
            ));
            self.cached_key = Some(key);
        }
        let Some(surface) = self.cached_surface.as_ref() else {
            return Vec::new();
        };
        let query = self.query.clone();
        let results = self.results.clone();
        let viewport = ctx.screen_rect();
        let width = (viewport.width() - 24.0).max(1.0).min(560.0);
        let top_inset = 48.0_f32.min((viewport.height() - 1.0).max(0.0));
        let height = (viewport.height() - top_inset - 16.0).max(1.0).min(520.0);
        let rect = egui::Rect::from_min_size(
            egui::pos2(
                viewport.left() + ((viewport.width() - width) * 0.5).max(0.0),
                viewport.top() + top_inset,
            ),
            egui::vec2(width, height),
        );
        let actions = egui::Area::new(egui::Id::new("rafui.global-search"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    rect.size(),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.bridge.show_with_control_state_ref(
                            ui,
                            render_state,
                            palette,
                            surface,
                            |controls| controls.set_text("search.query", query.clone(), 256),
                            |key| t(key, language),
                        )
                    },
                )
                .inner
            })
            .inner;

        let mut intents = Vec::new();
        let mut closed = false;
        for action in actions {
            match action.action {
                UiAction::SetText { key, value } if key == "search.query" => {
                    let value = value.chars().take(256).collect::<String>();
                    if self.query != value {
                        self.query = value.clone();
                        self.results.clear();
                        self.state = SearchSurfaceState::Loading;
                        self.cached_key = None;
                        intents.push(SearchIntent::QueryChanged(value));
                    }
                }
                UiAction::Command { name } if name == "search.close" => {
                    self.close();
                    closed = true;
                    intents.push(SearchIntent::Close);
                }
                UiAction::Command { name } if name == "search.activate-first" => {
                    if let Some(index) = search_result_order(&results).first().copied() {
                        let Some(result) = results.get(index).cloned() else {
                            continue;
                        };
                        self.close();
                        closed = true;
                        intents.push(SearchIntent::Activate(result));
                    }
                }
                UiAction::Command { name } if name == "search.focus-first" => {
                    self.focus_result(search_result_order(&results).first().copied());
                }
                UiAction::Command { name } if name == "search.focus-last" => {
                    self.focus_result(search_result_order(&results).last().copied());
                }
                UiAction::Command { name } if name.starts_with("search.focus-next:") => {
                    self.focus_relative(&results, &name, true);
                }
                UiAction::Command { name } if name.starts_with("search.focus-previous:") => {
                    self.focus_relative(&results, &name, false);
                }
                UiAction::Command { name } if name.starts_with("search.activate:") => {
                    let Some(index) = name
                        .strip_prefix("search.activate:")
                        .and_then(|value| value.parse::<usize>().ok())
                    else {
                        continue;
                    };
                    if let Some(result) = results.get(index).cloned() {
                        self.close();
                        closed = true;
                        intents.push(SearchIntent::Activate(result));
                    }
                }
                _ => {}
            }
        }

        if self.open && self.bridge.input_snapshot().pointer_pressed_outside {
            self.close();
            closed = true;
            intents.push(SearchIntent::Close);
        }
        if self.open && !closed && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close();
            intents.push(SearchIntent::Close);
        }
        if self.open && self.focus_input_on_open {
            self.bridge.request_focus("search.input");
            self.focus_input_on_open = false;
            ctx.request_repaint();
        }
        intents
    }

    fn focus_result(&mut self, index: Option<usize>) {
        if let Some(index) = index {
            self.bridge.request_focus(format!("search.result.{index}"));
        } else {
            self.bridge.request_focus("search.input");
        }
    }

    fn focus_relative(&mut self, results: &[SearchResult], command: &str, next: bool) {
        let Some(index) = command
            .rsplit_once(':')
            .and_then(|(_, value)| value.parse::<usize>().ok())
        else {
            self.focus_result(None);
            return;
        };
        let order = search_result_order(results);
        let Some(position) = order.iter().position(|candidate| *candidate == index) else {
            self.focus_result(order.first().copied());
            return;
        };
        if !next && position == 0 {
            self.focus_result(None);
            return;
        }
        let target = if next {
            order
                .get(position + 1)
                .copied()
                .or_else(|| order.first().copied())
        } else {
            position
                .checked_sub(1)
                .and_then(|previous| order.get(previous).copied())
        };
        self.focus_result(target);
    }
}
