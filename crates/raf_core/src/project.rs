//! Project management - create, load, save AuraRafi projects.
//!
//! A project is a directory containing:
//! - `project.ron` (metadata)
//! - `assets/` (imported assets)
//! - `scenes/` (scene files)
//! - `scripts/` (user scripts, if any)

use crate::config::RenderPreset;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn default_true() -> bool {
    true
}

fn default_main_scene_name() -> String {
    "MainScene".to_string()
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
    /// Show the hierarchy panel while editing this project.
    #[serde(default = "default_true")]
    pub show_hierarchy_panel: bool,
    /// Show the properties panel while editing this project.
    #[serde(default = "default_true")]
    pub show_properties_panel: bool,
    /// Whether complement tabs are available for this project.
    #[serde(default = "default_true")]
    pub enable_complements: bool,
    /// Hard gate for GPU-heavy features. Default false for potato mode.
    #[serde(default)]
    pub allow_gpu_features: bool,
    /// Runtime systems that can be toggled project-by-project.
    #[serde(default = "default_true")]
    pub enable_audio: bool,
    #[serde(default = "default_true")]
    pub enable_physics: bool,
    #[serde(default = "default_true")]
    pub pause_when_unfocused: bool,
    /// Preferred runtime preset for this specific project.
    #[serde(default)]
    pub runtime_render_preset: RenderPreset,
    /// Scene name to create/use by default.
    #[serde(default = "default_main_scene_name")]
    pub default_scene_name: String,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            show_hierarchy_panel: true,
            show_properties_panel: true,
            enable_complements: true,
            allow_gpu_features: false,
            enable_audio: true,
            enable_physics: true,
            pause_when_unfocused: true,
            runtime_render_preset: RenderPreset::Potato,
            default_scene_name: default_main_scene_name(),
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
        std::fs::create_dir_all(&project_dir)?;
        std::fs::create_dir_all(project_dir.join("assets"))?;
        std::fs::create_dir_all(project_dir.join("scenes"))?;
        std::fs::create_dir_all(project_dir.join("scripts"))?;

        let now = Utc::now();
        let project = Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            project_type,
            path: project_dir.clone(),
            created_at: now,
            modified_at: now,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            settings: ProjectSettings::default(),
        };

        project.save()?;
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
