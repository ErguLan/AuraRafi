//! Native host/controller for engine settings.

use raf_core::ai::AiProvider;
use raf_core::config::{EngineSettings, Language};
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;
use crate::settings_surface::{build_settings_surface_with_api_keys, provider_id, SettingsSection};

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
    revealed_api_keys: Vec<AiProvider>,
}

impl SettingsSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let draft = EngineSettings::default();
        let mut host = Self {
            region: InputRegionId::from_static("native.editor.settings"),
            rect,
            host: graphics.create_ui_host(
                build_settings_surface_with_api_keys(
                    palette,
                    &draft,
                    SettingsSection::Appearance,
                    &[],
                ),
                [0, 0, 0, 0],
            ),
            draft,
            section: SettingsSection::Appearance,
            draft_dirty: false,
            surface_dirty: true,
            last_rect: None,
            revealed_api_keys: Vec::new(),
        };
        host.seed_numeric_inputs();
        host
    }

    pub fn reset_draft(&mut self, settings: &EngineSettings) {
        self.draft = settings.clone();
        self.draft_dirty = false;
        self.surface_dirty = true;
        self.revealed_api_keys.clear();
    }

    pub fn mark_dirty(&mut self) {
        self.draft_dirty = true;
        self.surface_dirty = true;
    }

    pub fn accept(&mut self) -> EngineSettings {
        self.draft_dirty = false;
        self.surface_dirty = true;
        self.draft.clone()
    }

    pub fn cancel(&mut self) {
        self.draft_dirty = false;
        self.surface_dirty = true;
        self.revealed_api_keys.clear();
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

    pub fn text(&self, key: &str) -> String {
        self.host
            .session()
            .interaction
            .controls
            .text(key)
            .to_string()
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
            self.surface_dirty = true;
        }
        self.rect = rect;
        if self.surface_dirty {
            self.host.set_surface(build_settings_surface_with_api_keys(
                palette,
                &self.draft,
                self.section,
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
                "settings.auto-save.text",
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
        ];
        let controls = &mut self.host.session_mut().interaction.controls;
        for (key, value) in values {
            controls.set_text(key, value, 32);
        }
        for config in &self.draft.ai_providers {
            let id = provider_id(config.provider);
            controls.set_text(
                &format!("settings.ai_provider.{id}.base_url"),
                &config.base_url,
                512,
            );
            controls.set_text(
                &format!("settings.ai_provider.{id}.model"),
                &config.model,
                512,
            );
            controls.set_text(
                &format!("settings.ai_provider.{id}.api_key"),
                &config.api_key,
                512,
            );
        }
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
