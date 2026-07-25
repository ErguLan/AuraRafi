//! Transitional host for the retained global Settings surface.
//!
//! Eframe supplies the window loop and final texture placement while RafUI
//! and ApiGraphicBasic own document construction, input, composition, and
//! the GPU-first presentation path.

use std::sync::Arc;

use eframe::{egui, egui_wgpu, wgpu};
use raf_core::ai::{AgentMode, AiProvider, AiProviderConfig};
use raf_core::config::{
    EngineSettings, Language, RenderExecutionPolicy, RenderQuality, ScriptLanguage, TargetPlatform,
    Theme, ViewportRenderMode,
};
use raf_core::i18n::t;
use raf_core::units::DisplayUnit;
use raf_render::api_graphic_basic::device::{GpuTextureView, SceneFrameOutput};
use raf_render::api_graphic_basic::ui_surface::{
    CpuUiSurfaceHost, DirectUiSurfaceHost, StudioUiPalette, UiAction, UiDispatchedAction,
    UiInputState, UiPointerButton, UiSurface,
};

use crate::panels::gpu_canvas::GpuCanvas;
use crate::settings_surface::{
    build_settings_surface, provider_from_id, provider_id, SettingsSection,
};

const SETTINGS_CLEAR_DARK: [u8; 4] = [8, 11, 15, 255];
const SETTINGS_CLEAR_LIGHT: [u8; 4] = [250, 250, 250, 255];
const SETTINGS_MIN_RASTER_SCALE: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSurfaceIntent {
    Save,
    Cancel,
}

struct GpuSettingsSurface {
    host: DirectUiSurfaceHost,
    texture: wgpu::Texture,
    view: Arc<wgpu::TextureView>,
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

impl GpuSettingsSurface {
    fn new(
        surface: UiSurface,
        render_state: &egui_wgpu::RenderState,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) -> Self {
        let format = render_state.target_format;
        let (texture, view) = create_target_texture(render_state.device.as_ref(), format, size);
        Self {
            host: DirectUiSurfaceHost::new(
                surface,
                render_state.device.as_ref(),
                format,
                clear_color,
            ),
            texture,
            view,
            size,
            format,
        }
    }

    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.size == size {
            return;
        }
        let (texture, view) = create_target_texture(device, self.format, size);
        self.texture = texture;
        self.view = view;
        self.size = size;
    }
}

/// Owns transient RafUI session state and translates retained UI actions into
/// the existing `EngineSettings` draft. It never persists settings itself.
pub struct SettingsSurfaceHost {
    canvas: GpuCanvas,
    gpu: Option<GpuSettingsSurface>,
    cpu: Option<CpuUiSurfaceHost>,
    settings: Option<EngineSettings>,
    language: Option<Language>,
    palette: Option<StudioUiPalette>,
    section: SettingsSection,
    visible_api_keys: Vec<AiProvider>,
}

impl Default for SettingsSurfaceHost {
    fn default() -> Self {
        Self {
            canvas: GpuCanvas::new("raf_ui_settings_surface").with_retained_ui_sampling(),
            gpu: None,
            cpu: None,
            settings: None,
            language: None,
            palette: None,
            section: SettingsSection::default(),
            visible_api_keys: Vec::new(),
        }
    }
}

impl SettingsSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        settings: &mut EngineSettings,
    ) -> Vec<SettingsSurfaceIntent> {
        let rect = ui.available_rect_before_wrap();
        let _response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let logical_size = [
            rect.width().round().max(1.0) as u32,
            rect.height().round().max(1.0) as u32,
        ];
        let pixels_per_point = ui.ctx().pixels_per_point().clamp(0.5, 4.0);
        let raster_scale = pixels_per_point.max(SETTINGS_MIN_RASTER_SCALE);
        let target_size = physical_size(logical_size, pixels_per_point);
        let clear_color = clear_color(palette);
        self.sync_surface(render_state, palette, settings, target_size, clear_color);

        let input = egui_input(ui.ctx(), rect);
        let language = settings.language;
        let (actions, hovered_changed) =
            if let (Some(render_state), Some(gpu)) = (render_state, self.gpu.as_mut()) {
                gpu.resize(render_state.device.as_ref(), target_size);
                let hovered_before = gpu.host.session().interaction.focus.hovered.clone();
                let actions = gpu.host.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_settings_text(key, language),
                    &input,
                );
                gpu.host.render_at_scale(
                    render_state.device.as_ref(),
                    render_state.queue.as_ref(),
                    gpu.view.as_ref(),
                    target_size,
                    logical_size,
                    raster_scale,
                    |key| resolve_settings_text(key, language),
                );
                self.canvas.present(
                    ui.ctx(),
                    Some(render_state),
                    SceneFrameOutput::GpuTexture {
                        view: GpuTextureView::from_wgpu(
                            gpu.view.clone(),
                            raf_render::api_graphic_basic::TextureHandle::new(0, 1),
                        ),
                        width: target_size[0],
                        height: target_size[1],
                    },
                    target_size[0],
                    target_size[1],
                );
                (
                    actions,
                    hovered_before != gpu.host.session().interaction.focus.hovered,
                )
            } else {
                let cpu = self
                    .cpu
                    .as_mut()
                    .expect("CPU Settings host must be prepared");
                let hovered_before = cpu.session().interaction.focus.hovered.clone();
                let actions = cpu.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_settings_text(key, language),
                    &input,
                );
                let frame = cpu.render_at_scale(target_size, logical_size, raster_scale, |key| {
                    resolve_settings_text(key, language)
                });
                self.canvas.present(
                    ui.ctx(),
                    None,
                    SceneFrameOutput::CpuPixels(frame.pixels.to_vec()),
                    frame.size[0],
                    frame.size[1],
                );
                (
                    actions,
                    hovered_before != cpu.session().interaction.focus.hovered,
                )
            };
        self.canvas.paint(&ui.painter_at(rect), rect);

        let actions_changed = !actions.is_empty();
        let intents = self.resolve_actions(actions, settings);
        if actions_changed || !intents.is_empty() || hovered_changed {
            ui.ctx().request_repaint();
        }
        intents
    }

    fn sync_surface(
        &mut self,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        settings: &EngineSettings,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) {
        let settings_changed = self.settings.as_ref() != Some(settings);
        let language_changed = self.language != Some(settings.language);
        let palette_changed = self.palette != Some(palette);
        let surface =
            build_settings_surface(palette, settings, self.section, &self.visible_api_keys);
        self.settings = Some(settings.clone());
        self.language = Some(settings.language);
        self.palette = Some(palette);

        if let Some(render_state) = render_state {
            let recreate = self
                .gpu
                .as_ref()
                .map(|gpu| gpu.format != render_state.target_format || palette_changed)
                .unwrap_or(true);
            if recreate {
                self.gpu = Some(GpuSettingsSurface::new(
                    surface.clone(),
                    render_state,
                    size,
                    clear_color,
                ));
            } else if settings_changed || language_changed {
                let gpu = self.gpu.as_mut().expect("GPU Settings host must exist");
                *gpu.host.surface_mut() = surface.clone();
                gpu.host.session_mut().text_atlas.clear();
            }
        }

        if render_state.is_none() {
            if self.cpu.is_none() || palette_changed {
                self.cpu = Some(CpuUiSurfaceHost::new(surface, clear_color));
            } else if settings_changed || language_changed {
                let cpu = self.cpu.as_mut().expect("CPU Settings host must exist");
                *cpu.surface_mut() = surface;
                cpu.session_mut().text_atlas.clear();
            }
        }
        self.seed_inputs(settings);
    }

    fn seed_inputs(&mut self, settings: &EngineSettings) {
        self.with_control_state(|controls| {
            controls.set_text(
                "settings.script-external-editor",
                &settings.script_external_editor_cmd,
                2_048,
            );
            for provider in AiProvider::editor_supported() {
                let id = provider_id(*provider);
                let config = settings
                    .ai_providers
                    .iter()
                    .find(|config| config.provider == *provider)
                    .cloned()
                    .unwrap_or_else(|| AiProviderConfig::for_provider(*provider));
                controls.set_text(format!("settings.ai.{id}.base-url"), config.base_url, 2_048);
                controls.set_text(format!("settings.ai.{id}.model"), config.model, 2_048);
                controls.set_text(format!("settings.ai.{id}.api-key"), config.api_key, 2_048);
            }
        });
    }

    fn with_control_state(&mut self, mut apply: impl FnMut(&mut raf_ui::UiControlState)) {
        if let Some(gpu) = self.gpu.as_mut() {
            apply(&mut gpu.host.session_mut().interaction.controls);
        }
        if let Some(cpu) = self.cpu.as_mut() {
            apply(&mut cpu.session_mut().interaction.controls);
        }
    }

    fn resolve_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        settings: &mut EngineSettings,
    ) -> Vec<SettingsSurfaceIntent> {
        let mut intents = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetToggle { key, value } => apply_toggle(settings, &key, value),
                UiAction::SetRange { key, value } => apply_range(settings, &key, value),
                UiAction::SetText { key, value } => apply_text(settings, &key, value),
                UiAction::Command { name } => {
                    if let Some(section) = SettingsSection::from_command(&name) {
                        if self.section != section {
                            self.section = section;
                            self.settings = None;
                        }
                        continue;
                    }
                    match name.as_str() {
                        "settings.save" => intents.push(SettingsSurfaceIntent::Save),
                        "settings.cancel" => intents.push(SettingsSurfaceIntent::Cancel),
                        "settings.theme.dark" => settings.theme = Theme::Dark,
                        "settings.theme.light" => settings.theme = Theme::Light,
                        "settings.theme.system" => settings.theme = Theme::System,
                        "settings.language.english" => settings.language = Language::English,
                        "settings.language.spanish" => settings.language = Language::Spanish,
                        "settings.quality.potato" => {
                            settings.render_quality = RenderQuality::Potato
                        }
                        "settings.quality.low" => settings.render_quality = RenderQuality::Low,
                        "settings.quality.medium" => {
                            settings.render_quality = RenderQuality::Medium
                        }
                        "settings.quality.high" => settings.render_quality = RenderQuality::High,
                        "settings.policy.auto" => {
                            settings.render_execution_policy = RenderExecutionPolicy::Auto
                        }
                        "settings.policy.cpu" => {
                            settings.render_execution_policy = RenderExecutionPolicy::CpuOnly
                        }
                        "settings.policy.gpu" => {
                            settings.render_execution_policy = RenderExecutionPolicy::GpuPreferred
                        }
                        "settings.units.metric" => settings.display_unit = DisplayUnit::Metric,
                        "settings.units.imperial" => settings.display_unit = DisplayUnit::Imperial,
                        "settings.units.game" => settings.display_unit = DisplayUnit::Game,
                        "settings.viewport.solid" => {
                            settings.viewport_render_mode = ViewportRenderMode::Solid
                        }
                        "settings.viewport.wireframe" => {
                            settings.viewport_render_mode = ViewportRenderMode::Wireframe
                        }
                        "settings.viewport.preview" => {
                            settings.viewport_render_mode = ViewportRenderMode::Preview
                        }
                        "settings.script-language.rhai" => {
                            settings.default_script_language = ScriptLanguage::Rhai
                        }
                        "settings.script-language.cpp" => {
                            settings.default_script_language = ScriptLanguage::Cpp
                        }
                        "settings.script-language.nodes" => {
                            settings.default_script_language = ScriptLanguage::Nodes
                        }
                        "settings.default-provider.openrouter" => {
                            settings.default_ai_provider = AiProvider::OpenRouter
                        }
                        "settings.default-provider.openai" => {
                            settings.default_ai_provider = AiProvider::OpenAI
                        }
                        "settings.agent-mode.passive" => settings.agent_mode = AgentMode::Passive,
                        "settings.agent-mode.active" => settings.agent_mode = AgentMode::Active,
                        "settings.platform.desktop" => {
                            settings.target_platform = TargetPlatform::Desktop
                        }
                        "settings.platform.mobile" => {
                            settings.target_platform = TargetPlatform::Mobile
                        }
                        "settings.platform.web" => settings.target_platform = TargetPlatform::Web,
                        "settings.platform.cloud" => {
                            settings.target_platform = TargetPlatform::Cloud
                        }
                        "settings.platform.console" => {
                            settings.target_platform = TargetPlatform::Console
                        }
                        _ if name.starts_with("settings.provider.toggle-key.") => {
                            if let Some(provider) = name
                                .strip_prefix("settings.provider.toggle-key.")
                                .and_then(provider_from_id)
                            {
                                toggle_api_key_visibility(&mut self.visible_api_keys, provider);
                                self.settings = None;
                            }
                        }
                        _ if name.starts_with("settings.provider.default.") => {
                            if let Some(provider) = name
                                .strip_prefix("settings.provider.default.")
                                .and_then(provider_from_id)
                            {
                                settings.default_ai_provider = provider;
                            }
                        }
                        _ if name.starts_with("settings.shortcut.remove.") => {
                            if let Some(index) = name
                                .strip_prefix("settings.shortcut.remove.")
                                .and_then(|value| value.parse::<usize>().ok())
                            {
                                if index < settings.agent_model_shortcuts.len() {
                                    settings.agent_model_shortcuts.remove(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        intents
    }
}

fn apply_toggle(settings: &mut EngineSettings, key: &str, value: bool) {
    match key {
        "settings.simple-mode" => settings.simple_mode = value,
        "settings.auto-ui-scale" => settings.auto_ui_scale = value,
        "settings.fps-unlimited" => {
            settings.fps_limit = if value { 0 } else { settings.fps_limit.max(60) }
        }
        "settings.show-fps-counter" => settings.show_fps_counter = value,
        "settings.vsync" => settings.vsync = value,
        "settings.multithreading" => settings.multithreading = value,
        "settings.show-grid" => settings.grid_visible = value,
        "settings.snap-grid" => settings.snap_to_grid = value,
        "settings.command-console" => settings.command_console_enabled = value,
        "settings.viewport-labels" => settings.show_viewport_labels = value,
        "settings.focus-lock" => settings.focus_lock_enabled = value,
        "settings.solid-edges" => settings.solid_show_surface_edges = value,
        "settings.solid-xray" => settings.solid_xray_mode = value,
        "settings.solid-tonality" => settings.solid_face_tonality = value,
        "settings.invert-x" => settings.invert_mouse_x = value,
        "settings.invert-y" => settings.invert_mouse_y = value,
        "settings.invert-ws" => settings.invert_ws = value,
        "settings.uniform-scale" => settings.uniform_scale_by_default = value,
        "settings.script-runtime" => settings.script_runtime_enabled = value,
        "settings.script-hot-reload" => settings.script_hot_reload = value,
        "settings.responsive-layout" => settings.responsive_layout = value,
        "settings.headless" => settings.headless = value,
        _ if key.starts_with("settings.ai.") && key.ends_with(".enabled") => {
            if let Some(provider) = key
                .strip_prefix("settings.ai.")
                .and_then(|value| value.strip_suffix(".enabled"))
                .and_then(provider_from_id)
            {
                provider_config_mut(settings, provider).enabled = value;
            }
        }
        _ => {}
    }
}

fn apply_range(settings: &mut EngineSettings, key: &str, value: f32) {
    match key {
        "settings.theme-experimental" => settings.theme_experimental = value.clamp(0.0, 100.0),
        "settings.font-size" => settings.font_size = value.clamp(10.0, 24.0),
        "settings.ui-scale" => settings.ui_scale = value.clamp(0.5, 3.0),
        "settings.fps-limit" if settings.fps_limit != 0 => {
            settings.fps_limit = value.round().clamp(15.0, 240.0) as u32
        }
        "settings.grid-size" => settings.grid_size = value.clamp(0.1, 10.0),
        "settings.grid-load-distance" => settings.grid_load_distance = value.clamp(0.0, 500.0),
        "settings.auto-save" => {
            settings.auto_save_interval_seconds = value.round().clamp(30.0, 600.0) as u32
        }
        "settings.wasd-speed" => settings.wasd_speed = value.clamp(0.5, 5.0),
        "settings.move-sensitivity" => settings.move_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.rotate-sensitivity" => settings.rotate_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.scale-sensitivity" => settings.scale_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.gizmo-growth" => settings.gizmo_growth_scale = value.clamp(0.0, 100.0),
        "settings.script-timeout" => {
            settings.script_timeout_ms = value.round().clamp(10.0, 1_000.0) as u32
        }
        _ => {}
    }
}

fn apply_text(settings: &mut EngineSettings, key: &str, value: String) {
    match key {
        "settings.script-external-editor" => settings.script_external_editor_cmd = value,
        _ if key.starts_with("settings.ai.") => {
            let Some(key) = key.strip_prefix("settings.ai.") else {
                return;
            };
            let Some((provider, field)) = key.split_once('.') else {
                return;
            };
            let Some(provider) = provider_from_id(provider) else {
                return;
            };
            let config = provider_config_mut(settings, provider);
            match field {
                "base-url" => config.base_url = value,
                "model" => config.model = value,
                "api-key" => config.api_key = value,
                _ => {}
            }
        }
        _ => {}
    }
}

fn provider_config_mut(
    settings: &mut EngineSettings,
    provider: AiProvider,
) -> &mut AiProviderConfig {
    if let Some(index) = settings
        .ai_providers
        .iter()
        .position(|config| config.provider == provider)
    {
        return &mut settings.ai_providers[index];
    }
    settings
        .ai_providers
        .push(AiProviderConfig::for_provider(provider));
    settings
        .ai_providers
        .last_mut()
        .expect("provider configuration was appended")
}

fn toggle_api_key_visibility(visible: &mut Vec<AiProvider>, provider: AiProvider) {
    if let Some(index) = visible.iter().position(|candidate| *candidate == provider) {
        visible.remove(index);
    } else {
        visible.push(provider);
    }
}

fn create_target_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> (wgpu::Texture, Arc<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ApiGraphicBasic.RafUiSettingsTarget"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
    (texture, view)
}

fn physical_size(logical_size: [u32; 2], pixels_per_point: f32) -> [u32; 2] {
    [
        ((logical_size[0].max(1) as f32 * pixels_per_point)
            .round()
            .max(1.0)) as u32,
        ((logical_size[1].max(1) as f32 * pixels_per_point)
            .round()
            .max(1.0)) as u32,
    ]
}

fn clear_color(palette: StudioUiPalette) -> [u8; 4] {
    match palette {
        StudioUiPalette::IndustrialDark => SETTINGS_CLEAR_DARK,
        StudioUiPalette::PaperLight => SETTINGS_CLEAR_LIGHT,
    }
}

fn resolve_settings_text(key: &str, language: Language) -> String {
    match key {
        "settings.surface.subtitle" => t("settings.surface.subtitle", language),
        "settings.surface.draft_hint" => t("settings.surface.draft_hint", language),
        "settings.surface.theme_preview" => t("settings.surface.theme_preview", language),
        "settings.surface.input" => t("settings.surface.input", language),
        "settings.surface.shortcuts_empty" => t("settings.surface.shortcuts_empty", language),
        "settings.surface.remove" => t("settings.surface.remove", language),
        "settings.value.dark" => t("settings.value.dark", language),
        "settings.value.light" => t("settings.value.light", language),
        "settings.value.system" => t("settings.value.system", language),
        "settings.value.english" => t("settings.value.english", language),
        "settings.value.spanish" => t("settings.value.spanish", language),
        "settings.value.potato" => t("settings.value.potato", language),
        "settings.value.low" => t("settings.value.low", language),
        "settings.value.medium" => t("settings.value.medium", language),
        "settings.value.high" => t("settings.value.high", language),
        "settings.value.game_units" => t("settings.value.game_units", language),
        "settings.value.rhai" => "Rhai".to_string(),
        "settings.value.cpp_wasm" => "C++ (WASM)".to_string(),
        "settings.value.visual_nodes" => t("settings.value.visual_nodes", language),
        "settings.value.openrouter" => "OpenRouter".to_string(),
        "settings.value.openai" => "OpenAI".to_string(),
        "settings.value.desktop" => t("settings.value.desktop", language),
        "settings.value.mobile" => t("settings.value.mobile", language),
        "settings.value.web" => t("settings.value.web", language),
        "settings.value.cloud" => t("settings.value.cloud", language),
        "settings.value.console" => t("settings.value.console", language),
        _ => t(key, language),
    }
}

fn egui_input(ctx: &egui::Context, rect: egui::Rect) -> UiInputState {
    ctx.input(|input| {
        let pointer_position = input
            .pointer
            .interact_pos()
            .filter(|position| rect.contains(*position))
            .map(|position| [position.x - rect.min.x, position.y - rect.min.y]);
        let pointer_delta = input.pointer.delta();
        let mut pressed_keys = Vec::new();
        let mut text_input = String::new();
        for event in &input.events {
            match event {
                egui::Event::Text(text) | egui::Event::Paste(text) => text_input.push_str(text),
                egui::Event::Key {
                    key, pressed: true, ..
                } => pressed_keys.push(format!("{key:?}")),
                _ => {}
            }
        }
        UiInputState {
            pointer_position,
            pointer_delta: [pointer_delta.x, pointer_delta.y],
            time_seconds: input.time,
            scroll_delta: [-input.smooth_scroll_delta.x, -input.smooth_scroll_delta.y],
            pointer_down: input.pointer.primary_down(),
            pointer_buttons_down: pointer_buttons_down(input),
            pointer_pressed_buttons: pointer_pressed_buttons(input),
            pointer_released_buttons: pointer_released_buttons(input),
            pressed_keys,
            text_input,
        }
    })
}

fn pointer_buttons_down(input: &egui::InputState) -> Vec<UiPointerButton> {
    let mut buttons = Vec::new();
    if input.pointer.primary_down() {
        buttons.push(UiPointerButton::Primary);
    }
    if input.pointer.secondary_down() {
        buttons.push(UiPointerButton::Secondary);
    }
    if input.pointer.middle_down() {
        buttons.push(UiPointerButton::Middle);
    }
    buttons
}

fn pointer_pressed_buttons(input: &egui::InputState) -> Vec<UiPointerButton> {
    pointer_buttons_for(
        input,
        egui::PointerButton::Primary,
        UiPointerButton::Primary,
        true,
    )
    .into_iter()
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Secondary,
        UiPointerButton::Secondary,
        true,
    ))
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Middle,
        UiPointerButton::Middle,
        true,
    ))
    .collect()
}

fn pointer_released_buttons(input: &egui::InputState) -> Vec<UiPointerButton> {
    pointer_buttons_for(
        input,
        egui::PointerButton::Primary,
        UiPointerButton::Primary,
        false,
    )
    .into_iter()
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Secondary,
        UiPointerButton::Secondary,
        false,
    ))
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Middle,
        UiPointerButton::Middle,
        false,
    ))
    .collect()
}

fn pointer_buttons_for(
    input: &egui::InputState,
    source: egui::PointerButton,
    target: UiPointerButton,
    pressed: bool,
) -> Option<UiPointerButton> {
    let active = if pressed {
        input.pointer.button_pressed(source)
    } else {
        input.pointer.button_released(source)
    };
    active.then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_actions_update_the_existing_draft_fields() {
        let mut settings = EngineSettings::default();

        apply_toggle(&mut settings, "settings.fps-unlimited", true);
        apply_range(&mut settings, "settings.fps-limit", 144.0);
        assert_eq!(settings.fps_limit, 0);

        apply_toggle(&mut settings, "settings.fps-unlimited", false);
        apply_range(&mut settings, "settings.fps-limit", 144.0);
        assert_eq!(settings.fps_limit, 144);

        apply_toggle(&mut settings, "settings.ai.openai.enabled", true);
        apply_text(
            &mut settings,
            "settings.ai.openai.model",
            "gpt-image-2".to_string(),
        );
        let openai = settings
            .ai_providers
            .iter()
            .find(|config| config.provider == AiProvider::OpenAI)
            .expect("OpenAI configuration exists");
        assert!(openai.enabled);
        assert_eq!(openai.model, "gpt-image-2");
    }

    #[test]
    fn provider_key_visibility_is_transient() {
        let mut visible = Vec::new();
        toggle_api_key_visibility(&mut visible, AiProvider::OpenRouter);
        assert_eq!(visible, vec![AiProvider::OpenRouter]);
        toggle_api_key_visibility(&mut visible, AiProvider::OpenRouter);
        assert!(visible.is_empty());
    }
}
