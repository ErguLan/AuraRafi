//! Project management - create, load, save AuraRafi projects.
//!
//! A project is a directory containing:
//! - `project.ron` (metadata)
//! - `assets/` (imported assets)
//! - `scenes/` (scene files)
//! - `scripts/` (user scripts, if any)

use crate::config::{RenderPreset, ScriptExecutionMode, ScriptLanguageFlags};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::session::ProjectSessionRegistry;

fn default_true() -> bool {
    true
}

fn default_main_scene_name() -> String {
    "MainScene".to_string()
}

fn default_depth_resolution_scale() -> f32 {
    0.6
}

fn default_world_stream_region_size() -> f32 {
    128.0
}

fn default_world_stream_load_radius() -> u32 {
    3
}

/// Type of project: Game or Electronics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Game,
    Electronics,
}

impl ProjectType {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Game => "Game Project",
            Self::Electronics => "Electronics Project",
        }
    }
}

/// Per-project settings stored inside `project.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSettings {
    #[serde(default)]
    pub runtime: crate::runtime_config::ProjectRuntimeSettings,
    /// Show the hierarchy panel while editing this project.
    #[serde(default = "default_true")]
    pub show_hierarchy_panel: bool,
    /// Show the properties panel while editing this project.
    #[serde(default = "default_true")]
    pub show_properties_panel: bool,
    /// Whether this project accepts manual slash commands from the console.
    #[serde(default)]
    pub enable_console_commands: bool,
    /// Hard gate for advanced GPU-heavy project features.
    ///
    /// This does not choose the base backend anymore. Engine-level render
    /// policy decides whether AuraRafi prefers GPU or CPU execution, while
    /// this flag only unlocks heavier project-side GPU features.
    #[serde(default)]
    pub allow_gpu_features: bool,
    /// Runtime systems that can be toggled project-by-project.
    #[serde(default = "default_true")]
    pub enable_audio: bool,
    #[serde(default = "default_true")]
    pub enable_physics: bool,
    #[serde(default = "default_true")]
    pub pause_when_unfocused: bool,
    /// Save immediately after each committed editor action.
    #[serde(default)]
    pub linear_save: bool,
    /// Legacy runtime preset retained for existing project files. The editor
    /// viewport does not apply these effect presets.
    #[serde(default)]
    pub runtime_render_preset: RenderPreset,
    /// Resolution policy for the editor viewport. This only changes render
    /// resolution; it does not enable optional effects or alter frame pacing.
    #[serde(default)]
    pub viewport_resolution: ViewportResolutionMode,
    /// Curved-geometry detail policy for the editor viewport.
    #[serde(default)]
    pub geometry_detail: GeometryDetailMode,
    /// Per-project cap for decoded and resident base-color textures.
    #[serde(default)]
    pub texture_quality: TextureQualityMode,
    /// Legacy software-depth preference retained for existing project files.
    #[serde(default)]
    pub depth_accurate: bool,
    /// Legacy software-depth resolution scale retained for existing projects.
    #[serde(default = "default_depth_resolution_scale")]
    pub depth_resolution_scale: f32,
    /// Enable region-based scene visibility for large-world projects.
    #[serde(default)]
    pub world_streaming_enabled: bool,
    /// Edge length in meters for a streamed world region.
    #[serde(default = "default_world_stream_region_size")]
    pub world_stream_region_size: f32,
    /// Number of visible regions kept around the viewport camera.
    #[serde(default = "default_world_stream_load_radius")]
    pub world_stream_load_radius: u32,
    /// Legacy LOD bias retained for existing project files; visibility culling
    /// does not currently consume it.
    #[serde(default)]
    pub world_stream_lod_bias: i8,
    /// Scene name to create/use by default.
    #[serde(default = "default_main_scene_name")]
    pub default_scene_name: String,
    // -- Scripting (v0.8.x) --
    /// Whether scripting is enabled for this project.
    #[serde(default = "default_true")]
    pub enable_scripting: bool,
    /// Bitflags of allowed script languages. C++ (WASM) requires explicit opt-in.
    #[serde(default)]
    pub allowed_script_languages: ScriptLanguageFlags,
    /// When scripts execute during the project lifecycle.
    #[serde(default)]
    pub script_execution_mode: ScriptExecutionMode,
    /// Attach a default script to newly created entities.
    #[serde(default)]
    pub auto_attach_scripts: bool,
    /// Editor building assist for Game projects. Free keeps today's behavior;
    /// Organized quantizes transforms and blocks entity overlap.
    #[serde(default)]
    pub building_style: BuildingStyle,
    /// Fixed translation/scale quantum in meters used by the organized
    /// building style. 1.0 places objects on whole meters.
    #[serde(default = "default_building_snap_step")]
    pub building_snap_step: f32,
    /// Base schematic snap/grid spacing in millimeters for this project.
    #[serde(default = "default_electronics_grid_step_mm")]
    pub electronics_schematic_grid_step_mm: f32,
    /// Base PCB snap/grid spacing in millimeters for this project.
    #[serde(default = "default_electronics_grid_step_mm")]
    pub electronics_pcb_grid_step_mm: f32,
    /// Whether Electronics placement and routing snap to the project grid.
    #[serde(default = "default_true")]
    pub electronics_snap_to_grid: bool,
}

/// Building assist style for the Game viewport editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildingStyle {
    /// Unconstrained authoring, identical to the historical behavior.
    Free,
    /// Grid-quantized movement with collision-aware clamping against walls.
    Organized,
}

impl Default for BuildingStyle {
    fn default() -> Self {
        Self::Free
    }
}

impl BuildingStyle {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Organized => "professional",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "free" => Some(Self::Free),
            "professional" => Some(Self::Organized),
            _ => None,
        }
    }
}

fn default_building_snap_step() -> f32 {
    1.0
}

/// A bounded render-resolution policy for the editor viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportResolutionMode {
    /// Spend fewer GPU pixels and allow a lower resolution under pressure.
    Efficient,
    /// Keep native resolution unless measured frame cost requires scaling down.
    Adaptive,
    /// Keep the viewport at native resolution even when the frame budget is exceeded.
    FullResolution,
}

/// Balances curved-mesh tessellation against the amount of visible detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeometryDetailMode {
    /// Always use the lowest-cost curved meshes.
    Efficient,
    /// Choose detail from projected size, with hysteresis at LOD boundaries.
    Adaptive,
    /// Always use the highest built-in curved-mesh detail.
    Detailed,
}

/// Hard image-dimension and residency caps for scene base-color textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureQualityMode {
    /// 256px maximum and a 4 MiB CPU/GPU texture-cache budget.
    Efficient,
    /// 512px maximum and a 12 MiB CPU/GPU texture-cache budget.
    Adaptive,
    /// 1024px maximum and a 32 MiB CPU/GPU texture-cache budget.
    Detailed,
}

impl Default for GeometryDetailMode {
    fn default() -> Self {
        Self::Adaptive
    }
}

impl Default for TextureQualityMode {
    fn default() -> Self {
        Self::Adaptive
    }
}

impl TextureQualityMode {
    pub const fn max_dimension(self) -> u32 {
        match self {
            Self::Efficient => 256,
            Self::Adaptive => 512,
            Self::Detailed => 1024,
        }
    }

    pub const fn cache_budget_bytes(self) -> u64 {
        match self {
            Self::Efficient => 4 * 1024 * 1024,
            Self::Adaptive => 12 * 1024 * 1024,
            Self::Detailed => 32 * 1024 * 1024,
        }
    }
}

impl Default for ViewportResolutionMode {
    fn default() -> Self {
        Self::Adaptive
    }
}

impl ViewportResolutionMode {
    /// Returns the minimum and maximum internal-resolution scale for this mode.
    pub const fn resolution_range(self) -> (f32, f32) {
        match self {
            Self::Efficient => (0.50, 0.85),
            Self::Adaptive => (0.60, 1.0),
            Self::FullResolution => (1.0, 1.0),
        }
    }
}

fn default_electronics_grid_step_mm() -> f32 {
    20.0
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            show_hierarchy_panel: true,
            runtime: crate::runtime_config::ProjectRuntimeSettings::default(),
            show_properties_panel: true,
            enable_console_commands: false,
            allow_gpu_features: false,
            enable_audio: true,
            enable_physics: true,
            pause_when_unfocused: true,
            linear_save: false,
            runtime_render_preset: RenderPreset::Potato,
            viewport_resolution: ViewportResolutionMode::Adaptive,
            geometry_detail: GeometryDetailMode::Adaptive,
            texture_quality: TextureQualityMode::Adaptive,
            depth_accurate: false,
            depth_resolution_scale: default_depth_resolution_scale(),
            world_streaming_enabled: false,
            world_stream_region_size: default_world_stream_region_size(),
            world_stream_load_radius: default_world_stream_load_radius(),
            world_stream_lod_bias: 0,
            default_scene_name: default_main_scene_name(),
            enable_scripting: true,
            allowed_script_languages: ScriptLanguageFlags::DEFAULT,
            script_execution_mode: ScriptExecutionMode::EditorOnly,
            auto_attach_scripts: false,
            building_style: BuildingStyle::Free,
            building_snap_step: default_building_snap_step(),
            electronics_schematic_grid_step_mm: default_electronics_grid_step_mm(),
            electronics_pcb_grid_step_mm: default_electronics_grid_step_mm(),
            electronics_snap_to_grid: true,
        }
    }
}

/// Metadata for a single AuraRafi project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique project identifier.
    pub id: Uuid,
    /// Human-readable project name.
    pub name: String,
    /// Project type (game or electronics).
    pub project_type: ProjectType,
    /// Absolute path to the project root directory.
    pub path: PathBuf,
    /// Date created.
    pub created_at: DateTime<Utc>,
    /// Date last modified.
    pub modified_at: DateTime<Utc>,
    /// Engine version used to create this project.
    pub engine_version: String,
    /// Project-specific runtime/editor settings.
    #[serde(default)]
    pub settings: ProjectSettings,
}

impl Project {
    /// Project metadata file name.
    pub const META_FILE: &'static str = "project.ron";

    /// Create a new project on disk. Creates the directory structure and
    /// writes the metadata file.
    pub fn create(
        name: &str,
        project_type: ProjectType,
        parent_dir: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let project_dir = parent_dir.join(name);
        if project_dir.join(Self::META_FILE).exists() {
            return Err(format!("project already exists: '{}'", project_dir.display()).into());
        }
        create_project_directory(&project_dir, "project directory")?;
        create_project_directory(&project_dir.join("assets"), "assets directory")?;
        create_project_directory(&project_dir.join("scenes"), "scenes directory")?;
        create_project_directory(&project_dir.join("scripts"), "scripts directory")?;

        let now = Utc::now();
        let mut project = Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            project_type,
            path: project_dir.clone(),
            created_at: now,
            modified_at: now,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            settings: ProjectSettings::default(),
        };

        let sessions = ProjectSessionRegistry::new(project_type);
        sessions.save(&project_dir).map_err(|error| {
            format!(
                "could not initialize sessions in '{}': {error}",
                project_dir.display()
            )
        })?;
        if project_type == ProjectType::Game {
            // Seed an editable template, never a runtime fallback or controller.
            let mut scene = crate::scene::SceneGraph::new();
            let camera_id = scene.add_root("Camera");
            let camera = scene.get_mut(camera_id).expect("new camera entity");
            camera.game_camera = Some(crate::runtime_config::GameCamera::default());
            camera.position = glam::Vec3::new(0.0, 2.0, 6.0);
            camera.rotation.x = -15.0;
            project.settings.runtime.active_camera = Some(camera.uuid);
            let session = sessions.active().expect("new project has a Main session");
            let scene_path = session.path(&project_dir, &session.scene_file);
            scene.save_ron(&scene_path).map_err(|error| {
                format!(
                    "could not initialize Game scene '{}': {error}",
                    scene_path.display()
                )
            })?;
        }
        project.save().map_err(|error| {
            format!(
                "could not write project metadata '{}': {error}",
                project_dir.join(Self::META_FILE).display()
            )
        })?;
        Ok(project)
    }

    /// Save the project metadata to its directory.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let meta_path = self.path.join(Self::META_FILE);
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(meta_path, data)?;
        Ok(())
    }

    /// Load a project from a directory.
    pub fn load(project_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let meta_path = project_dir.join(Self::META_FILE);
        let data = std::fs::read_to_string(meta_path)?;
        let project: Self = ron::from_str(&data)?;
        Ok(project)
    }
}

/// Registry of recent projects for the Project Hub.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecentProjects {
    pub projects: Vec<RecentProjectEntry>,
}

/// A lightweight entry for the recent projects list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProjectEntry {
    pub name: String,
    pub path: PathBuf,
    pub project_type: ProjectType,
    #[serde(default = "default_utc_now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_utc_now")]
    pub modified_at: DateTime<Utc>,
    pub last_opened: DateTime<Utc>,
    /// Statistical metadata (e.g. number of nodes or components).
    #[serde(default)]
    pub n_elements: u32,
}

fn default_utc_now() -> DateTime<Utc> {
    Utc::now()
}

impl RecentProjects {
    pub const FILE_NAME: &'static str = "recent_projects.ron";

    /// Add or update a project in the recent list.
    pub fn add(&mut self, project: &Project) {
        // Remove existing entry with same path if present.
        self.projects.retain(|p| p.path != project.path);

        self.projects.insert(
            0,
            RecentProjectEntry {
                name: project.name.clone(),
                path: project.path.clone(),
                project_type: project.project_type,
                created_at: project.created_at,
                modified_at: project.modified_at,
                last_opened: Utc::now(),
                n_elements: 0, // Placeholder
            },
        );

        // Keep at most 20 recent projects.
        self.projects.truncate(20);
    }

    /// Save to disk.
    pub fn save(&self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = dir.join(Self::FILE_NAME);
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load from disk. Returns empty if file doesn't exist.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(Self::FILE_NAME);
        match std::fs::read_to_string(&path) {
            Ok(data) => ron::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}

fn create_project_directory(
    path: &Path,
    description: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "could not create {description} '{}': {error}",
            path.display()
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_create_initializes_files_and_reports_the_failing_path() {
        let root = std::env::temp_dir().join(format!("raf-project-create-{}", Uuid::new_v4()));
        let project = Project::create("Working", ProjectType::Game, &root)
            .expect("project should be created in a writable directory");

        assert!(project.path.join(Project::META_FILE).is_file());
        assert!(project.path.join("assets").is_dir());
        assert!(project.path.join("scenes").is_dir());
        assert!(project.path.join("scripts").is_dir());
        assert!(project
            .path
            .join(ProjectSessionRegistry::FILE_NAME)
            .is_file());

        let sessions = ProjectSessionRegistry::load_or_legacy(&project.path, ProjectType::Game);
        let session = sessions.active().expect("Main session");
        let scene: crate::scene::SceneGraph = ron::from_str(
            &std::fs::read_to_string(session.path(&project.path, &session.scene_file)).unwrap(),
        )
        .expect("persisted Game template");
        assert_eq!(scene.len(), 1);
        let (_, camera) = scene.iter().next().unwrap();
        assert_eq!(camera.name, "Camera");
        assert_eq!(camera.primitive, crate::scene::Primitive::Empty);
        assert_eq!(camera.position, glam::Vec3::new(0.0, 2.0, 6.0));
        assert!(camera.scripts.is_empty());
        assert!(camera.game_camera.as_ref().unwrap().validate().is_ok());
        assert_eq!(project.settings.runtime.active_camera, Some(camera.uuid));
        assert_eq!(
            Project::load(&project.path)
                .unwrap()
                .settings
                .runtime
                .active_camera,
            Some(camera.uuid)
        );
        assert!(Project::create("Working", ProjectType::Game, &root).is_err());
        assert_eq!(Project::load(&project.path).unwrap().id, project.id);
        let electronics = Project::create("Circuit", ProjectType::Electronics, &root).unwrap();
        assert_eq!(electronics.settings.runtime.active_camera, None);
        let electronics_sessions =
            ProjectSessionRegistry::load_or_legacy(&electronics.path, ProjectType::Electronics);
        let electronics_main = electronics_sessions.active().unwrap();
        assert!(!electronics_main
            .path(&electronics.path, &electronics_main.scene_file)
            .exists());
        assert_eq!(crate::scene::SceneGraph::new().len(), 0);

        let blocked_parent = root.join("not-a-directory");
        std::fs::write(&blocked_parent, "blocker").expect("create blocker file");
        let expected_path = blocked_parent.join("Broken");
        let error = Project::create("Broken", ProjectType::Game, &blocked_parent)
            .expect_err("a file cannot be used as the parent directory")
            .to_string();

        assert!(error.contains("could not create project directory"));
        assert!(error.contains(&expected_path.to_string_lossy().to_string()));

        std::fs::remove_dir_all(&root).expect("remove isolated test project");
    }

    #[test]
    fn world_streaming_defaults_are_lightweight_and_persisted() {
        let settings = ProjectSettings::default();
        assert_eq!(
            settings.viewport_resolution,
            ViewportResolutionMode::Adaptive
        );
        assert_eq!(settings.geometry_detail, GeometryDetailMode::Adaptive);
        assert_eq!(settings.texture_quality, TextureQualityMode::Adaptive);
        assert!(!settings.world_streaming_enabled);
        assert_eq!(settings.world_stream_region_size, 128.0);
        assert_eq!(settings.world_stream_load_radius, 3);
        assert_eq!(settings.world_stream_lod_bias, 0);

        let serialized = ron::ser::to_string(&settings).expect("settings serialize");
        let loaded: ProjectSettings = ron::from_str(&serialized).expect("settings deserialize");
        assert_eq!(loaded.viewport_resolution, ViewportResolutionMode::Adaptive);
        assert_eq!(loaded.geometry_detail, GeometryDetailMode::Adaptive);
        assert_eq!(loaded.texture_quality, TextureQualityMode::Adaptive);
        assert_eq!(loaded.world_stream_region_size, 128.0);
    }

    #[test]
    fn texture_quality_modes_keep_explicit_dimension_and_memory_caps() {
        assert_eq!(TextureQualityMode::Efficient.max_dimension(), 256);
        assert_eq!(
            TextureQualityMode::Efficient.cache_budget_bytes(),
            4 * 1024 * 1024
        );
        assert_eq!(TextureQualityMode::Adaptive.max_dimension(), 512);
        assert_eq!(
            TextureQualityMode::Adaptive.cache_budget_bytes(),
            12 * 1024 * 1024
        );
        assert_eq!(TextureQualityMode::Detailed.max_dimension(), 1024);
        assert_eq!(
            TextureQualityMode::Detailed.cache_budget_bytes(),
            32 * 1024 * 1024
        );
    }

    #[test]
    fn older_settings_files_receive_adaptive_geometry_detail() {
        let loaded: ProjectSettings = serde_json::from_str("{}").expect("legacy settings load");
        assert_eq!(loaded.geometry_detail, GeometryDetailMode::Adaptive);
    }
}
