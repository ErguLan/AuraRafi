//! RON persistence, debug formatting, and default construction for the scene graph.

use std::fmt;

use super::SceneGraph;

impl SceneGraph {
    /// Save the scene graph to a RON file.
    pub fn save_ron(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load a scene graph from a RON file. Returns default if file missing.
    pub fn load_ron(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(data) => ron::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
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
