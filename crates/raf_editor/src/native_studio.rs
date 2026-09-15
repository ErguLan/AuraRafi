//! Native retained Project Hub surface.
//!
//! The Hub is part of the same migration boundary as the editor: RafUI owns
//! the document and interaction state, while Winit owns the window and the
//! application boundary executes project creation/opening.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType, RecentProjectEntry, RecentProjects};
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, UiAction, UiDispatchedAction,
    UiSurfaceImageStore,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::UiRect;
use winit::event_loop::EventLoopProxy;

use crate::editor_layout::{EditorFrameLayout, EditorRect};
use crate::folder_picker;
use crate::settings_surface::SettingsSection;
use crate::studio_surface::{
    build_hub_surface_with_model, hub_visible_featured, hub_visible_projects, hub_visible_recent,
    HubProjectCreateError, HubSurfaceFilter, HubSurfaceModel, HubSurfaceProject,
};
use raf_ui::UiWindowCommand;

const HUB_CLEAR: [u8; 4] = [8, 11, 15, 255];
const MAX_TEXT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeStudioIntent {
    Window(UiWindowCommand),
    OpenSettings {
        section: SettingsSection,
    },
    Open(PathBuf),
    Duplicate(PathBuf),
    Forget(PathBuf),
    Create {
        name: String,
        parent: PathBuf,
        project_type: ProjectType,
    },
}

pub struct NativeStudioSurface {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    model: HubSurfaceModel,
    last_model: Option<HubSurfaceModel>,
    pointer_position: Option<[f32; 2]>,
    palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
    language: Language,
    discovery: Option<Receiver<HubSurfaceModel>>,
}

impl NativeStudioSurface {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
        wakeup: Option<EventLoopProxy<()>>,
    ) -> Self {
        let mut model = HubSurfaceModel::default();
        model.create_path = default_project_location().to_string_lossy().to_string();
        model.create_active = true;
        let (sender, discovery) = mpsc::channel();
        std::thread::Builder::new()
            .name("raf-hub-discovery".to_string())
            .spawn(move || {
                let _ = sender.send(discover_hub_model());
                if let Some(wakeup) = wakeup {
                    let _ = wakeup.send_event(());
                }
            })
            .ok();
        let surface = build_hub_surface_with_model(palette, &model);
        let mut host = graphics.create_ui_host(surface, HUB_CLEAR);
        load_hub_images(host.images_mut());
        Self {
            region: InputRegionId::from_static("native.editor.studio"),
            rect,
            host,
            model,
            last_model: None,
            pointer_position: None,
            palette,
            language: Language::English,
            discovery: Some(discovery),
        }
    }

    pub fn owner(&self) -> InputOwner {
        InputOwner::RetainedUi(self.region)
    }

    pub fn sync(
        &mut self,
        layout: EditorFrameLayout,
        palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
    ) {
        self.poll_discovery();
        self.rect = layout.window;
        let palette_changed = self.palette != palette;
        self.palette = palette;
        if self.last_model.as_ref() == Some(&self.model) && !palette_changed {
            return;
        }
        let surface = build_hub_surface_with_model(palette, &self.model);
        self.host.set_surface(surface);
        self.host.session_mut().interaction.controls.set_text(
            "hub.create.name",
            &self.model.create_name,
            MAX_TEXT_LENGTH,
        );
        self.host.session_mut().interaction.controls.set_text(
            "hub.create.path",
            &self.model.create_path,
            MAX_TEXT_LENGTH,
        );
        self.host.session_mut().interaction.controls.set_text(
            "hub.search",
            &self.model.search_query,
            MAX_TEXT_LENGTH,
        );
        self.last_model = Some(self.model.clone());
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn set_language(&mut self, language: Language) {
        if self.language == language {
            return;
        }
        self.language = language;
        self.last_model = None;
    }

    pub fn set_create_error(&mut self, detail: impl Into<String>) {
        let error = Some(HubProjectCreateError::CreationFailed(detail.into()));
        if self.model.create_error == error {
            return;
        }
        self.model.create_error = error;
        self.last_model = None;
    }

    pub fn reset_for_hub(&mut self, open_create: bool) {
        // Returning to the Hub must be a UI-state transition, not a filesystem
        // discovery pass. The initial model was already discovered at startup;
        // In-memory project mutations update the cached model; returning here
        // never performs an implicit filesystem scan. A missing or slow
        // project path therefore cannot stall the editor event loop.
        self.model.create_error = None;
        self.model.filter = HubSurfaceFilter::All;
        self.model.context_project = None;
        self.model.context_menu_position = None;
        self.model.hovered_project_path = None;
        self.model.create_type_menu_open = false;
        if self.model.create_path.trim().is_empty() {
            self.model.create_path = default_project_location().to_string_lossy().to_string();
        }
        self.model.create_active = open_create;
        self.last_model = None;
        self.pointer_position = None;
    }

    pub fn model_snapshot(&self) -> HubSurfaceModel {
        self.model.clone()
    }

    pub fn needs_surface_sync(&self) -> bool {
        self.last_model.as_ref() != Some(&self.model)
    }

    pub fn has_active_motion(&self) -> bool {
        self.host.has_active_motion()
    }

    fn poll_discovery(&mut self) {
        let Some(receiver) = self.discovery.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(discovered) => {
                self.model.total_projects = discovered.total_projects;
                self.model.game_projects = discovered.game_projects;
                self.model.electronics_projects = discovered.electronics_projects;
                self.model.featured_project = discovered.featured_project;
                self.model.recent_activity = discovered.recent_activity;
                self.model.projects = discovered.projects;
                self.last_model = None;
                self.discovery = None;
            }
            Err(TryRecvError::Disconnected) => self.discovery = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat()
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<NativeStudioIntent> {
        self.pointer_position = input.snapshot().pointer_position;
        let model = &self.model;
        let language = self.language;
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| resolve_hub_text(key, &model, language),
            input,
            router,
            self.owner(),
            UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        let had_actions = !actions.is_empty();
        let mut intents = Vec::new();
        for action in actions {
            self.handle_action(action, &mut intents);
        }
        if input.snapshot().key_pressed(raf_core::InputKey::Escape) {
            self.model.create_type_menu_open = false;
            self.model.context_project = None;
            self.model.context_menu_position = None;
            self.last_model = None;
        }
        if had_actions {
            self.last_model = None;
        }
        intents
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

    fn handle_action(
        &mut self,
        dispatched: UiDispatchedAction,
        intents: &mut Vec<NativeStudioIntent>,
    ) {
        match dispatched.action {
            UiAction::SetText { key, value } => match key.as_str() {
                "hub.create.name" => {
                    self.model.create_name = value;
                    self.model.create_error = None;
                }
                "hub.create.path" => {
                    self.model.create_path = value;
                    self.model.create_error = None;
                }
                "hub.search" => self.model.search_query = value,
                _ => {}
            },
            UiAction::Command { name } => match name.as_str() {
                "window.drag" => {
                    intents.push(NativeStudioIntent::Window(UiWindowCommand::BeginDrag))
                }
                "window.minimize" => {
                    intents.push(NativeStudioIntent::Window(UiWindowCommand::Minimize))
                }
                "window.maximize" => {
                    intents.push(NativeStudioIntent::Window(UiWindowCommand::ToggleMaximize))
                }
                "window.close" => intents.push(NativeStudioIntent::Window(UiWindowCommand::Close)),
                "hub.settings" => intents.push(NativeStudioIntent::OpenSettings {
                    section: SettingsSection::Appearance,
                }),
                "hub.new" => {
                    self.model.create_active = true;
                    self.model.create_error = None;
                    self.host
                        .session_mut()
                        .interaction
                        .focus
                        .request_focus("hub.create.name");
                }
                "hub.new-game" => {
                    self.model.create_active = true;
                    self.model.create_project_type = ProjectType::Game;
                    self.model.create_type_menu_open = false;
                    self.model.create_error = None;
                }
                "hub.new-electronics" => {
                    self.model.create_active = true;
                    self.model.create_project_type = ProjectType::Electronics;
                    self.model.create_type_menu_open = false;
                    self.model.create_error = None;
                }
                "hub.projects" | "hub.view-all" => {
                    self.model.filter = HubSurfaceFilter::All;
                    self.model.create_active = false;
                    self.model.context_project = None;
                    self.model.context_menu_position = None;
                }
                "hub.filter-all" => self.model.filter = HubSurfaceFilter::All,
                "hub.filter-games" => self.model.filter = HubSurfaceFilter::Game,
                "hub.filter-electronics" => self.model.filter = HubSurfaceFilter::Electronics,
                "hub.context-open" => {
                    if let Some(project) = self.model.context_project.take() {
                        intents.push(NativeStudioIntent::Open(project.path));
                    }
                    self.model.context_menu_position = None;
                }
                "hub.context-duplicate" => {
                    if let Some(project) = self.model.context_project.take() {
                        intents.push(NativeStudioIntent::Duplicate(project.path));
                    }
                    self.model.context_menu_position = None;
                }
                "hub.context-forget" => {
                    if let Some(project) = self.model.context_project.take() {
                        let path = project.path.clone();
                        self.model.projects.retain(|item| item.path != path);
                        self.model.recent_activity.retain(|item| item.path != path);
                        if self
                            .model
                            .featured_project
                            .as_ref()
                            .is_some_and(|item| item.path == path)
                        {
                            self.model.featured_project = self.model.projects.first().cloned();
                        }
                        self.model.total_projects = self.model.projects.len();
                        self.model.game_projects = self
                            .model
                            .projects
                            .iter()
                            .filter(|item| item.project_type == ProjectType::Game)
                            .count();
                        self.model.electronics_projects = self
                            .model
                            .total_projects
                            .saturating_sub(self.model.game_projects);
                        intents.push(NativeStudioIntent::Forget(path));
                    }
                    self.model.context_menu_position = None;
                }
                "hub.context-close" => {
                    self.model.context_project = None;
                    self.model.context_menu_position = None;
                }
                "hub.create.type-menu" => {
                    self.model.create_type_menu_open = !self.model.create_type_menu_open;
                    self.model.create_active = true;
                }
                "hub.create.game" => {
                    self.model.create_project_type = ProjectType::Game;
                    self.model.create_type_menu_open = false;
                }
                "hub.create.electronics" => {
                    self.model.create_project_type = ProjectType::Electronics;
                    self.model.create_type_menu_open = false;
                }
                "hub.create.choose-path" => {
                    let current = self.model.create_path.clone();
                    if let Some(path) = folder_picker::pick_folder(&current) {
                        self.model.create_path = path.to_string_lossy().to_string();
                        self.model.create_error = None;
                    }
                }
                "hub.create.submit" => {
                    let name = self.model.create_name.trim().to_string();
                    let parent = PathBuf::from(self.model.create_path.trim());
                    if name.is_empty() {
                        self.model.create_error = Some(HubProjectCreateError::NameRequired);
                    } else if !valid_project_name(&name) {
                        self.model.create_error = Some(HubProjectCreateError::NameInvalid);
                    } else if parent.as_os_str().is_empty() {
                        self.model.create_error = Some(HubProjectCreateError::LocationRequired);
                    } else {
                        self.model.create_error = None;
                        intents.push(NativeStudioIntent::Create {
                            name,
                            parent,
                            project_type: self.model.create_project_type,
                        });
                    }
                }
                _ => {}
            },
            UiAction::Custom { channel, payload } if channel == "hub.project" => {
                let Some(path) = payload.get("path").and_then(|value| value.as_str()) else {
                    return;
                };
                let path = PathBuf::from(path);
                match payload.get("action").and_then(|value| value.as_str()) {
                    Some("open") => intents.push(NativeStudioIntent::Open(path)),
                    Some("duplicate") => intents.push(NativeStudioIntent::Duplicate(path)),
                    Some("forget") => intents.push(NativeStudioIntent::Forget(path)),
                    Some("hover") => {
                        self.model.hovered_project_path = Some(path);
                    }
                    Some("leave") => {
                        if self.model.hovered_project_path.as_deref() == Some(path.as_path()) {
                            self.model.hovered_project_path = None;
                        }
                    }
                    Some("menu") => {
                        self.model.context_project = self
                            .model
                            .projects
                            .iter()
                            .find(|project| project.path == path)
                            .cloned();
                        self.model.context_menu_position = self.pointer_position;
                        self.model.hovered_project_path = Some(path);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn default_project_location() -> PathBuf {
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let mut candidates = profile.into_iter().flat_map(|profile| {
        [
            profile
                .join("OneDrive")
                .join("Documentos")
                .join("AuraRafi Projects"),
            profile
                .join("OneDrive")
                .join("Documents")
                .join("AuraRafi Projects"),
            profile.join("Documents").join("AuraRafi Projects"),
        ]
    });
    if let Some(path) = candidates.find(|path| path.is_dir()) {
        return path;
    }

    // The recent-project registry is authoritative when the user moved or
    // localized the Documents folder. It also keeps the Hub useful when the
    // default folder itself was not created yet.
    if let Some(path) = recent_projects()
        .projects
        .iter()
        .map(|entry| entry.path.parent().map(PathBuf::from))
        .flatten()
        .find(|path| path.is_dir())
    {
        return path;
    }

    dirs_next::document_dir()
        .map(|path| path.join("AuraRafi Projects"))
        .filter(|path| path.is_dir())
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.join("AuraRafi Projects"))
        })
        .unwrap_or_else(|| PathBuf::from("AuraRafi Projects"))
}

fn recent_projects() -> RecentProjects {
    dirs_next::config_dir()
        .map(|path| path.join("AuraRafi"))
        .map(|path| RecentProjects::load(&path))
        .unwrap_or_default()
}

pub(crate) fn remember_project(project: &Project) {
    let Some(config_root) = dirs_next::config_dir().map(|path| path.join("AuraRafi")) else {
        return;
    };
    if std::fs::create_dir_all(&config_root).is_err() {
        return;
    }
    let mut recent = RecentProjects::load(&config_root);
    recent.add(project);
    if let Err(error) = recent.save(&config_root) {
        tracing::warn!(%error, "could not persist native Hub recent project");
    }
}

pub(crate) fn forget_project(path: &std::path::Path) {
    let Some(config_root) = dirs_next::config_dir().map(|path| path.join("AuraRafi")) else {
        return;
    };
    let mut recent = RecentProjects::load(&config_root);
    recent.projects.retain(|entry| entry.path != path);
    if let Err(error) = recent.save(&config_root) {
        tracing::warn!(%error, "could not persist native Hub project removal");
    }
}

fn project_from_recent(entry: &RecentProjectEntry) -> HubSurfaceProject {
    HubSurfaceProject {
        name: entry.name.clone(),
        path: entry.path.clone(),
        project_type: entry.project_type,
        last_opened_label: entry.last_opened.format("%Y-%m-%d").to_string(),
    }
}

fn project_from_loaded(project: Project, last_opened_label: Option<String>) -> HubSurfaceProject {
    let fallback_label = project.modified_at.format("%Y-%m-%d").to_string();
    HubSurfaceProject {
        name: project.name,
        path: project.path,
        project_type: project.project_type,
        last_opened_label: last_opened_label.unwrap_or(fallback_label),
    }
}

fn discover_hub_model() -> HubSurfaceModel {
    let projects = discover_hub_projects(&default_project_location());
    let total_projects = projects.len();
    let game_projects = projects
        .iter()
        .filter(|project| project.project_type == ProjectType::Game)
        .count();
    let electronics_projects = total_projects.saturating_sub(game_projects);
    HubSurfaceModel {
        total_projects,
        game_projects,
        electronics_projects,
        featured_project: projects.first().cloned(),
        recent_activity: projects.clone(),
        projects,
        ..HubSurfaceModel::default()
    }
}

fn discover_hub_projects(root: &std::path::Path) -> Vec<HubSurfaceProject> {
    let recent = recent_projects();
    let mut projects = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Preserve the user's actual recent ordering. If an old project metadata
    // file no longer deserializes after a schema evolution, the registry still
    // gives the Hub enough truthful information to show it and let the user
    // decide whether to open or repair it.
    for entry in &recent.projects {
        // The registry is also useful when a project metadata file is from an
        // older schema. Keep directory-backed entries visible and let the
        // open action report a repair/load error instead of silently turning
        // the Hub into "No projects found".
        if !entry.path.is_dir() || !seen.insert(entry.path.clone()) {
            continue;
        }
        let row = if entry.path.join(Project::META_FILE).is_file() {
            Project::load(&entry.path)
                .map(|project| {
                    project_from_loaded(
                        project,
                        Some(entry.last_opened.format("%Y-%m-%d").to_string()),
                    )
                })
                .unwrap_or_else(|_| project_from_recent(entry))
        } else {
            project_from_recent(entry)
        };
        projects.push(row);
    }

    let mut directories = Vec::new();
    if root.join(Project::META_FILE).is_file() {
        directories.push(root.to_path_buf());
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        directories.extend(entries.flatten().filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .and_then(|_| {
                    let path = entry.path();
                    path.join(Project::META_FILE).is_file().then_some(path)
                })
        }));
    }

    for path in directories {
        if !seen.insert(path.clone()) {
            continue;
        }
        if let Ok(project) = Project::load(&path) {
            projects.push(project_from_loaded(project, None));
        }
    }

    projects
}

fn valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_TEXT_LENGTH
        && name != "."
        && name != ".."
        && !name.chars().any(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
}

fn load_hub_images(store: &mut UiSurfaceImageStore) {
    let images = [
        (
            "editor.hub.brand",
            include_bytes!("../../../editor/icon.png").as_slice(),
        ),
        (
            "editor.hub.preview-game",
            include_bytes!("../../../editor/assets/ui_icons/hub_game_preview.png").as_slice(),
        ),
        (
            "editor.hub.preview-electronics",
            include_bytes!("../../../editor/assets/ui_icons/hub_electronics_preview.png")
                .as_slice(),
        ),
        (
            "editor.hub.kind-game",
            include_bytes!("../../../editor/assets/ui_icons/project_game.png").as_slice(),
        ),
        (
            "editor.hub.kind-electronics",
            include_bytes!("../../../editor/assets/ui_icons/project_electronics.png").as_slice(),
        ),
        (
            "editor.hub.more",
            include_bytes!("../../../editor/assets/ui_icons/3-dots vertical.png").as_slice(),
        ),
        (
            "editor.hub.settings",
            include_bytes!("../../../editor/assets/ui_icons/settings_HUB.png").as_slice(),
        ),
        (
            "editor.hub.icon-sun",
            include_bytes!("../../../editor/assets/ui_icons/sun.png").as_slice(),
        ),
        (
            "editor.hub.icon-moon",
            include_bytes!("../../../editor/assets/ui_icons/moon.png").as_slice(),
        ),
        (
            "editor.hub.icon-home",
            include_bytes!("../../../editor/assets/ui_icons/home.png").as_slice(),
        ),
        (
            "editor.hub.icon-arrow",
            include_bytes!("../../../editor/assets/ui_icons/arrow_right.png").as_slice(),
        ),
        (
            "editor.hub.icon-pulse",
            include_bytes!("../../../editor/assets/ui_icons/pulse.png").as_slice(),
        ),
        (
            "editor.hub.plus",
            include_bytes!("../../../editor/assets/ui_icons/hub_generated/plus.png").as_slice(),
        ),
        (
            "editor.hub.folder",
            include_bytes!("../../../editor/assets/ui_icons/folder.png").as_slice(),
        ),
        (
            "editor.hub.minimize",
            include_bytes!("../../../editor/assets/ui_icons/top/minimize.png").as_slice(),
        ),
        (
            "editor.hub.maximize",
            include_bytes!("../../../editor/assets/ui_icons/top/maximize.png").as_slice(),
        ),
        (
            "editor.hub.close",
            include_bytes!("../../../editor/assets/ui_icons/top/close.png").as_slice(),
        ),
    ];
    for (key, bytes) in images {
        if let Ok(image) = image::load_from_memory(bytes) {
            let image = image.to_rgba8();
            let _ = store.insert_rgba(key, [image.width(), image.height()], image.into_raw());
        }
    }
}

pub fn resolve_hub_text(key: &str, model: &HubSurfaceModel, language: Language) -> String {
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
            hub_visible_projects(model).len(),
            t("app.hub_results_label", language)
        ),
        "hub.create.type" => match model.create_project_type {
            ProjectType::Game => t("app.hub_game_kind", language),
            ProjectType::Electronics => t("app.hub_electronics_kind", language),
        },
        "hub.create.error" => match model.create_error.as_ref() {
            Some(HubProjectCreateError::NameRequired) => {
                t("app.project_create_name_required", language)
            }
            Some(HubProjectCreateError::NameInvalid) => {
                t("app.project_create_name_invalid", language)
            }
            Some(HubProjectCreateError::LocationRequired) => {
                t("app.project_create_location_required", language)
            }
            Some(HubProjectCreateError::CreationFailed(detail)) => {
                format!("{} {detail}", t("app.project_create_failed", language))
            }
            None => String::new(),
        },
        "hub.context.title" => model
            .context_project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_default(),
        _ => resolve_dynamic_project_text(key, model, language).unwrap_or_else(|| t(key, language)),
    }
}

fn resolve_dynamic_project_text(
    key: &str,
    model: &HubSurfaceModel,
    language: Language,
) -> Option<String> {
    let visible_projects = hub_visible_projects(model);
    let visible_recent = hub_visible_recent(model);
    let featured = hub_visible_featured(model, &visible_projects);
    if let Some(suffix) = key.strip_prefix("hub.project.") {
        let (index, field) = suffix.split_once('.')?;
        let project = visible_projects.get(index.parse::<usize>().ok()?)?;
        return match field {
            "name" => Some(project.name.clone()),
            "kind" => Some(match project.project_type {
                ProjectType::Game => t("app.hub_type_game_label", language),
                ProjectType::Electronics => t("app.hub_type_electronics_label", language),
            }),
            "last-opened" => Some(format!(
                "{} {}",
                t("app.hub_last_opened", language),
                project.last_opened_label
            )),
            _ => None,
        };
    }
    if let Some(suffix) = key.strip_prefix("hub.activity.") {
        let (index, field) = suffix.split_once('.')?;
        let project = visible_recent.get(index.parse::<usize>().ok()?)?;
        return match field {
            "name" => Some(project.name.clone()),
            "meta" => Some(project.last_opened_label.clone()),
            _ => None,
        };
    }
    if let Some(suffix) = key.strip_prefix("hub.recent.") {
        let (index, field) = suffix.split_once('.')?;
        let project = visible_projects.get(index.parse::<usize>().ok()?)?;
        return match field {
            "name" => Some(project.name.clone()),
            "kind" => Some(match project.project_type {
                ProjectType::Game => t("app.hub_type_game_label", language),
                ProjectType::Electronics => t("app.hub_type_electronics_label", language),
            }),
            "meta" => Some(format!(
                "{} {}",
                t("app.hub_last_opened", language),
                project.last_opened_label
            )),
            _ => None,
        };
    }
    if let Some(suffix) = key.strip_prefix("hub.showcase.small.") {
        let (index, field) = suffix.split_once('.')?;
        let project = visible_recent.get(index.parse::<usize>().ok()?.saturating_add(1))?;
        return match field {
            "name" => Some(project.name.clone()),
            "kind" => Some(match project.project_type {
                ProjectType::Game => t("app.hub_type_game_label", language),
                ProjectType::Electronics => t("app.hub_type_electronics_label", language),
            }),
            "meta" => Some(project.last_opened_label.clone()),
            _ => None,
        };
    }
    match key {
        "hub.featured.kind" => featured.as_ref().map(|project| match project.project_type {
            ProjectType::Game => t("app.hub_type_game_label", language),
            ProjectType::Electronics => t("app.hub_type_electronics_label", language),
        }),
        "hub.featured.name" => featured.as_ref().map(|project| project.name.clone()),
        "hub.featured.path" => featured
            .as_ref()
            .map(|project| project.path.to_string_lossy().to_string()),
        "hub.featured.meta" => featured.as_ref().map(|project| {
            format!(
                "{} | {}",
                t("app.hub_last_opened", language),
                project.last_opened_label
            )
        }),
        _ => None,
    }
}
