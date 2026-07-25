//! JSON-backed primitive model manifests.
//!
//! The manifest is intentionally small: it describes a folder root plus a
//! list of primitive parts with transforms and colors. Higher-level tooling can
//! generate, save, diff, and import this data without generating Rust code.

use std::fmt;
use std::fs;
use std::path::Path;

use glam::Vec3;
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
use serde::{Deserialize, Serialize};

use crate::PrimitiveShape;

pub const PRIMITIVE_MODEL_SCHEMA_VERSION: u32 = 1;

const CUBE_JSON: &str = include_str!("prefabs/cube.json");
const SPHERE_JSON: &str = include_str!("prefabs/sphere.json");
const CYLINDER_JSON: &str = include_str!("prefabs/cylinder.json");
const PLANE_JSON: &str = include_str!("prefabs/plane.json");
const PLATFORM_JSON: &str = include_str!("prefabs/platform.json");
const TOWER_JSON: &str = include_str!("prefabs/tower.json");
const GATE_JSON: &str = include_str!("prefabs/gate.json");
const BOAT_JSON: &str = include_str!("prefabs/boat.json");
const BUILTIN_PRIMITIVE_MODEL_KINDS: &[&str] = &[
    "cube", "sphere", "cylinder", "plane", "platform", "tower", "gate", "boat",
];

#[derive(Debug)]
pub enum PrimitiveManifestError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for PrimitiveManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::Invalid(message) => write!(f, "Invalid primitive manifest: {message}"),
        }
    }
}

impl std::error::Error for PrimitiveManifestError {}

impl From<std::io::Error> for PrimitiveManifestError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for PrimitiveManifestError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveModelManifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub parts: Vec<PrimitiveModelPart>,
}

impl PrimitiveModelManifest {
    pub fn from_json_str(json: &str) -> Result<Self, PrimitiveManifestError> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn to_json_string_pretty(&self) -> Result<String, PrimitiveManifestError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, PrimitiveManifestError> {
        let json = fs::read_to_string(path)?;
        Self::from_json_str(&json)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), PrimitiveManifestError> {
        self.validate()?;
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, self.to_json_string_pretty()?)?;
        Ok(())
    }

    pub fn builtin(kind: &str) -> Result<Option<Self>, PrimitiveManifestError> {
        let json = match normalize_builtin_kind(kind).as_str() {
            "cube" => CUBE_JSON,
            "sphere" => SPHERE_JSON,
            "cylinder" => CYLINDER_JSON,
            "plane" => PLANE_JSON,
            "platform" => PLATFORM_JSON,
            "tower" => TOWER_JSON,
            "gate" => GATE_JSON,
            "boat" => BOAT_JSON,
            _ => return Ok(None),
        };
        Self::from_json_str(json).map(Some)
    }

    pub fn builtin_for_primitive(
        primitive: Primitive,
    ) -> Result<Option<Self>, PrimitiveManifestError> {
        let kind = match primitive {
            Primitive::Cube => "cube",
            Primitive::Sphere => "sphere",
            Primitive::Cylinder => "cylinder",
            Primitive::Plane => "plane",
            Primitive::Empty => return Ok(None),
        };
        Self::builtin(kind)
    }

    pub fn instantiate_into_scene(&self, scene: &mut SceneGraph) -> Vec<SceneNodeId> {
        self.instantiate_into_scene_with_name(scene, None)
    }

    pub fn instantiate_into_scene_with_name(
        &self,
        scene: &mut SceneGraph,
        root_name: Option<&str>,
    ) -> Vec<SceneNodeId> {
        let root_name = root_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(self.name.as_str());
        let root = scene.add_root_folder(root_name);
        let mut created = Vec::with_capacity(self.parts.len() + 1);
        created.push(root);

        for part in &self.parts {
            let id = scene.add_child(root, &part.name);
            if let Some(node) = scene.get_mut(id) {
                apply_part_to_node(node, part);
            }
            created.push(id);
        }

        created
    }

    pub fn instantiate_single_root_into_scene(
        &self,
        scene: &mut SceneGraph,
        root_name: Option<&str>,
        source_asset: Option<&str>,
    ) -> Result<SceneNodeId, PrimitiveManifestError> {
        self.validate()?;
        if self.parts.len() != 1 {
            return Err(PrimitiveManifestError::Invalid(format!(
                "single-root import requires exactly one part, got {}",
                self.parts.len()
            )));
        }

        let part = &self.parts[0];
        let name = root_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(part.name.as_str());
        let id = scene.add_root_with_primitive(name, primitive_shape_to_scene(part.primitive));
        if let Some(node) = scene.get_mut(id) {
            apply_part_to_node(node, part);
            node.name = name.to_string();
            node.source_asset = source_asset.map(str::to_string);
            node.source_schema_version = Some(self.schema_version);
        }
        Ok(id)
    }

    pub fn validate(&self) -> Result<(), PrimitiveManifestError> {
        if self.schema_version == 0 {
            return Err(PrimitiveManifestError::Invalid(
                "schema_version must be greater than zero".to_string(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(PrimitiveManifestError::Invalid(
                "name cannot be empty".to_string(),
            ));
        }
        if self.parts.is_empty() {
            return Err(PrimitiveManifestError::Invalid(
                "parts cannot be empty".to_string(),
            ));
        }

        for (index, part) in self.parts.iter().enumerate() {
            part.validate(index)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveModelPart {
    pub name: String,
    pub primitive: PrimitiveShape,
    #[serde(default = "default_position")]
    pub position: [f32; 3],
    #[serde(default = "default_rotation")]
    pub rotation_degrees: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    #[serde(default = "default_color")]
    pub color_rgba: [u8; 4],
}

impl PrimitiveModelPart {
    fn validate(&self, index: usize) -> Result<(), PrimitiveManifestError> {
        if self.name.trim().is_empty() {
            return Err(PrimitiveManifestError::Invalid(format!(
                "part {index} has an empty name"
            )));
        }
        validate_finite_vec3(self.position, "position", index)?;
        validate_finite_vec3(self.rotation_degrees, "rotation_degrees", index)?;
        validate_finite_vec3(self.scale, "scale", index)?;
        if self.scale.iter().any(|component| *component <= 0.0) {
            return Err(PrimitiveManifestError::Invalid(format!(
                "part {index} scale components must be positive"
            )));
        }
        Ok(())
    }
}

pub fn builtin_primitive_model_kinds() -> &'static [&'static str] {
    BUILTIN_PRIMITIVE_MODEL_KINDS
}

fn primitive_shape_to_scene(shape: PrimitiveShape) -> Primitive {
    match shape {
        PrimitiveShape::Cube => Primitive::Cube,
        PrimitiveShape::Sphere => Primitive::Sphere,
        PrimitiveShape::Cylinder => Primitive::Cylinder,
        PrimitiveShape::Plane => Primitive::Plane,
    }
}

fn apply_part_to_node(node: &mut SceneNode, part: &PrimitiveModelPart) {
    node.primitive = primitive_shape_to_scene(part.primitive);
    node.position = vec3_from_array(part.position);
    node.rotation = vec3_from_array(part.rotation_degrees);
    node.scale = vec3_from_array(part.scale);
    node.color = NodeColor::rgba(
        part.color_rgba[0],
        part.color_rgba[1],
        part.color_rgba[2],
        part.color_rgba[3],
    );
}

fn normalize_builtin_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "" => "platform".to_string(),
        "ship" => "boat".to_string(),
        other => other.to_string(),
    }
}

fn validate_finite_vec3(
    value: [f32; 3],
    field: &str,
    index: usize,
) -> Result<(), PrimitiveManifestError> {
    if value.iter().all(|component| component.is_finite()) {
        return Ok(());
    }
    Err(PrimitiveManifestError::Invalid(format!(
        "part {index} {field} contains a non-finite component"
    )))
}

fn vec3_from_array(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn default_schema_version() -> u32 {
    PRIMITIVE_MODEL_SCHEMA_VERSION
}

fn default_position() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

fn default_rotation() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_color() -> [u8; 4] {
    [180, 180, 180, 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_platform_manifest() {
        let manifest = PrimitiveModelManifest::builtin("platform")
            .expect("manifest parses")
            .expect("platform exists");

        assert_eq!(manifest.schema_version, PRIMITIVE_MODEL_SCHEMA_VERSION);
        assert_eq!(manifest.name, "Platform Prefab");
        assert!(manifest.parts.len() >= 2);
    }

    #[test]
    fn primitive_asset_manifest_imports_as_single_root() {
        let manifest = PrimitiveModelManifest::builtin_for_primitive(Primitive::Cube)
            .expect("manifest parses")
            .expect("cube exists");
        let mut scene = SceneGraph::new();

        let id = manifest
            .instantiate_single_root_into_scene(
                &mut scene,
                Some("Block"),
                Some("builtin://primitive/cube"),
            )
            .expect("single root import");

        let node = scene.get(id).expect("imported node");
        assert_eq!(node.name, "Block");
        assert_eq!(node.primitive, Primitive::Cube);
        assert_eq!(
            node.source_asset.as_deref(),
            Some("builtin://primitive/cube")
        );
        assert_eq!(
            node.source_schema_version,
            Some(PRIMITIVE_MODEL_SCHEMA_VERSION)
        );
        assert!(node.children.is_empty());
    }

    #[test]
    fn manifest_instantiates_folder_and_parts() {
        let manifest = PrimitiveModelManifest::from_json_str(
            r##"{
                "schema_version": 1,
                "name": "Test Model",
                "parts": [
                    {
                        "name": "Body",
                        "primitive": "cube",
                        "position": [1.0, 2.0, 3.0],
                        "scale": [2.0, 1.0, 3.0],
                        "color_rgba": [10, 20, 30, 255]
                    }
                ]
            }"##,
        )
        .expect("valid manifest");

        let mut scene = SceneGraph::new();
        let created = manifest.instantiate_into_scene_with_name(&mut scene, Some("Imported"));

        assert_eq!(created.len(), 2);
        let root = scene.get(created[0]).expect("root node");
        assert!(root.is_folder);
        assert_eq!(root.name, "Imported");
        assert_eq!(root.children, vec![created[1]]);

        let body = scene.get(created[1]).expect("body node");
        assert_eq!(body.primitive, Primitive::Cube);
        assert_eq!(body.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(body.scale, Vec3::new(2.0, 1.0, 3.0));
        assert_eq!(body.color, NodeColor::rgba(10, 20, 30, 255));
    }
}
