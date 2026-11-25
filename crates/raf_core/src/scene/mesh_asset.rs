//! Stable scene reference to a static imported mesh asset.

use serde::{Deserialize, Serialize};

/// Project-relative reference to a decoded static mesh asset.
///
/// The path is relative to the project `assets` directory and never includes
/// the `assets/` prefix. `content_hash` lets renderers and thumbnails reuse a
/// decoded model when the file has not changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshAssetRef {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Optional source node index inside a multi-node glTF document. `None`
    /// flattens every static mesh in the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<usize>,
    /// Uniform import scale applied after the node transform.
    #[serde(default = "default_import_scale")]
    pub import_scale: f32,
    /// Optional local-space AABB `[min_x, min_y, min_z, max_x, max_y, max_z]`
    /// cached from import metadata so selection, focus and snapping can work
    /// without decoding the model again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f32; 6]>,
}

/// Source skin and relative joint paths in the placed model hierarchy.
/// The mesh lives directly under the same model root as the joint hierarchy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinBinding {
    pub joints: Vec<Vec<String>>,
}

const fn default_import_scale() -> f32 {
    1.0
}

impl MeshAssetRef {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: normalize_reference(path.into()),
            content_hash: None,
            node: None,
            import_scale: 1.0,
            bounds: None,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.path.is_empty()
            || self.path.starts_with('/')
            || self.path.contains('\\')
            || self.path.contains(':')
            || self
                .path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("Mesh asset path must be project-relative without traversal components.");
        }
        if !self.import_scale.is_finite()
            || self.import_scale <= 0.0
            || self.import_scale > 10_000.0
        {
            return Err("Mesh import scale must be finite and greater than zero.");
        }
        if let Some(bounds) = self.bounds {
            if !bounds.iter().all(|value| value.is_finite())
                || bounds[0] > bounds[3]
                || bounds[1] > bounds[4]
                || bounds[2] > bounds[5]
            {
                return Err("Mesh bounds must be finite and ordered min <= max.");
            }
        }
        let lower = self.path.to_ascii_lowercase();
        if !(lower.ends_with(".glb") || lower.ends_with(".gltf")) {
            return Err("Static mesh assets currently support .glb and .gltf files.");
        }
        Ok(())
    }
}

pub fn normalize_reference(value: String) -> String {
    let normalized = value.replace('\\', "/");
    normalized
        .strip_prefix("assets/")
        .unwrap_or(&normalized)
        .trim_start_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_normalized_and_confined() {
        assert_eq!(
            normalize_reference("assets/models/a.glb".into()),
            "models/a.glb"
        );
        assert!(MeshAssetRef::new("models/a.glb").validate().is_ok());
        assert!(MeshAssetRef::new("../a.glb").validate().is_err());
        assert!(MeshAssetRef::new("C:/a.glb").validate().is_err());
        assert!(MeshAssetRef::new("models/a.png").validate().is_err());
    }
}
