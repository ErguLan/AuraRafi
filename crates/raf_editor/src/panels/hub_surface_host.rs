//! Transitional presentation host for the retained project Hub.
//!
//! The Hub is rendered by RafUI and ApiGraphicBasic. Egui currently provides
//! only the window loop and the final texture placement while the editor
//! migrates screen by screen to the native Winit host.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::{egui, egui_wgpu, wgpu};
use raf_core::config::{Language, Theme};
use raf_core::i18n::t;
use raf_core::project::{ProjectType, RecentProjectEntry};
use raf_render::api_graphic_basic::device::{GpuTextureView, SceneFrameOutput};
use raf_render::api_graphic_basic::ui_surface::{
    CpuUiSurfaceHost, DirectUiSurfaceHost, UiAction, UiDispatchedAction, UiInputState,
    UiPointerButton, UiSurfaceImageStore,
};
use raf_ui::UiWindowCommand;

use crate::panels::gpu_canvas::GpuCanvas;
use crate::studio_surface::{
    build_hub_surface_with_model, HubSurfaceFilter, HubSurfaceModel, HubSurfaceProject,
};

const HUB_CLEAR_DARK: [u8; 4] = [8, 11, 15, 255];
const HUB_CLEAR_LIGHT: [u8; 4] = [250, 250, 250, 255];
const SEARCH_MAX_LENGTH: usize = 512;
const CONTEXT_MENU_SIZE: [f32; 2] = [224.0, 178.0];

/// Renderer-neutral action produced by the retained Hub surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubSurfaceIntent {
    Open(PathBuf),
    Duplicate(PathBuf),
    Forget(PathBuf),
    NewProject(ProjectType),
    OpenSettings,
    SetSearch(String),
    SetFilter(HubSurfaceFilter),
    SetTheme(Theme),
    Window(UiWindowCommand),
}

struct GpuHubSurface {
    host: DirectUiSurfaceHost,
    texture: wgpu::Texture,
    view: Arc<wgpu::TextureView>,
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

impl GpuHubSurface {
    fn new(
        surface: raf_render::api_graphic_basic::ui_surface::UiSurface,
        render_state: &egui_wgpu::RenderState,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) -> Self {
        let format = render_state.target_format;
        let (texture, view) = create_target_texture(render_state.device.as_ref(), format, size);
        let mut host =
            DirectUiSurfaceHost::new(surface, render_state.device.as_ref(), format, clear_color);
        load_hub_images(host.images_mut());
        Self {
            host,
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

/// Owns the RafUI Hub session and only the small compatibility adapter that is
/// still needed by the current eframe application shell.
pub struct HubSurfaceHost {
    canvas: GpuCanvas,
    gpu: Option<GpuHubSurface>,
    cpu: Option<CpuUiSurfaceHost>,
    model: Option<HubSurfaceModel>,
    language: Option<Language>,
    palette: Option<raf_render::api_graphic_basic::ui_surface::StudioUiPalette>,
    context_project_path: Option<PathBuf>,
    context_menu_position: Option<[f32; 2]>,
    hovered_project_path: Option<PathBuf>,
}

impl Default for HubSurfaceHost {
    fn default() -> Self {
        Self {
            canvas: GpuCanvas::new("raf_ui_hub_surface").with_retained_ui_sampling(),
            gpu: None,
            cpu: None,
            model: None,
            language: None,
            palette: None,
            context_project_path: None,
            context_menu_position: None,
            hovered_project_path: None,
        }
    }
}

impl HubSurfaceHost {
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
        language: Language,
        theme: Theme,
        filter: HubSurfaceFilter,
        all_projects: &[RecentProjectEntry],
        visible_projects: &[RecentProjectEntry],
        search_query: &str,
    ) -> Vec<HubSurfaceIntent> {
        let rect = ui.available_rect_before_wrap();
        let _response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let logical_size = [
            rect.width().round().max(1.0) as u32,
            rect.height().round().max(1.0) as u32,
        ];
        let pixels_per_point = ui.ctx().pixels_per_point().clamp(0.5, 4.0);
        let raster_scale = pixels_per_point.max(1.0);
        let target_size = physical_size(logical_size, pixels_per_point);
        let model = hub_model(
            filter,
            theme,
            all_projects,
            visible_projects,
            self.context_project_path.as_deref(),
            self.context_menu_position,
            self.hovered_project_path.as_deref(),
        );
        let clear_color = clear_color(palette);
        self.sync_surface(
            render_state,
            palette,
            language,
            model,
            target_size,
            clear_color,
            search_query,
        );

        let input = egui_input(ui.ctx(), rect);
        let model = self.model.clone();
        let (actions, hovered_id) =
            if let (Some(render_state), Some(gpu)) = (render_state, self.gpu.as_mut()) {
                gpu.resize(render_state.device.as_ref(), target_size);
                let actions = gpu.host.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_hub_text(key, language, model.as_ref()),
                    &input,
                );
                gpu.host.render_at_scale(
                    render_state.device.as_ref(),
                    render_state.queue.as_ref(),
                    gpu.view.as_ref(),
                    target_size,
                    logical_size,
                    raster_scale,
                    |key| resolve_hub_text(key, language, model.as_ref()),
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
                    gpu.host.session().interaction.focus.hovered.clone(),
                )
            } else {
                let cpu = self.cpu.as_mut().expect("CPU Hub host must be prepared");
                let actions = cpu.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_hub_text(key, language, model.as_ref()),
                    &input,
                );
                let frame = cpu.render_at_scale(target_size, logical_size, raster_scale, |key| {
                    resolve_hub_text(key, language, model.as_ref())
                });
                self.canvas.present(
                    ui.ctx(),
                    None,
                    SceneFrameOutput::CpuPixels(frame.pixels.to_vec()),
                    frame.size[0],
                    frame.size[1],
                );
                (actions, cpu.session().interaction.focus.hovered.clone())
            };

        self.canvas.paint(&ui.painter_at(rect), rect);
        let needs_follow_up_frame = !actions.is_empty();
        let hovered_project_path = hovered_project_path(hovered_id.as_deref(), model.as_ref());
        let hover_changed = self.hovered_project_path != hovered_project_path;
        self.hovered_project_path = hovered_project_path;
        let context_before = (
            self.context_project_path.clone(),
            self.context_menu_position,
        );
        let intents = self.resolve_actions(actions, input.pointer_position, logical_size, &input);
        let context_changed = context_before
            != (
                self.context_project_path.clone(),
                self.context_menu_position,
            );
        if needs_follow_up_frame || hover_changed || context_changed {
            ui.ctx().request_repaint();
        }
        intents
    }

    fn resolve_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        pointer_position: Option<[f32; 2]>,
        surface_size: [u32; 2],
        input: &UiInputState,
    ) -> Vec<HubSurfaceIntent> {
        let mut intents = Vec::new();
        let mut opened_context_menu = false;
        for action in actions {
            if let Some(path) = context_menu_path(&action) {
                opened_context_menu = true;
                self.context_project_path = Some(path);
                self.context_menu_position = pointer_position.map(|position| {
                    [
                        position[0].clamp(
                            0.0,
                            surface_size[0].saturating_sub(CONTEXT_MENU_SIZE[0] as u32) as f32,
                        ),
                        position[1].clamp(
                            0.0,
                            surface_size[1].saturating_sub(CONTEXT_MENU_SIZE[1] as u32) as f32,
                        ),
                    ]
                });
                continue;
            }
            if matches!(
                action.action,
                UiAction::Command { ref name } if name == "hub.context-close"
            ) {
                self.context_project_path = None;
                self.context_menu_position = None;
                continue;
            }
            if let Some(intent) = intent_from_action(action) {
                if matches!(
                    intent,
                    HubSurfaceIntent::Open(_)
                        | HubSurfaceIntent::Duplicate(_)
                        | HubSurfaceIntent::Forget(_)
                ) {
                    self.context_project_path = None;
                    self.context_menu_position = None;
                }
                intents.push(intent);
            }
        }
        if !opened_context_menu
            && should_dismiss_context_menu(self.context_menu_position, pointer_position, input)
        {
            self.context_project_path = None;
            self.context_menu_position = None;
        }
        intents
    }

    fn sync_surface(
        &mut self,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
        language: Language,
        model: HubSurfaceModel,
        size: [u32; 2],
        clear_color: [u8; 4],
        search_query: &str,
    ) {
        let model_changed = self.model.as_ref() != Some(&model);
        let text_model_changed = hub_text_model_changed(self.model.as_ref(), &model);
        let language_changed = self.language != Some(language);
        let palette_changed = self.palette != Some(palette);
        self.model = Some(model);
        self.language = Some(language);
        self.palette = Some(palette);
        let changed = model_changed || language_changed || palette_changed;
        let surface =
            build_hub_surface_with_model(palette, self.model.as_ref().expect("hub model"));

        if let Some(render_state) = render_state {
            let rebuild_gpu = self
                .gpu
                .as_ref()
                .map(|gpu| gpu.format != render_state.target_format || palette_changed)
                .unwrap_or(true);
            if rebuild_gpu {
                self.gpu = Some(GpuHubSurface::new(
                    surface.clone(),
                    render_state,
                    size,
                    clear_color,
                ));
            } else if changed {
                let gpu = self.gpu.as_mut().expect("GPU Hub host must exist");
                gpu.host.set_surface(surface.clone());
                if text_model_changed || language_changed || palette_changed {
                    gpu.host.session_mut().text_atlas.clear();
                }
            }
        }

        if render_state.is_none() {
            if self.cpu.is_none() || palette_changed {
                let mut host = CpuUiSurfaceHost::new(surface, clear_color);
                load_hub_images(host.images_mut());
                self.cpu = Some(host);
            } else if changed {
                let cpu = self.cpu.as_mut().expect("CPU Hub host must exist");
                cpu.set_surface(surface);
                if text_model_changed || language_changed || palette_changed {
                    cpu.session_mut().text_atlas.clear();
                }
            }
        }

        self.seed_search(search_query);
    }

    fn seed_search(&mut self, search_query: &str) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.host.session_mut().interaction.controls.set_text(
                "hub.search",
                search_query,
                SEARCH_MAX_LENGTH,
            );
        }
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.session_mut().interaction.controls.set_text(
                "hub.search",
                search_query,
                SEARCH_MAX_LENGTH,
            );
        }
    }
}

fn hub_text_model_changed(previous: Option<&HubSurfaceModel>, next: &HubSurfaceModel) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let mut previous = previous.clone();
    let mut next = next.clone();
    previous.hovered_project_path = None;
    next.hovered_project_path = None;
    previous != next
}

fn create_target_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> (wgpu::Texture, Arc<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ApiGraphicBasic.RafUiHubTarget"),
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

fn hub_model(
    filter: HubSurfaceFilter,
    theme: Theme,
    all_projects: &[RecentProjectEntry],
    visible_projects: &[RecentProjectEntry],
    context_project_path: Option<&std::path::Path>,
    context_menu_position: Option<[f32; 2]>,
    hovered_project_path: Option<&std::path::Path>,
) -> HubSurfaceModel {
    let game_projects = all_projects
        .iter()
        .filter(|project| project.project_type == ProjectType::Game)
        .count();
    let to_surface_project = |project: &RecentProjectEntry| HubSurfaceProject {
        name: project.name.clone(),
        path: project.path.clone(),
        project_type: project.project_type,
        last_opened_label: project.last_opened.format("%d/%m/%Y").to_string(),
    };
    HubSurfaceModel {
        filter,
        theme,
        total_projects: all_projects.len(),
        game_projects,
        electronics_projects: all_projects.len().saturating_sub(game_projects),
        projects: visible_projects.iter().map(to_surface_project).collect(),
        featured_project: all_projects.first().map(to_surface_project),
        recent_activity: all_projects
            .iter()
            .take(4)
            .map(to_surface_project)
            .collect(),
        context_project: context_project_path.and_then(|path| {
            all_projects
                .iter()
                .find(|project| project.path == path)
                .map(to_surface_project)
        }),
        context_menu_position,
        hovered_project_path: hovered_project_path.map(PathBuf::from),
    }
}

fn hovered_project_path(
    hovered_id: Option<&str>,
    model: Option<&HubSurfaceModel>,
) -> Option<PathBuf> {
    let hovered_id = hovered_id?;
    let model = model?;
    model
        .projects
        .iter()
        .enumerate()
        .find_map(|(index, project)| {
            let card_id = format!("hub.project.{index}");
            (hovered_id == card_id || hovered_id.starts_with(&format!("{card_id}.")))
                .then(|| project.path.clone())
        })
}

fn should_dismiss_context_menu(
    context_menu_position: Option<[f32; 2]>,
    pointer_position: Option<[f32; 2]>,
    input: &UiInputState,
) -> bool {
    if input.key_pressed("escape") {
        return true;
    }
    if !input.button_pressed(UiPointerButton::Primary)
        && !input.button_pressed(UiPointerButton::Secondary)
    {
        return false;
    }
    let (Some(menu_position), Some(pointer_position)) = (context_menu_position, pointer_position)
    else {
        return false;
    };
    !(pointer_position[0] >= menu_position[0]
        && pointer_position[0] <= menu_position[0] + CONTEXT_MENU_SIZE[0]
        && pointer_position[1] >= menu_position[1]
        && pointer_position[1] <= menu_position[1] + CONTEXT_MENU_SIZE[1])
}

fn clear_color(palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette) -> [u8; 4] {
    match palette {
        raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark => {
            HUB_CLEAR_DARK
        }
        raf_render::api_graphic_basic::ui_surface::StudioUiPalette::PaperLight => HUB_CLEAR_LIGHT,
    }
}

fn resolve_hub_text(key: &str, language: Language, model: Option<&HubSurfaceModel>) -> String {
    let Some(model) = model else {
        return t(key, language);
    };
    match key {
        "hub.brand" => "AuraRafi".to_string(),
        "hub.version" => format!("v{}", env!("CARGO_PKG_VERSION")),
        "hub.stats" => format!(
            "{} {} | {} {} | {} {}",
            model.total_projects,
            t("app.hub_total_projects", language),
            model.game_projects,
            t("app.hub_game_kind", language),
            model.electronics_projects,
            t("app.hub_electronics_kind", language),
        ),
        "hub.results" => format!(
            "{} {}",
            model.projects.len(),
            t("app.hub_results_label", language)
        ),
        "hub.welcome" => t("app.hub_welcome", language),
        "hub.welcome-detail" => t("app.hub_welcome_detail", language),
        "hub.context.title" => model
            .context_project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_default(),
        _ => resolve_project_text(key, language, model)
            .or_else(|| resolve_featured_text(key, language, model))
            .or_else(|| resolve_activity_text(key, language, model))
            .unwrap_or_else(|| t(key, language)),
    }
}

fn resolve_project_text(key: &str, language: Language, model: &HubSurfaceModel) -> Option<String> {
    let suffix = key.strip_prefix("hub.project.")?;
    let (index, field) = suffix.split_once('.')?;
    let project = model.projects.get(index.parse::<usize>().ok()?)?;
    match field {
        "name" => Some(project.name.clone()),
        "kind" => {
            let kind_key = match project.project_type {
                ProjectType::Game => "app.hub_type_game_label",
                ProjectType::Electronics => "app.hub_type_electronics_label",
            };
            Some(t(kind_key, language))
        }
        "last-opened" => Some(format!(
            "{} {}",
            t("app.hub_last_opened", language),
            project.last_opened_label
        )),
        _ => None,
    }
}

fn resolve_featured_text(key: &str, language: Language, model: &HubSurfaceModel) -> Option<String> {
    let project = model.featured_project.as_ref()?;
    match key {
        "hub.featured.kind" => Some(t(project_kind_key(project), language)),
        "hub.featured.name" => Some(project.name.clone()),
        "hub.featured.path" => Some(project.path.to_string_lossy().to_string()),
        "hub.featured.meta" => Some(format!(
            "{} | {}",
            t("app.hub_last_opened", language),
            project.last_opened_label
        )),
        _ => None,
    }
}

fn resolve_activity_text(key: &str, language: Language, model: &HubSurfaceModel) -> Option<String> {
    let suffix = key.strip_prefix("hub.activity.")?;
    let (index, field) = suffix.split_once('.')?;
    let project = model.recent_activity.get(index.parse::<usize>().ok()?)?;
    match field {
        "name" => Some(project.name.clone()),
        "meta" => Some(format!(
            "{} | {}",
            t(project_kind_key(project), language),
            project.last_opened_label
        )),
        _ => None,
    }
}

fn project_kind_key(project: &HubSurfaceProject) -> &'static str {
    match project.project_type {
        ProjectType::Game => "app.hub_type_game_label",
        ProjectType::Electronics => "app.hub_type_electronics_label",
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
            // Egui exposes wheel movement in viewport coordinates; RafUI's
            // scroll offset grows in content coordinates. Invert here at the
            // compatibility boundary so wheel-up reveals earlier content.
            scroll_delta: raf_scroll_delta(input.smooth_scroll_delta),
            pointer_down: input.pointer.primary_down(),
            pointer_buttons_down: pointer_buttons_down(input),
            pointer_pressed_buttons: pointer_pressed_buttons(input),
            pointer_released_buttons: pointer_released_buttons(input),
            pointer_pressed_outside: false,
            pressed_keys,
            text_input,
            ime_preedit: String::new(),
            modifiers: raf_ui::UiModifiers {
                shift: input.modifiers.shift,
                control: input.modifiers.ctrl,
                alt: input.modifiers.alt,
                command: input.modifiers.mac_cmd,
            },
        }
    })
}

fn raf_scroll_delta(delta: egui::Vec2) -> [f32; 2] {
    [-delta.x, -delta.y]
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

fn intent_from_action(dispatched: UiDispatchedAction) -> Option<HubSurfaceIntent> {
    match dispatched.action {
        UiAction::SetText { key, value } if key == "hub.search" => {
            Some(HubSurfaceIntent::SetSearch(value))
        }
        UiAction::Command { name } => match name.as_str() {
            "hub.new-game" => Some(HubSurfaceIntent::NewProject(ProjectType::Game)),
            "hub.new-electronics" => Some(HubSurfaceIntent::NewProject(ProjectType::Electronics)),
            "hub.settings" => Some(HubSurfaceIntent::OpenSettings),
            "hub.filter-all" => Some(HubSurfaceIntent::SetFilter(HubSurfaceFilter::All)),
            "hub.filter-games" => Some(HubSurfaceIntent::SetFilter(HubSurfaceFilter::Game)),
            "hub.filter-electronics" => {
                Some(HubSurfaceIntent::SetFilter(HubSurfaceFilter::Electronics))
            }
            "hub.theme-dark" => Some(HubSurfaceIntent::SetTheme(Theme::Dark)),
            "hub.theme-light" => Some(HubSurfaceIntent::SetTheme(Theme::Light)),
            "hub.theme-system" => Some(HubSurfaceIntent::SetTheme(Theme::System)),
            "window.minimize" => Some(HubSurfaceIntent::Window(UiWindowCommand::Minimize)),
            "window.maximize" => Some(HubSurfaceIntent::Window(UiWindowCommand::ToggleMaximize)),
            "window.close" => Some(HubSurfaceIntent::Window(UiWindowCommand::Close)),
            "window.drag" => Some(HubSurfaceIntent::Window(UiWindowCommand::BeginDrag)),
            _ => None,
        },
        UiAction::Custom { channel, payload } if channel == "hub.project" => {
            let action = payload.get("action")?.as_str()?;
            let path = PathBuf::from(payload.get("path")?.as_str()?);
            match action {
                "open" => Some(HubSurfaceIntent::Open(path)),
                "duplicate" => Some(HubSurfaceIntent::Duplicate(path)),
                "forget" => Some(HubSurfaceIntent::Forget(path)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn context_menu_path(dispatched: &UiDispatchedAction) -> Option<PathBuf> {
    let UiAction::Custom { channel, payload } = &dispatched.action else {
        return None;
    };
    if channel != "hub.project" || payload.get("action")?.as_str()? != "menu" {
        return None;
    }
    payload.get("path")?.as_str().map(PathBuf::from)
}

fn load_hub_images(store: &mut UiSurfaceImageStore) {
    insert_embedded_png(
        store,
        "editor.hub.brand",
        include_bytes!("../../../../editor/icon.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.preview-game",
        include_bytes!("../../../../editor/assets/ui_icons/hub_game_preview.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.preview-electronics",
        include_bytes!("../../../../editor/assets/ui_icons/hub_electronics_preview.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.kind-game",
        include_bytes!("../../../../editor/assets/ui_icons/project_game.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.kind-electronics",
        include_bytes!("../../../../editor/assets/ui_icons/project_electronics.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.more",
        include_bytes!("../../../../editor/assets/ui_icons/3-dots vertical.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.settings",
        include_bytes!("../../../../editor/assets/ui_icons/settings_HUB.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.icon-sun",
        include_bytes!("../../../../editor/assets/ui_icons/sun.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.icon-moon",
        include_bytes!("../../../../editor/assets/ui_icons/moon.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.icon-home",
        include_bytes!("../../../../editor/assets/ui_icons/home.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.icon-arrow",
        include_bytes!("../../../../editor/assets/ui_icons/arrow_right.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.icon-pulse",
        include_bytes!("../../../../editor/assets/ui_icons/pulse.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.minimize",
        include_bytes!("../../../../editor/assets/ui_icons/top/minimize.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.maximize",
        include_bytes!("../../../../editor/assets/ui_icons/top/maximize.png"),
    );
    insert_embedded_png(
        store,
        "editor.hub.close",
        include_bytes!("../../../../editor/assets/ui_icons/top/close.png"),
    );
}

fn insert_embedded_png(store: &mut UiSurfaceImageStore, key: &str, bytes: &[u8]) {
    let Ok(image) = image::load_from_memory(bytes) else {
        return;
    };
    let image = image.to_rgba8();
    let _ = store.insert_rgba(key, [image.width(), image.height()], image.into_raw());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_project_actions_preserve_the_project_path() {
        let action = UiDispatchedAction {
            target_id: "project".to_string(),
            event: raf_render::api_graphic_basic::ui_surface::UiEventKind::Click,
            action: UiAction::Custom {
                channel: "hub.project".to_string(),
                payload: serde_json::json!({ "action": "open", "path": "C:/Projects/Hub" }),
            },
        };

        assert_eq!(
            intent_from_action(action),
            Some(HubSurfaceIntent::Open(PathBuf::from("C:/Projects/Hub")))
        );
    }

    #[test]
    fn project_context_requests_keep_the_target_path() {
        let action = UiDispatchedAction {
            target_id: "project".to_string(),
            event: raf_render::api_graphic_basic::ui_surface::UiEventKind::ContextMenu,
            action: UiAction::Custom {
                channel: "hub.project".to_string(),
                payload: serde_json::json!({ "action": "menu", "path": "C:/Projects/Hub" }),
            },
        };

        assert_eq!(
            context_menu_path(&action),
            Some(PathBuf::from("C:/Projects/Hub"))
        );
    }

    #[test]
    fn hub_model_keeps_counts_separate_from_visible_filter() {
        let now = chrono::Utc::now();
        let all = vec![
            RecentProjectEntry {
                name: "Game".to_string(),
                path: PathBuf::from("game"),
                project_type: ProjectType::Game,
                created_at: now,
                modified_at: now,
                last_opened: now,
                n_elements: 0,
            },
            RecentProjectEntry {
                name: "Board".to_string(),
                path: PathBuf::from("board"),
                project_type: ProjectType::Electronics,
                created_at: now,
                modified_at: now,
                last_opened: now,
                n_elements: 0,
            },
        ];
        let model = hub_model(
            HubSurfaceFilter::Game,
            Theme::Dark,
            &all,
            &all[..1],
            None,
            None,
            None,
        );

        assert_eq!(model.total_projects, 2);
        assert_eq!(model.game_projects, 1);
        assert_eq!(model.electronics_projects, 1);
        assert_eq!(model.projects.len(), 1);
        assert_eq!(
            model.featured_project.as_ref().map(|project| &project.name),
            Some(&"Game".to_string())
        );
    }

    #[test]
    fn hub_maps_logical_size_to_native_pixels_without_rounding_down() {
        assert_eq!(physical_size([800, 600], 1.25), [1000, 750]);
        assert_eq!(physical_size([1, 1], 0.5), [1, 1]);
    }

    #[test]
    fn hub_inverts_egui_wheel_delta_at_the_compatibility_boundary() {
        assert_eq!(raf_scroll_delta(egui::vec2(0.0, 48.0)), [0.0, -48.0]);
        assert_eq!(raf_scroll_delta(egui::vec2(-12.0, -24.0)), [12.0, 24.0]);
    }

    #[test]
    fn context_menu_dismisses_outside_clicks_and_escape() {
        let position = Some([100.0, 120.0]);
        let outside_click = UiInputState {
            pointer_pressed_buttons: vec![UiPointerButton::Primary],
            ..UiInputState::default()
        };
        assert!(should_dismiss_context_menu(
            position,
            Some([80.0, 80.0]),
            &outside_click,
        ));

        assert!(!should_dismiss_context_menu(
            position,
            Some([160.0, 180.0]),
            &outside_click,
        ));

        let escape = UiInputState {
            pressed_keys: vec!["Escape".to_string()],
            ..UiInputState::default()
        };
        assert!(should_dismiss_context_menu(position, None, &escape));
    }

    #[test]
    fn hovered_project_path_accepts_card_and_menu_targets() {
        let project = HubSurfaceProject {
            name: "Demo".to_string(),
            path: PathBuf::from("C:/Projects/Demo"),
            project_type: ProjectType::Game,
            last_opened_label: "14/07/2026".to_string(),
        };
        let model = HubSurfaceModel {
            projects: vec![project.clone()],
            ..HubSurfaceModel::default()
        };

        assert_eq!(
            hovered_project_path(Some("hub.project.0"), Some(&model)),
            Some(project.path.clone())
        );
        assert_eq!(
            hovered_project_path(Some("hub.project.0.menu"), Some(&model)),
            Some(project.path)
        );
        assert_eq!(hovered_project_path(Some("hub.search"), Some(&model)), None);
    }
}
