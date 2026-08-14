//! Project session registry.
//!
//! Sessions are independent editable worlds inside one project. Project assets
//! and scripts remain shared while scene, node graph, and UI document files are
//! scoped to the active session.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::project::ProjectType;

pub const SESSION_REGISTRY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectSessionKind {
    World,
    Interface,
    ElectronicsDesign,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSession {
    pub id: SessionId,
    pub name: String,
    pub kind: ProjectSessionKind,
    pub directory: PathBuf,
    pub scene_file: PathBuf,
    pub nodes_file: PathBuf,
    pub ui_document_file: PathBuf,
    pub schematic_file: PathBuf,
    pub pcb_file: PathBuf,
}

impl ProjectSession {
    pub fn new(name: impl Into<String>, kind: ProjectSessionKind) -> Self {
        let id = SessionId::new();
        let directory = PathBuf::from("sessions").join(id.0.to_string());
        Self {
            id,
            name: name.into(),
            kind,
            scene_file: directory.join("scene.ron"),
            nodes_file: directory.join("nodes.ron"),
            ui_document_file: directory.join("ui.ron"),
            schematic_file: directory.join("schematic.ron"),
            pcb_file: directory.join("pcb_layout.ron"),
            directory,
        }
    }

    pub fn legacy_main(project_type: ProjectType) -> Self {
        Self {
            id: SessionId::new(),
            name: "Main".to_string(),
            kind: default_kind(project_type),
            directory: PathBuf::new(),
            scene_file: PathBuf::from("scene.ron"),
            nodes_file: PathBuf::from("nodes.ron"),
            ui_document_file: PathBuf::from("ui.ron"),
            schematic_file: PathBuf::from("schematic.ron"),
            pcb_file: PathBuf::from("pcb_layout.ron"),
        }
    }

    pub fn path(&self, project_root: &Path, relative: &Path) -> PathBuf {
        project_root.join(relative)
    }

    /// Editor-only camera resource for this session. It is intentionally
    /// derived from the session directory instead of becoming a SceneGraph
    /// node, so it never appears in the user hierarchy.
    pub fn editor_camera_file(&self) -> PathBuf {
        self.directory.join("editor_camera.ron")
    }

    pub fn ensure_storage(&self, project_root: &Path) -> std::io::Result<()> {
        if !self.directory.as_os_str().is_empty() {
            fs::create_dir_all(project_root.join(&self.directory))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSessionRegistry {
    pub version: u32,
    pub active_session: SessionId,
    pub sessions: Vec<ProjectSession>,
}

impl ProjectSessionRegistry {
    pub const FILE_NAME: &'static str = "sessions/index.ron";

    pub fn new(project_type: ProjectType) -> Self {
        let session = ProjectSession::new("Main", default_kind(project_type));
        Self {
            version: SESSION_REGISTRY_VERSION,
            active_session: session.id,
            sessions: vec![session],
        }
    }

    pub fn load_or_legacy(project_root: &Path, project_type: ProjectType) -> Self {
        let path = project_root.join(Self::FILE_NAME);
        match fs::read_to_string(path)
            .ok()
            .and_then(|raw| ron::from_str::<Self>(&raw).ok())
        {
            Some(mut registry) if !registry.sessions.is_empty() => {
                if registry.active().is_none() {
                    registry.active_session = registry.sessions[0].id;
                }
                registry
            }
            _ => {
                let session = ProjectSession::legacy_main(project_type);
                Self {
                    version: SESSION_REGISTRY_VERSION,
                    active_session: session.id,
                    sessions: vec![session],
                }
            }
        }
    }

    pub fn save(&self, project_root: &Path) -> Result<(), String> {
        for session in &self.sessions {
            session
                .ensure_storage(project_root)
                .map_err(|error| format!("session storage: {error}"))?;
        }
        let path = project_root.join(Self::FILE_NAME);
        let parent = path
            .parent()
            .ok_or_else(|| "session registry has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| format!("sessions directory: {error}"))?;
        let raw = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| format!("session registry serialize: {error}"))?;
        fs::write(path, raw).map_err(|error| format!("session registry write: {error}"))
    }

    pub fn active(&self) -> Option<&ProjectSession> {
        self.sessions
            .iter()
            .find(|session| session.id == self.active_session)
    }

    pub fn active_mut(&mut self) -> Option<&mut ProjectSession> {
        self.sessions
            .iter_mut()
            .find(|session| session.id == self.active_session)
    }

    pub fn set_active(&mut self, id: SessionId) -> bool {
        if self.sessions.iter().any(|session| session.id == id) {
            self.active_session = id;
            true
        } else {
            false
        }
    }

    pub fn create(&mut self, name: impl Into<String>, kind: ProjectSessionKind) -> SessionId {
        let session = ProjectSession::new(name, kind);
        let id = session.id;
        self.sessions.push(session);
        id
    }

    pub fn rename(&mut self, id: SessionId, name: impl Into<String>) -> Result<(), String> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err("session name cannot be empty".to_string());
        }
        if self
            .sessions
            .iter()
            .any(|session| session.id != id && session.name.eq_ignore_ascii_case(&name))
        {
            return Err("a session with that name already exists".to_string());
        }
        let session = self
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
            .ok_or_else(|| "session was not found".to_string())?;
        session.name = name;
        Ok(())
    }

    pub fn duplicate(&mut self, id: SessionId, name: impl Into<String>) -> Option<SessionId> {
        let source = self.sessions.iter().find(|session| session.id == id)?;
        let mut duplicate = ProjectSession::new(name, source.kind);
        duplicate.scene_file = duplicate.directory.join("scene.ron");
        duplicate.nodes_file = duplicate.directory.join("nodes.ron");
        duplicate.ui_document_file = duplicate.directory.join("ui.ron");
        duplicate.schematic_file = duplicate.directory.join("schematic.ron");
        duplicate.pcb_file = duplicate.directory.join("pcb_layout.ron");
        let duplicate_id = duplicate.id;
        self.sessions.push(duplicate);
        Some(duplicate_id)
    }

    pub fn remove(&mut self, id: SessionId) -> bool {
        if self.sessions.len() <= 1 || self.active_session == id {
            return false;
        }
        let initial_len = self.sessions.len();
        self.sessions.retain(|session| session.id != id);
        self.sessions.len() != initial_len
    }
}

fn default_kind(project_type: ProjectType) -> ProjectSessionKind {
    match project_type {
        ProjectType::Game => ProjectSessionKind::World,
        ProjectType::Electronics => ProjectSessionKind::ElectronicsDesign,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_has_a_single_active_main_session() {
        let registry = ProjectSessionRegistry::new(ProjectType::Game);
        assert_eq!(registry.sessions.len(), 1);
        assert_eq!(
            registry.active().map(|session| session.name.as_str()),
            Some("Main")
        );
        assert_eq!(
            registry.active().map(|session| session.kind),
            Some(ProjectSessionKind::World)
        );
    }

    #[test]
    fn legacy_projects_keep_their_existing_root_paths() {
        let session = ProjectSession::legacy_main(ProjectType::Game);
        assert_eq!(session.scene_file, PathBuf::from("scene.ron"));
        assert_eq!(session.nodes_file, PathBuf::from("nodes.ron"));
        assert_eq!(
            session.editor_camera_file(),
            PathBuf::from("editor_camera.ron")
        );
    }

    #[test]
    fn active_session_cannot_be_removed() {
        let mut registry = ProjectSessionRegistry::new(ProjectType::Game);
        let active = registry.active_session;
        assert!(!registry.remove(active));
    }

    #[test]
    fn session_rename_keeps_storage_identity_and_rejects_duplicates() {
        let mut registry = ProjectSessionRegistry::new(ProjectType::Game);
        let main = registry.active_session;
        let other = registry.create("Other", ProjectSessionKind::World);

        let original_directory = registry
            .active()
            .map(|session| session.directory.clone())
            .expect("main session exists");
        registry.rename(main, "Renamed").expect("rename succeeds");
        assert_eq!(
            registry.active().map(|session| session.name.as_str()),
            Some("Renamed")
        );
        assert_eq!(
            registry.active().map(|session| &session.directory),
            Some(&original_directory)
        );
        assert!(registry.rename(other, "renamed").is_err());
    }
}
