//! Static glTF/GLB import for the engine mesh pipeline.
//!
//! The importer deliberately produces plain CPU data with baked node
//! transforms. Skins, animations and morph targets are ignored: this is the
//! static geometry path consumed by `SceneNode::mesh_asset`.

use glam::{Mat3, Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::asset_paths;

pub const MAX_MODEL_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MODEL_TRIANGLES: usize = 500_000;
pub const MAX_MODEL_MATERIALS: usize = 64;
pub const MAX_MODEL_TEXTURES: usize = 32;
pub const MAX_MODEL_NODE_DEPTH: usize = 32;
pub const MAX_MODEL_VERTICES: usize = 2_000_000;
const MAX_MODEL_NODES: usize = 10_000;
const MAX_EXTERNAL_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

/// Axis-aligned bounds in model space after node transforms are baked.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Default for ModelBounds {
    fn default() -> Self {
        Self {
            min: [0.0; 3],
            max: [0.0; 3],
        }
    }
}

impl ModelBounds {
    pub fn dimensions(&self) -> [f32; 3] {
        [
            (self.max[0] - self.min[0]).max(0.0),
            (self.max[1] - self.min[1]).max(0.0),
            (self.max[2] - self.min[2]).max(0.0),
        ]
    }

    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

/// One triangle primitive ready for a renderer upload.
#[derive(Debug, Clone)]
pub struct ImportedMesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
    /// Source glTF node index that instanced this mesh.
    pub node: Option<usize>,
}

impl ImportedMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDependencyKind {
    Buffer,
    Texture,
}

/// External file referenced by a glTF package.
///
/// In an [`ImportedModel`] `path` is relative to the model file's directory. In
/// cached import metadata the same type stores a project-relative path below
/// `assets/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDependency {
    pub path: String,
    pub kind: ModelDependencyKind,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMaterial {
    pub name: String,
    pub base_color: [f32; 4],
    /// `opaque`, `mask` or `blend`.
    pub alpha_mode: String,
    pub double_sided: bool,
    /// Package-relative texture path for external images, or
    /// `embedded:image/<index>` for images stored inside the container.
    pub base_color_texture: Option<String>,
}

impl Default for ModelMaterial {
    fn default() -> Self {
        Self {
            name: "Material".to_string(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            alpha_mode: "opaque".to_string(),
            double_sided: false,
            base_color_texture: None,
        }
    }
}

/// Validated static model payload.
#[derive(Debug, Clone)]
pub struct ImportedModel {
    pub meshes: Vec<ImportedMesh>,
    pub bounds: ModelBounds,
    pub triangles: usize,
    pub materials: Vec<ModelMaterial>,
    pub dependencies: Vec<ModelDependency>,
    pub warnings: Vec<String>,
}

impl ImportedModel {
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|mesh| mesh.positions.len()).sum()
    }
}

/// Load and validate a static `.glb` or `.gltf` file.
///
/// External `.bin` and texture references are confined to the model file's
/// directory. Data URIs are supported for buffers; images with data URIs are
/// reported as embedded references without decoding them.
pub fn load_static_model(path: &Path) -> Result<ImportedModel, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or_else(|| "Static models require a .glb or .gltf file extension.".to_string())?;
    if extension != "glb" && extension != "gltf" {
        return Err(format!(
            "Unsupported static model extension '.{extension}'; expected .glb or .gltf."
        ));
    }
    let bytes = read_bounded(path, MAX_MODEL_FILE_BYTES)?;
    let gltf = gltf::Gltf::from_slice(&bytes)
        .map_err(|error| format!("Invalid glTF document '{}': {error}", path.display()))?;
    let document = &gltf.document;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    if document.materials().count() > MAX_MODEL_MATERIALS {
        return Err(format!(
            "Static model declares {} materials; the limit is {MAX_MODEL_MATERIALS}.",
            document.materials().count()
        ));
    }
    if document.textures().count() > MAX_MODEL_TEXTURES
        || document.images().count() > MAX_MODEL_TEXTURES
    {
        return Err(format!(
            "Static model declares more than {MAX_MODEL_TEXTURES} textures/images."
        ));
    }

    let mut warnings = Vec::new();
    let mut dependencies = Vec::new();
    let mut external_bytes = 0u64;

    let buffer_count = document.buffers().count();
    let mut buffers: Vec<Option<Vec<u8>>> = vec![None; buffer_count];
    let blob = gltf.blob.as_deref();
    for buffer in document.buffers() {
        if buffer.index() >= buffers.len() {
            return Err(format!(
                "Static model references invalid buffer index {}.",
                buffer.index()
            ));
        }
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => {
                let blob = blob.ok_or_else(|| {
                    "GLB buffer is missing its BIN chunk.".to_string()
                })?;
                blob.to_vec()
            }
            gltf::buffer::Source::Uri(uri) => {
                if uri.starts_with("data:") {
                    decode_data_uri(uri)?
                } else {
                    read_external_reference(
                        base_dir,
                        uri,
                        ModelDependencyKind::Buffer,
                        &mut external_bytes,
                        &mut dependencies,
                    )?
                }
            }
        };
        if data.len() < buffer.length() {
            return Err(format!(
                "Buffer {} is {} bytes but the glTF declares {} bytes.",
                buffer.index(),
                data.len(),
                buffer.length()
            ));
        }
        buffers[buffer.index()] = Some(data);
    }

    let images: Vec<_> = document.images().collect();
    let mut image_references = vec![None; images.len()];
    for image in &images {
        let reference = match image.source() {
            gltf::image::Source::View { .. } => {
                Some(format!("embedded:image/{}", image.index()))
            }
            gltf::image::Source::Uri { uri, .. } => {
                if uri.starts_with("data:") {
                    Some(format!("embedded:image/{}", image.index()))
                } else {
                    let normalized = normalize_package_uri(uri)?;
                    let resolved = asset_paths::resolve_confined(base_dir, &normalized)?;
                    let metadata = std::fs::metadata(&resolved).map_err(|error| {
                        format!(
                            "Cannot inspect texture '{}': {error}",
                            resolved.display()
                        )
                    })?;
                    if !metadata.is_file() {
                        return Err(format!(
                            "Texture reference '{}' is not a file.",
                            resolved.display()
                        ));
                    }
                    let size = metadata.len();
                    if size > MAX_MODEL_FILE_BYTES {
                        return Err(format!(
                            "Texture '{}' is {} bytes; the per-file limit is {}.",
                            resolved.display(),
                            size,
                            MAX_MODEL_FILE_BYTES
                        ));
                    }
                    external_bytes = external_bytes.saturating_add(size);
                    if external_bytes > MAX_EXTERNAL_TOTAL_BYTES {
                        return Err(
                            "Static model dependencies exceed the 512 MiB package limit."
                                .to_string(),
                        );
                    }
                    if !dependencies
                        .iter()
                        .any(|dependency| dependency.path == normalized)
                    {
                        dependencies.push(ModelDependency {
                            path: normalized.clone(),
                            kind: ModelDependencyKind::Texture,
                            bytes: size,
                        });
                    }
                    Some(normalized)
                }
            }
        };
        image_references[image.index()] = reference;
    }

    let mut materials = Vec::with_capacity(document.materials().count());
    for (index, material) in document.materials().enumerate() {
        let base_color_texture = material
            .pbr_metallic_roughness()
            .base_color_texture()
            .and_then(|info| {
                image_references
                    .get(info.texture().source().index())
                    .cloned()
                    .flatten()
            });
        materials.push(ModelMaterial {
            name: material
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("Material {index}")),
            base_color: material
                .pbr_metallic_roughness()
                .base_color_factor(),
            alpha_mode: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => "opaque",
                gltf::material::AlphaMode::Mask => "mask",
                gltf::material::AlphaMode::Blend => "blend",
            }
            .to_string(),
            double_sided: material.double_sided(),
            base_color_texture,
        });
    }

    let root_nodes: Vec<_> = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .map(|scene| scene.nodes().collect())
        .unwrap_or_else(|| document.nodes().collect());

    let mut meshes = Vec::new();
    let mut triangles = 0usize;
    let mut vertices = 0usize;
    let mut node_count = 0usize;
    for root in root_nodes {
        visit_node(
            root,
            Mat4::IDENTITY,
            0,
            &buffers,
            &mut meshes,
            &mut triangles,
            &mut vertices,
            &mut node_count,
            &mut warnings,
        )?;
    }

    if meshes.is_empty() || triangles == 0 {
        return Err(
            "Static model contains no triangle geometry. Skinned-only or animation-only files are not supported."
                .to_string(),
        );
    }

    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for mesh in &meshes {
        for position in &mesh.positions {
            let position = Vec3::from(*position);
            min = min.min(position);
            max = max.max(position);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return Err("Static model produced non-finite bounds.".to_string());
    }

    Ok(ImportedModel {
        meshes,
        bounds: ModelBounds {
            min: min.to_array(),
            max: max.to_array(),
        },
        triangles,
        materials,
        dependencies,
        warnings,
    })
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Cannot inspect model '{}': {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Model '{}' is a symlink; import the real file instead.",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "Model '{}' is not a regular file.",
            path.display()
        ));
    }
    if metadata.len() > limit {
        return Err(format!(
            "Model '{}' is {} bytes; the limit is {limit} bytes.",
            path.display(),
            metadata.len()
        ));
    }
    let file = File::open(path)
        .map_err(|error| format!("Cannot open model '{}': {error}", path.display()))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read model '{}': {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "Model '{}' exceeds the {limit} byte limit.",
            path.display()
        ));
    }
    Ok(bytes)
}

fn read_external_reference(
    base_dir: &Path,
    uri: &str,
    kind: ModelDependencyKind,
    external_bytes: &mut u64,
    dependencies: &mut Vec<ModelDependency>,
) -> Result<Vec<u8>, String> {
    if uri.starts_with("data:") {
        let data = decode_data_uri(uri)?;
        if data.len() as u64 > MAX_MODEL_FILE_BYTES {
            return Err(format!(
                "Embedded buffer is {} bytes; the per-file limit is {}.",
                data.len(),
                MAX_MODEL_FILE_BYTES
            ));
        }
        *external_bytes = external_bytes.saturating_add(data.len() as u64);
        if *external_bytes > MAX_EXTERNAL_TOTAL_BYTES {
            return Err(
                "Static model dependencies exceed the 512 MiB package limit.".to_string(),
            );
        }
        return Ok(data);
    }
    let normalized = normalize_package_uri(uri)?;
    let resolved = asset_paths::resolve_confined(base_dir, &normalized)?;
    let metadata = std::fs::metadata(&resolved)
        .map_err(|error| format!("Cannot inspect '{}': {error}", resolved.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "External reference '{}' is not a file.",
            resolved.display()
        ));
    }
    let size = metadata.len();
    if size > MAX_MODEL_FILE_BYTES {
        return Err(format!(
            "External file '{}' is {} bytes; the per-file limit is {}.",
            resolved.display(),
            size,
            MAX_MODEL_FILE_BYTES
        ));
    }
    *external_bytes = external_bytes.saturating_add(size);
    if *external_bytes > MAX_EXTERNAL_TOTAL_BYTES {
        return Err("Static model dependencies exceed the 512 MiB package limit.".to_string());
    }
    if !dependencies
        .iter()
        .any(|dependency| dependency.path == normalized)
    {
        dependencies.push(ModelDependency {
            path: normalized,
            kind,
            bytes: size,
        });
    }
    read_bounded(&resolved, MAX_MODEL_FILE_BYTES)
}

#[allow(clippy::too_many_arguments)]
fn visit_node(
    node: gltf::Node<'_>,
    parent: Mat4,
    depth: usize,
    buffers: &[Option<Vec<u8>>],
    meshes: &mut Vec<ImportedMesh>,
    triangles: &mut usize,
    vertices: &mut usize,
    node_count: &mut usize,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if depth > MAX_MODEL_NODE_DEPTH {
        return Err(format!(
            "Static model node hierarchy exceeds the {MAX_MODEL_NODE_DEPTH} level limit."
        ));
    }
    *node_count = node_count.saturating_add(1);
    if *node_count > MAX_MODEL_NODES {
        return Err(format!(
            "Static model declares more than {MAX_MODEL_NODES} nodes."
        ));
    }
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    if !local.is_finite() {
        return Err(format!(
            "Node {} has a non-finite transform.",
            node.index()
        ));
    }
    let world = parent * local;
    if let Some(mesh) = node.mesh() {
        import_mesh(
            &mesh,
            world,
            node.index(),
            buffers,
            meshes,
            triangles,
            vertices,
            warnings,
        )?;
    }
    for child in node.children() {
        visit_node(
            child,
            world,
            depth + 1,
            buffers,
            meshes,
            triangles,
            vertices,
            node_count,
            warnings,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn import_mesh(
    mesh: &gltf::Mesh<'_>,
    world: Mat4,
    node_index: usize,
    buffers: &[Option<Vec<u8>>],
    out: &mut Vec<ImportedMesh>,
    total_triangles: &mut usize,
    total_vertices: &mut usize,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let primitives: Vec<_> = mesh.primitives().collect();
    let primitive_count = primitives.len();
    let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
    let mesh_name = mesh
        .name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("Mesh {}", mesh.index()));

    for primitive in primitives {
        let reader = primitive.reader(|buffer| {
            buffers
                .get(buffer.index())
                .and_then(|data| data.as_deref())
        });
        let Some(raw_positions) = reader.read_positions() else {
            warnings.push(format!(
                "Skipped a primitive in '{mesh_name}' without POSITION data."
            ));
            continue;
        };
        let positions: Vec<[f32; 3]> = raw_positions
            .map(|position| {
                world
                    .transform_point3(Vec3::from(position))
                    .to_array()
            })
            .collect();
        if positions.is_empty() {
            continue;
        }
        if positions.len() > MAX_MODEL_VERTICES {
            return Err(format!(
                "Primitive in '{mesh_name}' has {} vertices; the limit is {MAX_MODEL_VERTICES}.",
                positions.len()
            ));
        }
        if positions
            .iter()
            .any(|position| !position.iter().all(|value| value.is_finite()))
        {
            return Err(format!(
                "Primitive in '{mesh_name}' contains non-finite vertex positions."
            ));
        }

        let mut indices: Vec<u32> = match reader.read_indices() {
            Some(indices) => indices.into_u32().collect(),
            None => (0..positions.len() as u32).collect(),
        };
        indices = match primitive.mode() {
            gltf::mesh::Mode::Triangles => indices,
            gltf::mesh::Mode::TriangleStrip => strip_to_triangles(&indices),
            gltf::mesh::Mode::TriangleFan => fan_to_triangles(&indices),
            other => {
                warnings.push(format!(
                    "Skipped non-triangle primitive ({other:?}) in '{mesh_name}'."
                ));
                continue;
            }
        };
        if indices.len() % 3 != 0 {
            indices.truncate(indices.len() / 3 * 3);
        }
        if indices.is_empty() {
            continue;
        }
        if let Some(invalid) = indices
            .iter()
            .copied()
            .find(|index| *index as usize >= positions.len())
        {
            return Err(format!(
                "Primitive in '{mesh_name}' references vertex {invalid} outside its {} vertices.",
                positions.len()
            ));
        }

        let provided_normals: Option<Vec<[f32; 3]>> =
            reader.read_normals().map(|normals| normals.collect());
        let normals = match provided_normals {
            Some(normals) if normals.len() == positions.len() => {
                let transformed: Vec<Vec3> = normals
                    .iter()
                    .map(|normal| {
                        let value = normal_matrix * Vec3::from(*normal);
                        if value.is_finite() && value.length_squared() > 1.0e-12 {
                            value.normalize()
                        } else {
                            Vec3::NAN
                        }
                    })
                    .collect();
                if transformed
                    .iter()
                    .all(|normal| normal.is_finite())
                {
                    transformed
                        .into_iter()
                        .map(|normal| normal.to_array())
                        .collect()
                } else {
                    compute_normals(&positions, &indices)
                }
            }
            _ => compute_normals(&positions, &indices),
        };

        let mut uvs: Vec<[f32; 2]> = reader
            .read_tex_coords(0)
            .map(|coords| coords.into_f32().collect())
            .unwrap_or_default();
        if uvs.len() != positions.len() {
            uvs = vec![[0.0, 0.0]; positions.len()];
        }
        if uvs
            .iter()
            .any(|uv| !uv.iter().all(|value| value.is_finite()))
        {
            uvs = vec![[0.0, 0.0]; positions.len()];
        }

        let triangle_count = indices.len() / 3;
        *total_triangles = total_triangles.saturating_add(triangle_count);
        if *total_triangles > MAX_MODEL_TRIANGLES {
            return Err(format!(
                "Static model exceeds the {MAX_MODEL_TRIANGLES} triangle limit."
            ));
        }
        *total_vertices = total_vertices.saturating_add(positions.len());
        if *total_vertices > MAX_MODEL_VERTICES {
            return Err(format!(
                "Static model exceeds the {MAX_MODEL_VERTICES} vertex limit."
            ));
        }

        let name = if primitive_count > 1 {
            format!("{mesh_name} {}", primitive.index())
        } else {
            mesh_name.clone()
        };
        out.push(ImportedMesh {
            name,
            positions,
            normals,
            uvs,
            indices,
            material: primitive.material().index(),
            node: Some(node_index),
        });
    }
    Ok(())
}

fn strip_to_triangles(indices: &[u32]) -> Vec<u32> {
    let mut triangles = Vec::with_capacity(indices.len().saturating_mul(3) / 2);
    for index in 2..indices.len() {
        if index % 2 == 0 {
            triangles.extend_from_slice(&[
                indices[index - 2],
                indices[index - 1],
                indices[index],
            ]);
        } else {
            triangles.extend_from_slice(&[
                indices[index - 1],
                indices[index - 2],
                indices[index],
            ]);
        }
    }
    triangles
}

fn fan_to_triangles(indices: &[u32]) -> Vec<u32> {
    let mut triangles = Vec::with_capacity(indices.len().saturating_mul(3));
    for index in 1..indices.len().saturating_sub(1) {
        triangles.extend_from_slice(&[indices[0], indices[index], indices[index + 1]]);
    }
    triangles
}

fn compute_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut accumulated = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let a = Vec3::from(positions[triangle[0] as usize]);
        let b = Vec3::from(positions[triangle[1] as usize]);
        let c = Vec3::from(positions[triangle[2] as usize]);
        let normal = (b - a).cross(c - a);
        if normal.is_finite() && normal.length_squared() > 1.0e-20 {
            accumulated[triangle[0] as usize] += normal;
            accumulated[triangle[1] as usize] += normal;
            accumulated[triangle[2] as usize] += normal;
        }
    }
    accumulated
        .into_iter()
        .map(|normal| {
            if normal.is_finite() && normal.length_squared() > 1.0e-20 {
                normal.normalize().to_array()
            } else {
                [0.0, 1.0, 0.0]
            }
        })
        .collect()
}

fn normalize_package_uri(uri: &str) -> Result<String, String> {
    if uri.contains(':') {
        return Err(format!(
            "External model reference '{uri}' uses an unsupported URI scheme. Only relative files are allowed."
        ));
    }
    let decoded = percent_decode(uri)?;
    asset_paths::normalize_relative(&decoded)
        .map_err(|error| format!("Invalid external model reference '{uri}': {error}"))
}

fn percent_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(format!(
                    "External model reference '{value}' has a truncated escape sequence."
                ));
            }
            let high = hex_value(bytes[index + 1]).ok_or_else(|| {
                format!("External model reference '{value}' has an invalid escape sequence.")
            })?;
            let low = hex_value(bytes[index + 2]).ok_or_else(|| {
                format!("External model reference '{value}' has an invalid escape sequence.")
            })?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output)
        .map_err(|_| format!("External model reference '{value}' is not valid UTF-8."))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn decode_data_uri(uri: &str) -> Result<Vec<u8>, String> {
    let payload = uri
        .strip_prefix("data:")
        .ok_or_else(|| "Malformed data URI.".to_string())?;
    let (header, payload) = payload
        .split_once(',')
        .ok_or_else(|| "Malformed data URI: missing payload separator.".to_string())?;
    if !header
        .rsplit(';')
        .next()
        .is_some_and(|value| value.eq_ignore_ascii_case("base64"))
    {
        return Err(
            "Only base64 data URIs are supported for embedded buffers.".to_string(),
        );
    }
    base64_decode(payload)
}

fn base64_decode(value: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(value.len().saturating_mul(3) / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let decoded = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => {
                return Err("Embedded buffer data URI contains invalid base64.".to_string())
            }
        };
        buffer = (buffer << 6) | u32::from(decoded);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
        }
    }
    if bits >= 6 {
        return Err("Embedded buffer data URI contains truncated base64.".to_string());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_strip_winding_is_preserved() {
        assert_eq!(
            strip_to_triangles(&[0, 1, 2, 3, 4]),
            vec![0, 1, 2, 2, 1, 3, 2, 3, 4]
        );
    }

    #[test]
    fn base64_payload_decodes() {
        assert_eq!(base64_decode("AAECAw==").unwrap(), vec![0, 1, 2, 3]);
    }
}
