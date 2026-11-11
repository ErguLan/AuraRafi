//! Native host for the Game viewport toolbar.

use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::action_bar::{UiActionBarItem, UiActionBarPlan};
use raf_ui::UiThemeMetrics;

use crate::editor_layout::EditorRect;

use super::viewport_toolbar_surface::{
    build_viewport_toolbar_surface, build_viewport_toolbar_surface_from_plan,
    build_viewport_toolbar_surface_measured, parse_viewport_toolbar_action,
    parse_viewport_toolbar_select_action, ViewportToolbarAction, ViewportToolbarState,
};

pub struct ViewportToolbarSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    state: ViewportToolbarState,
    palette: StudioUiPalette,
    theme: UiThemeMetrics,
    language: raf_core::Language,
    toolbar_width: f32,
    action_items: Vec<UiActionBarItem>,
    action_plan: Option<UiActionBarPlan>,
    overflow_open: bool,
}

impl ViewportToolbarSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let surface = build_viewport_toolbar_surface(palette, default_toolbar_state());
        let host = graphics.create_ui_host(surface, [0, 0, 0, 0]);
        let theme = host.surface().theme.metrics;
        Self {
            region: InputRegionId::from_static("native.editor.viewport-toolbar"),
            rect,
            host,
            state: default_toolbar_state(),
            palette,
            theme,
            language: raf_core::Language::English,
            toolbar_width: rect.width,
            action_items: Vec::new(),
            action_plan: None,
            overflow_open: false,
        }
    }

    pub fn sync(&mut self, palette: StudioUiPalette, rect: EditorRect) {
        self.rect = rect;
        self.palette = palette;
        self.toolbar_width = rect.width;
        self.action_items.clear();
        self.action_plan = None;
        self.overflow_open = false;
        self.host
            .set_surface(build_viewport_toolbar_surface(palette, self.state));
    }

    /// Syncs with an actual localized text measurement. Overflow state is
    /// retained across layout updates and every overflow action remains
    /// available through the More popup.
    pub fn sync_measured(
        &mut self,
        palette: StudioUiPalette,
        rect: EditorRect,
        language: raf_core::Language,
        theme: UiThemeMetrics,
        measure_text: impl FnMut(&str) -> f32,
    ) {
        self.rect = rect;
        self.palette = palette;
        self.theme = theme;
        self.language = language;
        self.toolbar_width = rect.width;
        let (surface, items, plan) = build_viewport_toolbar_surface_measured(
            palette,
            self.state,
            rect.width,
            language,
            theme,
            measure_text,
            self.overflow_open,
        );
        self.action_items = items;
        self.action_plan = Some(plan);
        self.host.set_surface(surface);
    }

    pub fn state(&self) -> ViewportToolbarState {
        self.state
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<ViewportToolbarAction> {
        let language = self.language;
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            InputOwner::RetainedUi(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        let mut parsed = Vec::new();
        let mut surface_changed = false;
        for dispatched in actions {
            match &dispatched.action {
                UiAction::Command { .. } => {
                    if let Some(action) = parse_viewport_toolbar_action(&dispatched.action) {
                        parsed.push(action);
                    }
                }
                UiAction::SetSelect { key, value, .. } => {
                    if key == "viewport.toolbar.actions.more.selection" {
                        if let Some(item) = self.action_items.iter().find(|item| item.id == *value)
                        {
                            if let Some(action) = parse_viewport_toolbar_action(&item.action) {
                                parsed.push(action);
                            }
                        }
                    } else if let Some(action) = parse_viewport_toolbar_select_action(key, value) {
                        parsed.push(action);
                    }
                }
                UiAction::SetSelectOpen { id, open } => {
                    surface_changed |= self.update_select_open(id, *open);
                }
                _ => {}
            }
        }
        if surface_changed {
            self.rebuild_surface();
        }
        parsed
    }

    fn update_select_open(&mut self, id: &str, open: bool) -> bool {
        let current = match id {
            "viewport.toolbar.actions.more.menu" | "viewport.toolbar.actions.more.trigger" => {
                Some(&mut self.overflow_open)
            }
            "viewport.toolbar.view-mode.menu" => Some(&mut self.state.view_menu_open),
            "viewport.toolbar.shading.menu" => Some(&mut self.state.shading_menu_open),
            "viewport.toolbar.building-menu" => Some(&mut self.state.building_menu_open),
            _ => None,
        };
        let Some(current) = current else {
            return false;
        };
        if *current == open {
            return false;
        }
        *current = open;
        true
    }

    fn rebuild_surface(&mut self) {
        let surface = if let Some(plan) = self.action_plan.as_ref() {
            build_viewport_toolbar_surface_from_plan(
                self.palette,
                self.state,
                self.toolbar_width,
                &self.action_items,
                plan,
                self.overflow_open,
                self.theme,
            )
        } else {
            build_viewport_toolbar_surface(self.palette, self.state)
        };
        self.host.set_surface(surface);
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
        actions_menu_open: false,
        compact: false,
    }
}
