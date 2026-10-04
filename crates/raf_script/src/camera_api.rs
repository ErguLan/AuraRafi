//! Shared camera, reference and hierarchy operations for all script frontends.
use crate::view::ScriptRole;
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use raf_core::runtime_config::GameCamera;
use uuid::Uuid;

fn invalid(message: &str) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}

impl ScriptContext<'_> {
    pub fn entity(&self, reference: &str) -> Option<NodeHandle> {
        if let Ok(uuid) = Uuid::parse_str(reference) {
            let mut matches = self
                .scene
                .iter()
                .filter(|(id, n)| self.scene.is_valid_node(*id) && n.uuid == uuid);
            let (id, _) = matches.next()?;
            return matches
                .next()
                .is_none()
                .then(|| NodeHandle::scoped(id, self));
        }
        if !reference.contains('/') {
            return self.get_node(reference);
        }
        let mut current = None;
        for part in reference.trim_start_matches('/').split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return None;
            }
            current = match current {
                Some(parent) => self.find_child(parent, part).ok().flatten(),
                None => {
                    let mut matches = self.scene.roots().iter().copied().filter(|id| {
                        self.scene.is_valid_node(*id)
                            && self.scene.get(*id).is_some_and(|n| n.name == part)
                    });
                    let id = matches.next()?;
                    matches
                        .next()
                        .is_none()
                        .then(|| NodeHandle::scoped(id, self))
                }
            };
            current?;
        }
        current
    }
    pub fn active_camera(&self) -> Option<NodeHandle> {
        let handle = self.entity(&self.view.active_camera?.to_string())?;
        self.scene
            .get(handle.resolve(self).ok()?)?
            .game_camera
            .as_ref()?;
        Some(handle)
    }
    pub fn activate_camera(&mut self, handle: NodeHandle) -> ScriptResult<()> {
        if self.view.role == ScriptRole::Server {
            return Err(invalid("Server scripts cannot select a client camera"));
        }
        let id = handle.resolve(self)?;
        let node = self
            .scene
            .get(id)
            .ok_or_else(|| invalid("Camera entity missing"))?;
        node.game_camera
            .as_ref()
            .ok_or_else(|| invalid("Entity has no camera component"))?
            .validate()
            .map_err(invalid)?;
        self.view.active_camera = Some(node.uuid);
        Ok(())
    }
    pub fn clear_camera(&mut self) -> ScriptResult<()> {
        if self.view.role == ScriptRole::Server {
            return Err(invalid("Server scripts cannot select a client camera"));
        }
        self.view.active_camera = None;
        Ok(())
    }
}

impl NodeHandle {
    pub fn add_camera(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        ctx.scene
            .get_mut(id)
            .ok_or_else(|| invalid("Entity missing"))?
            .game_camera
            .get_or_insert_with(GameCamera::default);
        Ok(())
    }
    pub fn remove_camera(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| invalid("Entity missing"))?;
        if ctx.view.active_camera == Some(node.uuid) {
            ctx.view.active_camera = None;
        }
        node.game_camera = None;
        Ok(())
    }
    pub fn camera_lens(&self, ctx: &ScriptContext<'_>) -> ScriptResult<GameCamera> {
        ctx.scene
            .get(self.resolve(ctx)?)
            .and_then(|n| n.game_camera.clone())
            .ok_or_else(|| invalid("Entity has no camera component"))
    }
    pub fn set_camera_lens(
        &self,
        ctx: &mut ScriptContext<'_>,
        lens: GameCamera,
    ) -> ScriptResult<()> {
        lens.validate().map_err(invalid)?;
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| invalid("Entity missing"))?;
        if node.game_camera.is_none() {
            return Err(invalid("Entity has no camera component"));
        }
        node.game_camera = Some(lens);
        Ok(())
    }
    pub fn set_local_position(&self, ctx: &mut ScriptContext<'_>, value: Vec3) -> ScriptResult<()> {
        if !value.is_finite() {
            return Err(invalid("Position must be finite"));
        }
        let id = self.resolve(ctx)?;
        ctx.scene
            .get_mut(id)
            .ok_or_else(|| invalid("Entity missing"))?
            .position = value;
        Ok(())
    }
    pub fn set_world_rotation(
        &self,
        ctx: &mut ScriptContext<'_>,
        rotation: Quat,
    ) -> ScriptResult<()> {
        if !rotation.is_finite() || (rotation.length_squared() - 1.0).abs() > 1e-3 {
            return Err(invalid("Rotation must be finite and normalized"));
        }
        let id = self.resolve(ctx)?;
        let parent = ctx.scene.get(id).and_then(|n| n.parent);
        let parent_rotation = if let Some(parent) = parent {
            let matrix = ctx.scene.world_matrix(parent);
            if !matrix.is_finite() || matrix.determinant().abs() < 1e-8 {
                return Err(invalid("Parent transform is singular"));
            }
            let (scale, q, _) = matrix.to_scale_rotation_translation();
            let rebuilt = Mat4::from_scale_rotation_translation(scale, q, matrix.w_axis.truncate());
            if scale.cmple(Vec3::ZERO).any()
                || (scale.max_element() - scale.min_element()).abs() > 1e-4
            {
                return Err(invalid(
                    "World camera rotation requires uniform positive parent scale",
                ));
            }
            if !rebuilt.is_finite()
                || matrix
                    .to_cols_array()
                    .iter()
                    .zip(rebuilt.to_cols_array())
                    .any(|(a, b)| (*a - b).abs() > 1e-3)
            {
                return Err(invalid(
                    "World camera rotation does not support sheared parents",
                ));
            }
            q
        } else {
            Quat::IDENTITY
        };
        let local = (parent_rotation.inverse() * rotation).normalize();
        let (yaw, pitch, roll) = local.to_euler(EulerRot::YXZ);
        self.set_rotation(ctx, pitch, yaw, roll)
    }
    pub fn look_at(&self, ctx: &mut ScriptContext<'_>, target: Vec3, up: Vec3) -> ScriptResult<()> {
        if !target.is_finite() || !up.is_finite() {
            return Err(invalid("Look-at vectors must be finite"));
        }
        let position = Vec3::from_array(self.get_position(ctx)?);
        let forward = (target - position).normalize_or_zero();
        let right = forward.cross(up).normalize_or_zero();
        if forward.length_squared() < 0.5 || right.length_squared() < 0.5 {
            return Err(invalid(
                "Look-at target coincides with camera or up is parallel",
            ));
        }
        let corrected_up = right.cross(forward).normalize();
        self.set_world_rotation(
            ctx,
            Quat::from_mat3(&Mat3::from_cols(right, corrected_up, -forward)),
        )
    }
    pub fn reparent(
        &self,
        ctx: &mut ScriptContext<'_>,
        parent: Option<NodeHandle>,
        keep_world: bool,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let parent = parent.map(|h| h.resolve(ctx)).transpose()?;
        if keep_world {
            let old = ctx.scene.world_matrix(id);
            let matrix = parent
                .map(|p| ctx.scene.world_matrix(p))
                .unwrap_or(Mat4::IDENTITY);
            if !matrix.is_finite() || matrix.determinant().abs() < 1e-8 {
                return Err(invalid("Parent transform is singular"));
            }
            let local = matrix.inverse() * old;
            let (s, r, t) = local.to_scale_rotation_translation();
            let rebuilt = Mat4::from_scale_rotation_translation(s, r, t);
            if !rebuilt.is_finite()
                || local
                    .to_cols_array()
                    .iter()
                    .zip(rebuilt.to_cols_array())
                    .any(|(a, b)| (*a - b).abs() > 1e-3)
            {
                return Err(invalid(
                    "Keeping world transform would require unsupported shear",
                ));
            }
        }
        let ok = if keep_world {
            ctx.scene.reparent_node_preserve_world_transform(id, parent)
        } else {
            ctx.scene.reparent_node(id, parent)
        };
        if !ok {
            return Err(invalid("Invalid hierarchy target or cycle"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::ScriptEventQueue;
    use crate::{AudioCommandQueue, InputSnapshot, TimeInfo};
    use raf_core::scene::{Primitive, SceneGraph};
    fn context(scene: &mut SceneGraph, run: impl FnOnce(&mut ScriptContext<'_>)) {
        let mut view = crate::view::RuntimeViewState::default();
        let input = InputSnapshot::default();
        let mut audio = AudioCommandQueue::default();
        let mut events = ScriptEventQueue::default();
        run(&mut ScriptContext {
            scene,
            view: &mut view,
            input: &input,
            audio: &mut audio,
            events: &mut events,
            time: TimeInfo::default(),
            instance_id: 7,
            owner: None,
            entity_limit: 100,
            cancellation: None,
            deadline: None,
        });
    }
    #[test]
    fn parts_are_cameras_without_losing_geometry_and_activation_is_transient() {
        let mut scene = SceneGraph::new();
        let part = scene.add_root_with_primitive("Part", Primitive::Cube);
        let other = scene.add_root("Other");
        context(&mut scene, |ctx| {
            let a = ctx.entity("Part").unwrap();
            let b = ctx.entity("Other").unwrap();
            assert!(ctx.active_camera().is_none());
            assert!(ctx.activate_camera(a).is_err());
            a.add_camera(ctx).unwrap();
            b.add_camera(ctx).unwrap();
            ctx.activate_camera(a).unwrap();
            ctx.activate_camera(b).unwrap();
            assert_eq!(
                ctx.view.active_camera,
                Some(ctx.scene.get(other).unwrap().uuid)
            );
            assert_eq!(ctx.scene.get(part).unwrap().primitive, Primitive::Cube);
            b.remove_camera(ctx).unwrap();
            assert!(ctx.active_camera().is_none());
            ctx.view.role = ScriptRole::Server;
            assert!(ctx.activate_camera(a).is_err());
        });
    }
    #[test]
    fn stable_paths_parenting_and_look_at_preserve_world_coordinates() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("World");
        let camera = scene.add_child(root, "Camera");
        scene.get_mut(root).unwrap().position = Vec3::new(5.0, 1.0, 0.0);
        scene.get_mut(root).unwrap().rotation = Vec3::new(0.0, 35.0, 0.0);
        scene.get_mut(camera).unwrap().position = Vec3::new(0.0, 2.0, 8.0);
        context(&mut scene, |ctx| {
            let h = ctx.entity("/World/Camera").unwrap();
            assert_eq!(
                ctx.entity(&ctx.scene.get(camera).unwrap().uuid.to_string()),
                Some(h)
            );
            h.add_camera(ctx).unwrap();
            h.look_at(ctx, Vec3::ZERO, Vec3::Y).unwrap();
            let m = ctx.scene.world_matrix(camera);
            let p = m.transform_point3(Vec3::ZERO);
            assert!(
                m.transform_vector3(-Vec3::Z)
                    .normalize()
                    .dot((-p).normalize())
                    > 0.999
            );
            h.reparent(ctx, None, true).unwrap();
            assert!((Vec3::from_array(h.get_position(ctx).unwrap()) - p).length() < 1e-4);
            let parent = ctx.entity("World").unwrap();
            parent.reparent(ctx, Some(h), true).unwrap();
            assert!(h.reparent(ctx, Some(parent), false).is_err());
            ctx.scene.remove_node(camera);
            assert!(!h.is_valid(ctx));
        });
    }
    #[test]
    fn invalid_lens_and_sheared_parenting_do_not_partially_mutate() {
        let mut scene = SceneGraph::new();
        let camera = scene.add_root("Camera");
        let parent = scene.add_root("Parent");
        scene.get_mut(parent).unwrap().scale = Vec3::new(2.0, 1.0, 1.0);
        scene.get_mut(parent).unwrap().rotation = Vec3::new(0.0, 40.0, 0.0);
        context(&mut scene, |ctx| {
            let h = ctx.entity("Camera").unwrap();
            h.add_camera(ctx).unwrap();
            let mut lens = h.camera_lens(ctx).unwrap();
            lens.near = lens.far;
            assert!(h.set_camera_lens(ctx, lens).is_err());
            assert_eq!(h.camera_lens(ctx).unwrap(), GameCamera::default());
            let p = ctx.entity("Parent").unwrap();
            assert!(h.reparent(ctx, Some(p), true).is_err());
            assert_eq!(ctx.scene.get(camera).unwrap().parent, None);
        });
    }
}
