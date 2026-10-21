//! Strict, project-confined session preparation. Never saves the editor or guesses a world.
use crate::manifest::{RuntimeHostSettings, RuntimeManifest, MANIFEST_LIMIT};
use raf_core::{
    project::ProjectSettings,
    session::{ProjectSessionKind, ProjectSessionRegistry, SessionId},
};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct SessionSource {
    pub root: PathBuf,
    pub project_id: Uuid,
    pub project_name: String,
    pub project_settings: ProjectSettings,
    pub host_settings: RuntimeHostSettings,
    pub save_root_grant: Option<PathBuf>,
    pub development_slot: u32,
}
impl SessionSource {
    pub fn from_manifest(manifest: &RuntimeManifest) -> Self {
        Self {
            root: manifest.project_root.clone(),
            project_id: manifest.project_id,
            project_name: manifest.project_name.clone(),
            project_settings: manifest.project_settings.clone(),
            host_settings: manifest.host_settings.clone(),
            save_root_grant: manifest.save_root_grant.clone(),
            development_slot: manifest.development_slot,
        }
    }
    pub fn load(&self, session: Uuid, instance: Uuid) -> Result<RuntimeManifest, String> {
        let root = self.root.canonicalize().map_err(|e| e.to_string())?;
        let registry: ProjectSessionRegistry = ron::de::from_bytes(&read_confined(
            &root,
            Path::new(ProjectSessionRegistry::FILE_NAME),
            1024 * 1024,
        )?)
        .map_err(|e| format!("runtime.session_registry: {e}"))?;
        if registry.version != raf_core::session::SESSION_REGISTRY_VERSION
            || registry.sessions.len() > 128
            || registry
                .sessions
                .iter()
                .filter(|s| s.id == SessionId(session))
                .count()
                != 1
        {
            return Err("runtime.invalid_session_registry".into());
        }
        let selected = registry
            .sessions
            .iter()
            .find(|s| s.id == SessionId(session))
            .ok_or("runtime.session_missing")?;
        if selected.kind != ProjectSessionKind::World {
            return Err("runtime.session_not_world".into());
        }
        for relative in [
            &selected.scene_file,
            &selected.nodes_file,
            &selected.ui_document_file,
        ] {
            validate_relative(relative)?;
        }
        let scene =
            ron::de::from_bytes(&read_confined(&root, &selected.scene_file, MANIFEST_LIMIT)?)
                .map_err(|e| format!("runtime.session_scene: {e}"))?;
        let node_graph = if root.join(&selected.nodes_file).exists() {
            Some(
                ron::de::from_bytes(&read_confined(&root, &selected.nodes_file, 1024 * 1024)?)
                    .map_err(|e| format!("runtime.session_nodes: {e}"))?,
            )
        } else {
            None
        };
        Ok(RuntimeManifest {
            project_id: self.project_id,
            session_id: Some(session),
            development_slot: self.development_slot,
            save_root_grant: self.save_root_grant.clone(),
            protocol: 1,
            instance,
            project_root: root,
            project_name: self.project_name.clone(),
            scene,
            node_graph,
            ui_document_file: Some(selected.ui_document_file.clone()),
            project_settings: self.project_settings.clone(),
            host_settings: self.host_settings.clone(),
        })
    }
}
fn read_confined(root: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>, String> {
    validate_relative(relative)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("runtime.session_path_escape".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("runtime.session_size_limit".into());
    }
    Ok(bytes)
}
fn validate_relative(relative: &Path) -> Result<(), String> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("runtime.session_path_escape".into());
    }
    Ok(())
}
