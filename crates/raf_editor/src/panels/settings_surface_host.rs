//! Native host/controller for engine settings.

use raf_core::ai::AiProvider;
use raf_core::config::EngineSettings;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;
use crate::settings_surface::{
    build_settings_surface_with_state_and_api_keys, provider_id, SettingsSection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSurfaceIntent {
    Changed,
    Close,
}

pub struct SettingsSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    pub draft: EngineSettings,
    section: SettingsSection,
    draft_dirty: bool,
    surface_dirty: bool,
    last_rect: Option<EditorRect>,
    modal_rect: raf_ui::UiRect,
    drag_anchor: Option<[f32; 2]>,
    resize_anchor: Option<([f32; 2], raf_ui::UiRect)>,
    search_query: String,
    open_select: Option<String>,
    revealed_api_keys: Vec<AiProvider>,
}

const MODAL_MARGIN: f32 = 24.0;
const MODAL_BOUNDS_MARGIN: f32 = 16.0;
const MODAL_MIN_WIDTH: f32 = 720.0;
const MODAL_MIN_HEIGHT: f32 = 520.0;
const MODAL_DEFAULT_MAX_WIDTH: f32 = 1080.0;
const MODAL_DEFAULT_MAX_HEIGHT: f32 = 720.0;
const MODAL_MAX_WIDTH: f32 = 1440.0;
const MODAL_MAX_HEIGHT: f32 = 960.0;

fn default_modal_rect(rect: EditorRect) -> raf_ui::UiRect {
    let viewport_width = rect.width.max(1.0);
    let viewport_height = rect.height.max(1.0);
    let available_width = (viewport_width - MODAL_MARGIN * 2.0).max(1.0);
    let available_height = (viewport_height - MODAL_MARGIN * 2.0).max(1.0);
    let width = available_width
        .min(MODAL_DEFAULT_MAX_WIDTH)
        .max(MODAL_MIN_WIDTH.min(available_width));
    let height = available_height
        .min(MODAL_DEFAULT_MAX_HEIGHT)
        .max(MODAL_MIN_HEIGHT.min(available_height));
    raf_ui::UiRect::new(
        ((viewport_width - width) * 0.5).max(0.0),
        ((viewport_height - height) * 0.5).max(0.0),
        width,
        height,
    )
}

fn modal_bounds(rect: EditorRect) -> raf_ui::UiRect {
    raf_ui::UiRect::new(
        MODAL_BOUNDS_MARGIN,
        MODAL_BOUNDS_MARGIN,
        (rect.width - MODAL_BOUNDS_MARGIN * 2.0).max(1.0),
        (rect.height - MODAL_BOUNDS_MARGIN * 2.0).max(1.0),
    )
}

fn clamp_modal_rect(modal: raf_ui::UiRect, rect: EditorRect) -> raf_ui::UiRect {
    let bounds = modal_bounds(rect);
    let min_width = MODAL_MIN_WIDTH.min(bounds.width);
    let min_height = MODAL_MIN_HEIGHT.min(bounds.height);
    let max_width = MODAL_MAX_WIDTH.min(bounds.width).max(min_width);
    let max_height = MODAL_MAX_HEIGHT.min(bounds.height).max(min_height);
    let width = modal.width.clamp(min_width, max_width);
    let height = modal.height.clamp(min_height, max_height);
    raf_ui::UiRect::new(modal.x, modal.y, width, height).clamp_inside(bounds)
}

fn resize_modal_rect(
    start: raf_ui::UiRect,
    origin: [f32; 2],
    point: [f32; 2],
    rect: EditorRect,
) -> raf_ui::UiRect {
    clamp_modal_rect(
        raf_ui::UiRect::new(
            start.x,
            start.y,
            start.width + point[0] - origin[0],
            start.height + point[1] - origin[1],
        ),
        rect,
    )
}

impl SettingsSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let draft = EngineSettings::default();
        let modal_rect = default_modal_rect(rect);
        let mut host = Self {
            region: InputRegionId::from_static("native.editor.settings"),
            rect,
            host: graphics.create_ui_host(
                build_settings_surface_with_state_and_api_keys(
                    palette,
                    &draft,
                    SettingsSection::Appearance,
                    modal_rect,
                    "",
                    None,
                    &[],
                ),
                [0, 0, 0, 0],
            ),
            draft,
            section: SettingsSection::Appearance,
            draft_dirty: false,
            surface_dirty: true,
            last_rect: None,
            modal_rect,
            drag_anchor: None,
            resize_anchor: None,
            search_query: String::new(),
            open_select: None,
            revealed_api_keys: Vec::new(),
        };
        host.seed_numeric_inputs();
        host
    }

    pub fn owner(&self) -> InputOwner {
        InputOwner::Modal(self.region)
    }

    pub fn reset_input_state(&mut self, router: &mut InputRouter) {
        self.host
            .session_mut()
            .reset_interaction_for_surface_change(None);
        router.cancel_owner(self.owner());
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.open_select = None;
        self.revealed_api_keys.clear();
        self.host
            .session_mut()
            .interaction
            .focus
            .request_focus("settings.search");
    }

    pub fn reset_draft(&mut self, settings: &EngineSettings) {
        self.draft = settings.clone();
        self.draft_dirty = false;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.search_query.clear();
        self.open_select = None;
        self.revealed_api_keys.clear();
        self.surface_dirty = true;
    }

    /// Restores editable preferences to their engine defaults. Window
    /// geometry is intentionally preserved because it is shell state, not a
    /// setting the modal currently exposes. The user can still cancel before
    /// applying the reset.
    pub fn restore_defaults(&mut self) {
        let window_width = self.draft.window_width;
        let window_height = self.draft.window_height;
        let window_maximized = self.draft.window_maximized;
        self.draft = EngineSettings::default();
        self.draft.window_width = window_width;
        self.draft.window_height = window_height;
        self.draft.window_maximized = window_maximized;
        self.draft_dirty = true;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.search_query.clear();
        self.open_select = None;
        self.revealed_api_keys.clear();
        self.surface_dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.draft_dirty
    }

    pub fn section(&self) -> SettingsSection {
        self.section
    }

    pub fn set_section(&mut self, section: SettingsSection) {
        if self.section != section {
            self.section = section;
            self.open_select = None;
            self.surface_dirty = true;
        }
    }

    pub fn modal_rect(&self) -> raf_ui::UiRect {
        self.modal_rect
    }

    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        self.host.cursor_hint()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat()
    }

    pub fn set_select_open(&mut self, id: &str, open: bool) {
        self.open_select = open.then(|| id.to_string());
        self.surface_dirty = true;
    }

    pub fn toggle_api_key(&mut self, provider: AiProvider) {
        if let Some(index) = self
            .revealed_api_keys
            .iter()
            .position(|candidate| *candidate == provider)
        {
            self.revealed_api_keys.remove(index);
        } else {
            self.revealed_api_keys.push(provider);
        }
        self.surface_dirty = true;
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if self.search_query == query {
            return;
        }
        self.search_query = query;
        if !self.search_query.trim().is_empty() {
            if let Some(section) = SettingsSection::ALL
                .into_iter()
                .find(|section| section.matches_query(&self.search_query))
            {
                self.section = section;
            }
        }
        self.open_select = None;
        self.surface_dirty = true;
    }

    pub fn mark_dirty(&mut self) {
        self.draft_dirty = true;
        self.surface_dirty = true;
    }

    pub fn mark_draft_dirty(&mut self) {
        self.draft_dirty = true;
    }

    pub fn commit_numeric_drafts(&mut self) -> bool {
        const NUMERIC_KEYS: [&str; 19] = [
            "settings.theme_experimental",
            "settings.font_size",
            "settings.ui_scale",
            "settings.fps_limit",
            "settings.grid_size",
            "settings.grid_load_distance",
            "settings.electronics_grid_step_mm",
            "settings.electronics_grid_opacity",
            "settings.auto_save",
            "settings.hierarchy_row_height",
            "settings.hierarchy_indent_width",
            "settings.wasd_speed",
            "settings.gizmo_growth_scale",
            "settings.move_sensitivity",
            "settings.rotate_sensitivity",
            "settings.scale_sensitivity",
            "settings.script_timeout_ms",
            "settings.agent_max_response_tokens",
            "settings.agent_max_tool_calls",
        ];
        let mut changed = false;
        for key in NUMERIC_KEYS {
            if !self.numeric_input_enabled(key) {
                continue;
            }
            let text = self.text(&format!("{key}.text"));
            let Ok(value) = text.trim().parse::<f32>() else {
                continue;
            };
            let before = self.draft.clone();
            if crate::native_workbench::apply_settings_range(&mut self.draft, key, value)
                && self.draft != before
            {
                changed = true;
            }
        }
        if changed {
            self.draft_dirty = true;
        }
        changed
    }

    fn numeric_input_enabled(&self, key: &str) -> bool {
        if key == "settings.fps_limit" && self.draft.fps_limit == 0 {
            return false;
        }
        if key == "settings.ui_scale" && self.draft.auto_ui_scale {
            return false;
        }
        if key == "settings.agent_max_tool_calls" && !self.draft.agent_tool_call_limit_enabled {
            return false;
        }
        if self.draft.simple_mode
            && matches!(
                key,
                "settings.grid_size" | "settings.grid_load_distance" | "settings.auto_save"
            )
        {
            return false;
        }
        true
    }

    pub fn accept(&mut self) -> EngineSettings {
        self.draft_dirty = false;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.surface_dirty = true;
        self.draft.clone()
    }

    pub fn cancel(&mut self, committed: &EngineSettings) {
        self.draft = committed.clone();
        self.draft_dirty = false;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.revealed_api_keys.clear();
        self.surface_dirty = true;
    }

    pub fn text(&self, key: &str) -> String {
        self.host
            .session()
            .interaction
            .controls
            .text(key)
            .to_string()
    }

    pub fn focused_text_rect(&self) -> Option<raf_ui::UiRect> {
        let rect = self.host.focused_text_rect()?;
        Some(raf_ui::UiRect::new(
            rect.x + self.rect.x,
            rect.y + self.rect.y,
            rect.width,
            rect.height,
        ))
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn sync(
        &mut self,
        palette: StudioUiPalette,
        settings: &EngineSettings,
        section: SettingsSection,
        rect: EditorRect,
    ) {
        if !self.draft_dirty && self.draft != *settings {
            self.draft = settings.clone();
            self.surface_dirty = true;
        }
        if self.section != section {
            self.section = section;
            self.surface_dirty = true;
        }
        if self.last_rect != Some(rect) {
            self.modal_rect = clamp_modal_rect(self.modal_rect, rect);
            self.surface_dirty = true;
        }
        self.rect = rect;
        if self.surface_dirty {
            self.host
                .set_surface(build_settings_surface_with_state_and_api_keys(
                    palette,
                    &self.draft,
                    self.section,
                    self.modal_rect,
                    &self.search_query,
                    self.open_select.as_deref(),
                    &self.revealed_api_keys,
                ));
            self.seed_numeric_inputs();
            self.last_rect = Some(rect);
            self.surface_dirty = false;
        }
    }

    fn seed_numeric_inputs(&mut self) {
        let values = [
            (
                "settings.theme_experimental.text",
                format!("{:.0}", self.draft.theme_experimental),
            ),
            (
                "settings.font_size.text",
                format!("{:.0}", self.draft.font_size),
            ),
            (
                "settings.ui_scale.text",
                format!("{:.1}", self.draft.ui_scale),
            ),
            (
                "settings.fps_limit.text",
                self.draft.fps_limit.max(15).to_string(),
            ),
            (
                "settings.grid_size.text",
                format!("{:.1}", self.draft.grid_size),
            ),
            (
                "settings.grid_load_distance.text",
                format!("{:.1}", self.draft.grid_load_distance),
            ),
            (
                "settings.electronics_grid_step_mm.text",
                format!("{:.0}", self.draft.electronics_grid_step_mm),
            ),
            (
                "settings.electronics_grid_opacity.text",
                format!("{:.2}", self.draft.electronics_grid_opacity),
            ),
            (
                "settings.auto_save.text",
                self.draft.auto_save_interval_seconds.to_string(),
            ),
            (
                "settings.hierarchy_row_height.text",
                format!("{:.0}", self.draft.hierarchy_row_height),
            ),
            (
                "settings.hierarchy_indent_width.text",
                format!("{:.0}", self.draft.hierarchy_indent_width),
            ),
            (
                "settings.wasd_speed.text",
                format!("{:.2}", self.draft.wasd_speed),
            ),
            (
                "settings.gizmo_growth_scale.text",
                format!("{:.0}", self.draft.gizmo_growth_scale),
            ),
            (
                "settings.move_sensitivity.text",
                format!("{:.2}", self.draft.move_gizmo_sensitivity),
            ),
            (
                "settings.rotate_sensitivity.text",
                format!("{:.2}", self.draft.rotate_gizmo_sensitivity),
            ),
            (
                "settings.scale_sensitivity.text",
                format!("{:.2}", self.draft.scale_gizmo_sensitivity),
            ),
            (
                "settings.script_timeout_ms.text",
                self.draft.script_timeout_ms.to_string(),
            ),
            (
                "settings.agent_max_response_tokens.text",
                self.draft.agent_max_response_tokens.to_string(),
            ),
            (
                "settings.agent_max_tool_calls.text",
                self.draft.agent_max_tool_calls.to_string(),
            ),
            (
                "settings.script_external_editor.text",
                self.draft.script_external_editor_cmd.clone(),
            ),
            (
                "settings.script_external_editor",
                self.draft.script_external_editor_cmd.clone(),
            ),
            ("settings.search", self.search_query.clone()),
        ];
        let ai_values = raf_core::ai::AiProvider::all()
            .iter()
            .filter_map(|provider| {
                let id = provider_id(*provider);
                let config = self
                    .draft
                    .ai_providers
                    .iter()
                    .find(|config| config.provider == *provider)?;
                Some([
                    (
                        format!("settings.ai_provider.{id}.base_url"),
                        config.base_url.clone(),
                    ),
                    (
                        format!("settings.ai_provider.{id}.model"),
                        config.model.clone(),
                    ),
                    (
                        format!("settings.ai_provider.{id}.api_key"),
                        config.api_key.clone(),
                    ),
                ])
            })
            .flatten()
            .collect::<Vec<_>>();
        let controls = &mut self.host.session_mut().interaction.controls;
        for (key, value) in values {
            let max_length = match key {
                "settings.search" => 256,
                "settings.script_external_editor.text" | "settings.script_external_editor" => 256,
                _ => 32,
            };
            controls.set_text(key, value, max_length);
        }
        for (key, value) in ai_values {
            controls.set_text(key, value, 512);
        }
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<UiDispatchedAction> {
        let language = self.draft.language;
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            InputOwner::Modal(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );

        let pointer = self.host.session().interaction.pointer_position();
        for dispatched in &actions {
            let UiAction::Command { name } = &dispatched.action else {
                continue;
            };
            match name.as_str() {
                "settings.modal.drag.start" => {
                    self.resize_anchor = None;
                    self.drag_anchor = pointer
                        .map(|point| [point[0] - self.modal_rect.x, point[1] - self.modal_rect.y]);
                }
                "settings.modal.drag.move" => {
                    if let (Some(anchor), Some(point)) = (self.drag_anchor, pointer) {
                        let next = clamp_modal_rect(
                            raf_ui::UiRect::new(
                                point[0] - anchor[0],
                                point[1] - anchor[1],
                                self.modal_rect.width,
                                self.modal_rect.height,
                            ),
                            self.rect,
                        );
                        if next != self.modal_rect {
                            self.modal_rect = next;
                            self.surface_dirty = true;
                        }
                    }
                }
                "settings.modal.drag.end" => self.drag_anchor = None,
                "settings.modal.resize.start" => {
                    self.drag_anchor = None;
                    self.resize_anchor = pointer.map(|point| (point, self.modal_rect));
                }
                "settings.modal.resize.move" => {
                    if let (Some((origin, start)), Some(point)) = (self.resize_anchor, pointer) {
                        let next = resize_modal_rect(start, origin, point, self.rect);
                        if next != self.modal_rect {
                            self.modal_rect = next;
                            self.surface_dirty = true;
                        }
                    }
                }
                "settings.modal.resize.end" => self.resize_anchor = None,
                _ => {}
            }
        }
        actions
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_modal_fits_the_editor_with_safe_margins() {
        let viewport = EditorRect::new(0.0, 0.0, 1280.0, 800.0);
        let modal = default_modal_rect(viewport);

        assert_eq!(modal.width, MODAL_DEFAULT_MAX_WIDTH);
        assert_eq!(modal.height, MODAL_DEFAULT_MAX_HEIGHT);
        assert!(modal.x >= MODAL_MARGIN);
        assert!(modal.y >= MODAL_MARGIN);
        assert!(modal.right() <= viewport.width);
        assert!(modal.bottom() <= viewport.height);
    }

    #[test]
    fn narrow_editor_keeps_the_modal_inside_the_viewport() {
        let viewport = EditorRect::new(0.0, 0.0, 767.0, 643.0);
        let modal = default_modal_rect(viewport);

        assert!(modal.x >= 0.0);
        assert!(modal.y >= 0.0);
        assert!(modal.right() <= viewport.width);
        assert!(modal.bottom() <= viewport.height);
        assert!(modal.width < MODAL_MIN_WIDTH);
        assert!(modal.height > MODAL_MIN_HEIGHT);
    }

    #[test]
    fn resize_grows_from_the_bottom_right_and_respects_limits() {
        let viewport = EditorRect::new(0.0, 0.0, 1600.0, 1000.0);
        let start = raf_ui::UiRect::new(120.0, 80.0, 720.0, 520.0);

        let grown = resize_modal_rect(start, [840.0, 600.0], [1080.0, 760.0], viewport);
        assert_eq!(grown.x, start.x);
        assert_eq!(grown.y, start.y);
        assert_eq!(grown.width, 960.0);
        assert_eq!(grown.height, 680.0);

        let clamped = resize_modal_rect(start, [840.0, 600.0], [5000.0, 5000.0], viewport);
        assert!(clamped.width <= MODAL_MAX_WIDTH);
        assert!(clamped.height <= MODAL_MAX_HEIGHT);
        assert!(clamped.right() <= viewport.width - MODAL_BOUNDS_MARGIN);
        assert!(clamped.bottom() <= viewport.height - MODAL_BOUNDS_MARGIN);
    }
}
