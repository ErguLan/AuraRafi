//! Staged asset placement. Preparation/preview are isolated; commit is one history edit.
use glam::{Mat4, Vec3};
use raf_core::scene::{
    geometry::subtree_geometry_bounds, prefab::ScenePrefab, LinkedAssembly, SceneGraph, SceneNodeId,
};
use raf_render::api_graphic_basic::command_list::GraphicCommand;
use raf_render::{
    camera::{Camera, CameraMode},
    scene_renderer::{RenderOptions, SceneRenderFrame, SceneRenderer},
};
use std::path::{Component, Path};

/// Source metadata stamped on an Assembly instance root at commit time.
#[derive(Debug, Clone)]
struct AssemblyStamp {
    source: String,
    version: u32,
    source_hash: String,
}

pub struct AssetPlacement {
    prefab: ScenePrefab,
    parent: Option<SceneNodeId>,
    min: Vec3,
    max: Vec3,
    template: SceneRenderFrame,
    assembly: Option<AssemblyStamp>,
    pub offset: Vec3,
    pub valid: bool,
    pub pressed: bool,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoring_asset_placement_is_staged_and_preserves_world_under_parent() {
        let mut scene = SceneGraph::new();
        let parent = scene.add_root("Parent");
        scene.get_mut(parent).unwrap().scale = Vec3::splat(2.0);
        scene.get_mut(parent).unwrap().rotation.y = 45.0;
        let before = scene.render_fingerprint();
        let mut placement = AssetPlacement::prepare(
            "builtin://primitive/capsule",
            Path::new("unused"),
            Some(parent),
            false,
        )
        .unwrap();
        placement.support_at(Vec3::new(3.0, 0.0, 4.0), Vec3::Y);
        assert_eq!(
            scene.render_fingerprint(),
            before,
            "preview must not edit the document"
        );
        let id = placement.commit(&mut scene).unwrap();
        let (min, max) = subtree_geometry_bounds(&scene, id).unwrap();
        assert!(min.y.abs() < 1e-5);
        assert!((max.y - 2.0).abs() < 1e-5);
        assert_eq!(scene.get(id).unwrap().parent, Some(parent));
        let original = scene.clone();
        let cancel =
            AssetPlacement::prepare("builtin://primitive/cube", Path::new("unused"), None, true)
                .unwrap();
        drop(cancel);
        assert_eq!(scene.render_fingerprint(), original.render_fingerprint());
    }

    #[test]
    fn authoring_assembly_placement_stamps_source_version_and_hash() {
        use raf_core::scene::Primitive;
        use std::path::PathBuf;

        struct TempProject(PathBuf);
        impl Drop for TempProject {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let project = TempProject(
            std::env::temp_dir().join(format!("raf-placement-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(project.0.join("assets/parts")).unwrap();
        let mut source = SceneGraph::new();
        let source_root = source.add_root_with_primitive("Rig", Primitive::Cube);
        source.add_child_with_primitive(source_root, "Bolt", Primitive::Sphere);
        let prefab = ScenePrefab::capture(&source, source_root).unwrap();
        std::fs::write(
            project.0.join("assets/parts/rig.prefab.ron"),
            ron::ser::to_string_pretty(&prefab, ron::ser::PrettyConfig::default()).unwrap(),
        )
        .unwrap();

        let mut placement =
            AssetPlacement::prepare("parts/rig.prefab.ron", &project.0, None, false).unwrap();
        placement.support_at(Vec3::ZERO, Vec3::Y);
        let mut scene = SceneGraph::new();
        let id = placement.commit(&mut scene).unwrap();
        let node = scene.get(id).unwrap();
        let link = node
            .linked_assembly
            .as_ref()
            .expect("assembly placement must link its source");
        assert_eq!(link.source, "parts/rig.prefab.ron");
        assert_eq!(link.version, 1);
        assert!(link
            .source_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("fnv1a64:")));
        assert_eq!(node.source_asset.as_deref(), Some("parts/rig.prefab.ron"));
        assert_eq!(node.source_schema_version, Some(1));
    }

    #[test]
    fn plain_gltf_without_import_sidecar_can_be_placed() {
        use std::path::PathBuf;

        struct TempProject(PathBuf);
        impl Drop for TempProject {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        const PYRAMID: &str = r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"data:application/octet-stream;base64,AAAAvwAAAL8AAAC/AAAAPwAAAL8AAAC/AAAAPwAAAL8AAAA/AAAAvwAAAL8AAAA/AAAAADMzMz8AAAAAAAABAAIAAAACAAMAAAABAAQAAQACAAQAAgADAAQAAwAAAAQA","byteLength":96}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":60,"target":34962},{"buffer":0,"byteOffset":60,"byteLength":36,"target":34963}],"accessors":[{"bufferView":0,"componentType":5126,"count":5,"type":"VEC3","min":[-0.5,-0.5,-0.5],"max":[0.5,0.7,0.5]},{"bufferView":1,"componentType":5123,"count":18,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"mode":4}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;

        let project = TempProject(
            std::env::temp_dir().join(format!("raf-model-placement-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(project.0.join("assets/models")).unwrap();
        std::fs::write(project.0.join("assets/models/pyramid.gltf"), PYRAMID).unwrap();

        let mut placement = AssetPlacement::prepare("models/pyramid.gltf", &project.0, None, false)
            .expect("a plain glTF is placeable without an import sidecar");
        placement.support_at(Vec3::ZERO, Vec3::Y);
        let mut scene = SceneGraph::new();
        let id = placement.commit(&mut scene).expect("model commit");
        let node = scene.get(id).expect("placed model");
        let mesh = node.mesh_asset.as_ref().expect("mesh asset reference");
        assert_eq!(mesh.path, "models/pyramid.gltf");
        assert!(mesh.content_hash.is_none(), "no sidecar hash is fabricated");
    }
}
impl AssetPlacement {
    pub fn prepare(
        path: &str,
        project: &Path,
        parent: Option<SceneNodeId>,
        drag: bool,
    ) -> Result<Self, String> {
        use raf_assets::authoring_catalog::{AssetClass, AuthoringAsset};
        let entry = AuthoringAsset::from_path(path);
        let mut temporary = SceneGraph::new();
        let mut assembly = None;
        let root = match entry.class {
            AssetClass::Shape(shape) => temporary.add_root_with_primitive(shape.label(), shape),
            AssetClass::Prefab => {
                if Path::new(path)
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                {
                    return Err("assets.invalid_path".into());
                }
                let assets_root = project.join("assets");
                let source = raf_assets::asset_paths::normalize_reference(path)
                    .map_err(|_| "assets.invalid_path")?;
                if std::fs::metadata(assets_root.join(&source))
                    .map_err(|_| "assets.missing_file")?
                    .len()
                    > 8 * 1024 * 1024
                {
                    return Err("assets.preview_budget".into());
                }
                let loaded = crate::commands::behavior::load_assembly(&assets_root, &source)
                    .map_err(|_| "assets.invalid_prefab")?;
                let root = loaded
                    .prefab
                    .instantiate(&mut temporary, None)
                    .map_err(|_| "assets.invalid_prefab")?;
                assembly = Some(AssemblyStamp {
                    source: loaded.source,
                    version: loaded.prefab.version,
                    source_hash: loaded.source_hash,
                });
                root
            }
            AssetClass::Model => {
                if Path::new(path)
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                {
                    return Err("assets.invalid_path".into());
                }
                let normalized = raf_assets::asset_paths::normalize_reference(path)
                    .map_err(|_| "assets.invalid_path")?;
                if !matches!(
                    Path::new(&normalized)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("glb") | Some("gltf")
                ) {
                    return Err("assets.model_import_pending".into());
                }
                let absolute = project.join("assets").join(&normalized);
                if !absolute.is_file() {
                    return Err("assets.missing_file".into());
                }
                // Imported packages carry a sidecar with bounds/hash, but a
                // plain glTF copied into the project is still a valid model.
                // Never block placement just because the optional sidecar is
                // absent; the renderer decodes the file itself.
                let metadata =
                    raf_assets::importer::ModelImportMetadata::read_for_asset_file(&absolute);
                let root = temporary.add_root(
                    Path::new(&normalized)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("Model"),
                );
                if let Some(node) = temporary.get_mut(root) {
                    let mut reference = raf_core::scene::MeshAssetRef::new(normalized.clone());
                    if let Some(metadata) = metadata.as_ref() {
                        reference.bounds = Some([
                            metadata.bounds.min[0],
                            metadata.bounds.min[1],
                            metadata.bounds.min[2],
                            metadata.bounds.max[0],
                            metadata.bounds.max[1],
                            metadata.bounds.max[2],
                        ]);
                        reference.content_hash = Some(metadata.content_hash.clone());
                        reference.import_scale = metadata.import_scale;
                    }
                    node.mesh_asset = Some(reference);
                    node.source_asset = Some(normalized);
                }
                root
            }
            _ => return Err("assets.model_import_pending".into()),
        };
        let (min, max) =
            subtree_geometry_bounds(&temporary, root).ok_or("assets.preview_unavailable")?;
        let prefab = ScenePrefab::capture(&temporary, root).map_err(|_| "assets.invalid_prefab")?;
        let radius = (max - min).length().max(0.5);
        let center = (min + max) * 0.5;
        let camera = Camera {
            position: center + Vec3::new(1.0, 1.0, 1.0) * radius * 2.0,
            target: center,
            mode: CameraMode::Orthographic,
            ortho_scale: radius * 2.5,
            near: 0.01,
            far: radius * 12.0,
            ..Default::default()
        };
        let mut preview_renderer = SceneRenderer::new(128, 128);
        preview_renderer.set_asset_root(Some(project.join("assets")));
        let template = preview_renderer.build_frame(
            &temporary,
            &camera,
            128.0,
            128.0,
            &[],
            [0; 4],
            Vec3::Y,
            RenderOptions {
                show_grid_3d: false,
                selection_outline: false,
                ..Default::default()
            },
            None,
        );
        let source = assembly
            .as_ref()
            .map(|assembly| assembly.source.clone())
            .unwrap_or_else(|| path.into());
        Ok(Self {
            prefab,
            parent,
            min,
            max,
            template,
            assembly,
            offset: Vec3::ZERO,
            valid: false,
            pressed: drag,
            source,
        })
    }
    pub fn support_at(&mut self, point: Vec3, normal: Vec3) {
        let center = (self.min + self.max) * 0.5;
        let support = (self.max - self.min) * 0.5;
        let distance = support.dot(normal.abs());
        self.offset = point + normal * distance - center;
        self.valid = self.offset.is_finite();
    }
    pub fn append(&self, frame: &mut SceneRenderFrame) {
        if !self.valid {
            return;
        }
        frame
            .commands
            .set_pipeline(raf_render::api_graphic_basic::pipeline::BasicPipelineKind::FlatColor);
        let shift = Mat4::from_translation(self.offset);
        for command in self.template.commands.commands() {
            let draw = |mesh_id: usize, transform: Mat4, frame: &mut SceneRenderFrame| {
                if let Some(mesh) = self.template.commands.mesh_arc(mesh_id) {
                    let id = frame.commands.register_mesh(mesh.clone());
                    frame
                        .commands
                        .draw_mesh(id, shift * transform, [246, 159, 54, 150]);
                }
            };
            match command {
                GraphicCommand::DrawMesh {
                    mesh_id, transform, ..
                } => draw(*mesh_id, *transform, frame),
                GraphicCommand::DrawMeshBatch {
                    mesh_id, instances, ..
                } => {
                    for instance in instances {
                        draw(*mesh_id, instance.transform, frame)
                    }
                }
                _ => {}
            }
        }
    }
    pub fn commit(self, scene: &mut SceneGraph) -> Result<SceneNodeId, String> {
        if !self.valid {
            return Err("assets.placement_cancelled".into());
        }
        if self
            .parent
            .is_some_and(|id| !scene.is_valid_node(id) || scene.get(id).is_some_and(|n| n.locked))
        {
            return Err("assets.invalid_parent".into());
        }
        let source = self.source.clone();
        let assembly = self.assembly.clone();
        let id = self
            .prefab
            .instantiate(scene, None)
            .map_err(|_| "assets.invalid_prefab")?;
        if let Some(node) = scene.get_mut(id) {
            node.position += self.offset;
            node.source_asset = Some(source);
            if let Some(assembly) = assembly {
                node.linked_assembly = Some(LinkedAssembly {
                    source: assembly.source,
                    version: assembly.version,
                    source_hash: Some(assembly.source_hash),
                });
                node.source_schema_version = Some(assembly.version);
            }
        }
        if let Some(parent) = self.parent {
            if !scene.reparent_node_preserve_world_transform(id, Some(parent)) {
                return Err("assets.invalid_parent".into());
            }
        }
        Ok(id)
    }
}
