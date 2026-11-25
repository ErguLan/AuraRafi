//! RON persistence, debug formatting, and default construction for the scene graph.

use std::fmt;

use super::SceneGraph;

impl SceneGraph {
    /// Save the scene graph to a RON file.
    pub fn save_ron(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        crate::project_store::write_atomic(path, data.as_bytes())
            .map_err(|error| std::io::Error::other(error))?;
        Ok(())
    }

    /// Load a scene graph with the recoverable previous generation as a
    /// fallback. Missing files produce an empty scene; corrupt files are
    /// reported instead of silently discarding the user's document.
    pub fn load_ron_checked(path: &std::path::Path) -> Result<Self, String> {
        let Some(bytes) = crate::project_store::read_recoverable(path)? else {
            return Ok(Self::default());
        };
        let data = String::from_utf8(bytes).map_err(|error| error.to_string())?;
        match ron::from_str(&data) {
            Ok(scene) => Ok(scene),
            Err(primary_error) => {
                let name = path
                    .file_name()
                    .ok_or_else(|| "Scene file has no name".to_string())?
                    .to_string_lossy();
                let backup = path.with_file_name(format!(".{name}.previous"));
                if let Ok(bytes) = std::fs::read(&backup) {
                    if let Ok(data) = String::from_utf8(bytes) {
                        if let Ok(scene) = ron::from_str(&data) {
                            return Ok(scene);
                        }
                    }
                }
                Err(format!(
                    "Could not parse scene '{}': {primary_error}",
                    path.display()
                ))
            }
        }
    }

    /// Compatibility helper for call sites that cannot yet surface an error.
    /// Prefer [`Self::load_ron_checked`] in new code.
    pub fn load_ron(path: &std::path::Path) -> Self {
        match Self::load_ron_checked(path) {
            Ok(scene) => scene,
            Err(error) => {
                tracing::error!(%error, path = %path.display(), "scene load failed");
                Self::default()
            }
        }
    }
}

impl fmt::Debug for SceneGraph {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneGraph")
            .field("environment", &self.environment)
            .field("nodes", &self.nodes)
            .field("roots", &self.roots)
            .finish()
    }
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}
