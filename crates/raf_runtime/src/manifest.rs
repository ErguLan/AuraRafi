use raf_core::runtime_config::RuntimePreferences;
use raf_core::{
    config::{EngineSettings, Language, RenderExecutionPolicy, Theme},
    project::ProjectSettings,
    SceneGraph,
};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const MANIFEST_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub struct RuntimeManifest {
    #[serde(default)]
    pub node_graph: Option<raf_nodes::NodeGraph>,
    pub protocol: u32,
    pub instance: Uuid,
    pub project_root: PathBuf,
    pub project_name: String,
    pub ui_document_file: Option<PathBuf>,
    pub scene: SceneGraph,
    pub project_settings: ProjectSettings,
    pub host_settings: RuntimeHostSettings,
}
/// Deliberately excludes Agent providers, API keys and editor-only preferences.
#[derive(Clone, Serialize, Deserialize)]
pub struct RuntimeHostSettings {
    pub runtime: RuntimePreferences,
    pub language: Language,
    pub theme: Theme,
    pub render_execution_policy: RenderExecutionPolicy,
    pub vsync: bool,
    pub reduced_motion: bool,
    pub high_contrast: bool,
}
impl RuntimeHostSettings {
    pub fn from_engine(settings: &EngineSettings) -> Self {
        Self {
            runtime: settings.runtime.normalized(),
            language: settings.language,
            theme: settings.theme,
            render_execution_policy: settings.render_execution_policy,
            vsync: settings.vsync,
            reduced_motion: settings.prefers_reduced_motion,
            high_contrast: settings.high_contrast,
        }
    }
}
impl RuntimeManifest {
    pub fn read(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("runtime snapshot: {e}"))?;
        let mut bytes = Vec::new();
        file.take(MANIFEST_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MANIFEST_LIMIT {
            return Err("runtime snapshot exceeds 64 MiB".into());
        }
        let manifest: Self =
            ron::de::from_bytes(&bytes).map_err(|e| format!("runtime snapshot: {e}"))?;
        if manifest.protocol != 1 {
            return Err("unsupported runtime snapshot version".into());
        }
        if !manifest.project_root.is_dir() {
            return Err("runtime project directory is unavailable".into());
        }
        Ok(manifest)
    }
    pub fn write(&self, path: &Path) -> Result<(), String> {
        // Constructed from live authoring state; the project is never saved by Play.
        let bytes = ron::ser::to_string(self).map_err(|e| e.to_string())?;
        if bytes.len() > MANIFEST_LIMIT {
            return Err("runtime snapshot exceeds 64 MiB".into());
        }
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_round_trip_excludes_agent_credentials_and_old_settings_have_defaults() {
        let mut engine = EngineSettings::default();
        if let Some(provider) = engine.ai_providers.first_mut() {
            provider.api_key = "test-secret-must-not-leak".into();
        }
        let manifest = RuntimeManifest {
            node_graph: None,
            protocol: 1,
            instance: Uuid::new_v4(),
            project_root: PathBuf::from("project"),
            project_name: "test".into(),
            ui_document_file: None,
            scene: SceneGraph::new(),
            project_settings: ProjectSettings::default(),
            host_settings: RuntimeHostSettings::from_engine(&engine),
        };
        let serialized = ron::ser::to_string(&manifest).unwrap();
        assert!(!serialized.contains("test-secret-must-not-leak"));
        assert!(!serialized.contains("ai_providers"));
        let decoded: RuntimeManifest = ron::from_str(&serialized).unwrap();
        assert_eq!(decoded.instance, manifest.instance);
        let preferences: RuntimePreferences = ron::from_str("()").unwrap();
        assert_eq!(preferences, RuntimePreferences::default());
    }
}
