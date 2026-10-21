//! Staged asset placement. Preparation/preview are isolated; commit is one history edit.
use glam::{Mat4, Vec3};
use raf_core::scene::{
    geometry::subtree_geometry_bounds, prefab::ScenePrefab, SceneGraph, SceneNodeId,
};
use raf_render::api_graphic_basic::command_list::GraphicCommand;
use raf_render::{
    camera::{Camera, CameraMode},
    scene_renderer::{RenderOptions, SceneRenderFrame, SceneRenderer},
};
use std::path::{Component, Path};

pub struct AssetPlacement {
    prefab: ScenePrefab,
    parent: Option<SceneNodeId>,
    min: Vec3,
    max: Vec3,
    template: SceneRenderFrame,
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
        let root = match entry.class {
            AssetClass::Shape(shape) => temporary.add_root_with_primitive(shape.label(), shape),
            AssetClass::Prefab => {
                if Path::new(path)
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                {
                    return Err("assets.invalid_path".into());
                }
                let absolute = project.join("assets").join(path);
                if std::fs::metadata(&absolute)
                    .map_err(|_| "assets.missing_file")?
                    .len()
                    > 2 * 1024 * 1024
                {
                    return Err("assets.preview_budget".into());
                }
                let prefab: ScenePrefab = ron::from_str(
                    &std::fs::read_to_string(absolute).map_err(|_| "assets.missing_file")?,
                )
                .map_err(|_| "assets.invalid_prefab")?;
                prefab
                    .instantiate(&mut temporary, None)
                    .map_err(|_| "assets.invalid_prefab")?
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
        let template = SceneRenderer::new(128, 128).build_frame(
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
        Ok(Self {
            prefab,
            parent,
            min,
            max,
            template,
            offset: Vec3::ZERO,
            valid: false,
            pressed: drag,
            source: path.into(),
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
                GraphicCommand::DrawMeshBatch { mesh_id, instances } => {
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
        let id = self
            .prefab
            .instantiate(scene, None)
            .map_err(|_| "assets.invalid_prefab")?;
        if let Some(node) = scene.get_mut(id) {
            node.position += self.offset;
            node.source_asset = Some(self.source);
        }
        if let Some(parent) = self.parent {
            if !scene.reparent_node_preserve_world_transform(id, Some(parent)) {
                return Err("assets.invalid_parent".into());
            }
        }
        Ok(id)
    }
}
