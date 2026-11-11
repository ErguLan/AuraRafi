//! Presentation of game-owned instances. World simulation and editor UI stay separate.
use glam::{EulerRot, Mat4, Quat, Vec3};
use raf_core::{
    i18n::t, CaptureMode, InputOwner, InputRegionId, InputRouter, Language, PointerButton,
};
use raf_render::api_graphic_basic::{
    canvas_presenter::CanvasTargetRect,
    editor_compositor::EditorUiLayer,
    mesh::{BasicMesh, BasicVertex},
    pipeline::BasicPipelineKind,
    ui_surface::{
        DirectUiSurfaceHost, NativeUiInputBridge, NativeUiWindowHost, UiOffscreenTarget, UiSurface,
        UiSurfaceCpuRenderer,
    },
};
use raf_render::{
    camera::{Camera, CameraMode},
    scene_renderer::SceneRenderFrame,
    texture::CpuTexture,
};
use raf_runtime::RuntimeWorld;
use raf_ui::{
    game_ui::{UiMount, UiPresentationKind},
    UiAction, UiDispatchedAction, UiEnvironment, UiRect,
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
const GAME_UI_IMAGE_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

struct Instance {
    host: DirectUiSurfaceHost,
    mount: UiMount,
    revision: u64,
    generation: u64,
    target: Option<UiOffscreenTarget>,
    cpu_texture: Option<Arc<CpuTexture>>,
    dirty: bool,
    transform: Option<Mat4>,
    visible: bool,
    image_keys: std::collections::BTreeSet<String>,
    entity: Option<(raf_core::SceneNodeId, uuid::Uuid)>,
    resolve_revision: Option<u64>,
}
#[derive(Default)]
pub struct GameUiPresenter {
    instances: BTreeMap<String, Instance>,
    revision: u64,
    quad: Option<Arc<BasicMesh>>,
    cpu_renderer: Option<UiSurfaceCpuRenderer>,
    retired_owners: Vec<InputOwner>,
}
pub struct GameUiInput {
    pub actions: Vec<(String, UiDispatchedAction)>,
    pub pointer: bool,
    pub keyboard: bool,
}
impl GameUiPresenter {
    pub fn clear(&mut self) {
        self.instances.clear();
        self.cpu_renderer = None;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn has_active_motion(&self) -> bool {
        self.instances
            .values()
            .any(|i| i.visible && (i.host.has_active_motion() || i.host.has_active_text_repeat()))
    }
    pub fn sync(
        &mut self,
        native: &NativeUiWindowHost,
        world: &RuntimeWorld,
        root: &Path,
        theme: raf_core::config::Theme,
        environment: UiEnvironment,
    ) {
        let state = world.game_ui().instances();
        let image_budget = GAME_UI_IMAGE_BUDGET_BYTES / state.len().max(1) as u64;
        let before = self.instances.len();
        self.instances.retain(|id, _| {
            let keep = state.contains_key(id);
            if !keep {
                self.retired_owners.push(instance_owner(id));
            }
            keep
        });
        if before != self.instances.len() {
            self.revision = self.revision.wrapping_add(1);
        }
        let camera = world.presentation_camera_pose().map(camera_from_pose);
        for (id, source) in state {
            let mut changed = false;
            let item = self
                .instances
                .entry(id.clone())
                .or_insert_with(|| Instance {
                    host: native.graphics_context().create_ui_host(
                        UiSurface::from_document(&source.document, crate::session::palette(theme)),
                        [0; 4],
                    ),
                    mount: source.mount.clone(),
                    revision: 0,
                    generation: source.generation,
                    target: None,
                    cpu_texture: None,
                    dirty: true,
                    transform: None,
                    visible: false,
                    image_keys: Default::default(),
                    entity: None,
                    resolve_revision: None,
                });
            if item.host.images().budget_bytes() != image_budget {
                item.host.images_mut().set_budget_bytes(image_budget);
            }
            if item.revision != source.revision || item.generation != source.generation {
                if item.generation != source.generation {
                    item.host.session_mut().interaction = Default::default();
                }
                item.host.set_surface(UiSurface::from_document(
                    &source.document,
                    crate::session::palette(theme),
                ));
                let mut keys = std::collections::BTreeSet::new();
                raf_ui::game_ui::visit_ui_nodes(&source.document.root, &mut |n| {
                    if let raf_ui::UiControl::Image(image) = &n.control {
                        keys.insert(image.source.key.clone());
                    }
                });
                if keys != item.image_keys {
                    let window = native.window().clone();
                    item.host.images_mut().register_project_images_with_wakeup(
                        root,
                        &source.document.root,
                        Some(Arc::new(move || window.request_redraw())),
                    );
                    item.image_keys = keys;
                }
                if item.mount.canvas_size != source.mount.canvas_size
                    || item.mount.kind != source.mount.kind
                {
                    item.target = None;
                    item.cpu_texture = None;
                }
                if item.mount.target != source.mount.target {
                    item.entity = None;
                    item.resolve_revision = None;
                }
                item.mount = source.mount.clone();
                item.revision = source.revision;
                item.generation = source.generation;
                item.dirty = true;
                changed = true;
            }
            let mut env = environment;
            if !is_overlay(item.mount.kind) {
                env.viewport_size = item.mount.canvas_size.map(|v| v as f32);
                env.scale_factor = 1.0;
            }
            item.host.set_environment(env);
            if item.host.images_mut().pump_project_images(4) {
                item.dirty = true;
                changed = true;
            }
            if item.host.images().has_deferred_images() {
                native.window().request_redraw();
            }
            let old_transform = item.transform;
            let old_visible = item.visible;
            if item.entity.is_some_and(|(id, uuid)| {
                !world.scene.is_valid_node(id) || world.scene.get(id).is_none_or(|n| n.uuid != uuid)
            }) {
                item.entity = None;
            }
            if item.entity.is_none()
                && item.resolve_revision != Some(world.scene.document_revision())
            {
                item.entity = resolve_target(world, &item.mount.target);
                item.resolve_revision = Some(world.scene.document_revision());
            }
            item.transform = camera
                .as_ref()
                .and_then(|camera| mount_transform(world, &item.mount, camera, item.entity));
            item.visible = item.mount.visible
                && match item.mount.kind {
                    UiPresentationKind::Screen => true,
                    UiPresentationKind::CameraOverlay => camera_matches(world, item.entity),
                    _ => item.transform.is_some(),
                };
            if !is_overlay(item.mount.kind)
                && (changed || old_transform != item.transform || old_visible != item.visible)
            {
                self.revision = self.revision.wrapping_add(1);
            }
            if !is_overlay(item.mount.kind) && item.host.has_active_motion() {
                item.dirty = true;
                self.revision = self.revision.wrapping_add(1);
            }
            if !item.visible {
                item.host.session_mut().interaction = Default::default();
            }
        }
    }
    pub fn input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        world: &RuntimeWorld,
        rect: UiRect,
        scale: f32,
        language: Language,
        blocked: bool,
    ) -> GameUiInput {
        let mut output = GameUiInput {
            actions: Vec::new(),
            pointer: false,
            keyboard: false,
        };
        for owner in self.retired_owners.drain(..) {
            router.cancel_owner(owner);
        }
        for (id, item) in &self.instances {
            if !item.visible || !item.mount.interactive {
                router.cancel_owner(instance_owner(id));
            }
        }
        let camera = world.presentation_camera_pose().map(camera_from_pose);
        let ray = camera
            .as_ref()
            .and_then(|c| pointer_ray(c, rect, input.snapshot().pointer_position));
        let mut hit = ray.and_then(|(origin, direction)| {
            self.instances
                .iter()
                .filter(|(_, i)| i.visible && i.mount.interactive && i.transform.is_some())
                .filter_map(|(id, i)| {
                    plane_hit(i.transform?, origin, direction, i.mount.canvas_size, false)
                        .map(|(distance, point)| (id.clone(), distance, point))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
        });
        if let (Some((origin, direction)), Some((id, distance, _))) = (ray, hit.as_ref()) {
            let item = &self.instances[id];
            if !item.mount.always_on_top {
                let occluded = world.scene.iter().any(|(node_id, node)| {
                    world.scene.is_valid_node(node_id)
                        && node.visible
                        && !node.rigid_body.is_trigger
                        && Some(node_id) != item.entity.map(|(id, _)| id)
                        && raf_script::query_api::world_aabb(&world.scene, node_id).is_some_and(
                            |bounds| {
                                raf_script::query_api::ray_box(
                                    origin,
                                    direction,
                                    *distance - 0.001,
                                    bounds,
                                )
                                .is_some()
                            },
                        )
                });
                if occluded {
                    hit = None;
                }
            }
        }
        // Topmost overlays receive input first. Their passive root does not capture the world.
        let mut ids = self.instances.keys().cloned().collect::<Vec<_>>();
        ids.sort_by_key(|id| (is_overlay(self.instances[id].mount.kind), id.clone()));
        ids.reverse();
        for id in ids {
            let item = self.instances.get_mut(&id).unwrap();
            if !item.visible || !item.mount.interactive {
                continue;
            }
            let owner = instance_owner(&id);
            let before = item.host.session().interaction.focus.clone();
            let mut actions;
            if is_overlay(item.mount.kind) {
                let mut state = input.ui_state_for_owner(rect, owner, router);
                if blocked || output.pointer {
                    state.pointer_position = None;
                    state.pointer_pressed_buttons.clear();
                    state.pointer_down = false;
                    state.pointer_buttons_down.clear();
                }
                actions = item.host.process_input_at_scale(
                    [rect.width.max(1.0) as u32, rect.height.max(1.0) as u32],
                    scale,
                    |key| t(key, language),
                    &state,
                );
            } else {
                let mut state = input.ui_state_for_owner(rect, owner, router);
                let captured = item.host.has_pointer_capture();
                state.pointer_position = if blocked || output.pointer {
                    None
                } else if captured {
                    ray.and_then(|(origin, direction)| {
                        plane_hit(
                            item.transform?,
                            origin,
                            direction,
                            item.mount.canvas_size,
                            true,
                        )
                        .map(|(_, p)| p)
                    })
                } else {
                    hit.as_ref()
                        .filter(|(hit_id, _, _)| hit_id == &id)
                        .map(|(_, _, p)| *p)
                };
                if state.pointer_position.is_none() {
                    state.pointer_down = false;
                    state.pointer_buttons_down.clear();
                    state.pointer_pressed_buttons.clear();
                    state.scroll_delta = [0.0; 2];
                }
                state.pointer_delta = [0.0; 2];
                actions =
                    item.host
                        .process_input(item.mount.canvas_size, |key| t(key, language), &state);
            }
            for action in &actions {
                if let UiAction::SetClipboard { text } = &action.action {
                    input.write_clipboard(text);
                }
            }
            if item.host.has_pointer_capture()
                && input.snapshot().button_down(PointerButton::Primary)
            {
                let _ = router.try_capture_pointer(
                    PointerButton::Primary,
                    owner,
                    CaptureMode::PerButton,
                    input.snapshot().pointer_position.unwrap_or_default(),
                    input.snapshot().time_seconds,
                );
            }
            if item.host.captures_keyboard_input() {
                let _ = router.try_capture_keyboard(owner);
                output.keyboard = true;
            } else {
                let _ = router.release_keyboard(owner);
            }
            output.pointer |= item.host.has_interactive_hover() || item.host.has_pointer_capture();
            if before != item.host.session().interaction.focus
                || !actions.is_empty()
                || input.snapshot().scroll_delta != [0.0; 2] && item.host.has_interactive_hover()
            {
                item.dirty = true;
                self.revision = self.revision.wrapping_add(1);
            }
            output.actions.extend(
                actions
                    .drain(..)
                    .take(128usize.saturating_sub(output.actions.len()))
                    .map(|a| (id.clone(), a)),
            );
        }
        output
    }
    pub fn world_commands(
        &mut self,
        native: &NativeUiWindowHost,
        frame: &mut SceneRenderFrame,
        cpu: bool,
        language: Language,
    ) -> Result<(), String> {
        let quad = self
            .quad
            .get_or_insert_with(|| {
                Arc::new(BasicMesh::new(
                    [
                        ([-0.5, 0.5, 0.0], [0.0, 0.0]),
                        ([0.5, 0.5, 0.0], [1.0, 0.0]),
                        ([0.5, -0.5, 0.0], [1.0, 1.0]),
                        ([-0.5, -0.5, 0.0], [0.0, 1.0]),
                    ]
                    .into_iter()
                    .map(|(position, uv)| BasicVertex {
                        position: Vec3::from_array(position),
                        normal: Vec3::Z,
                        uv,
                    })
                    .collect(),
                    vec![0, 2, 1, 0, 3, 2],
                ))
            })
            .clone();
        let mut order = self
            .instances
            .iter()
            .filter(|(_, item)| item.visible && !is_overlay(item.mount.kind))
            .map(|(id, item)| {
                let depth = item.transform.map_or(0.0, |matrix| {
                    frame.view_proj.project_point3(matrix.w_axis.truncate()).z
                });
                (id.clone(), item.mount.always_on_top, depth)
            })
            .collect::<Vec<_>>();
        order.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.2.total_cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        for (id, _, _) in order {
            let item = self.instances.get_mut(&id).unwrap();
            if !item.visible || is_overlay(item.mount.kind) {
                continue;
            }
            let Some(transform) = item.transform else {
                continue;
            };
            let texture = if cpu {
                if item.dirty || item.host.has_active_motion() || item.cpu_texture.is_none() {
                    let renderer = self
                        .cpu_renderer
                        .get_or_insert_with(UiSurfaceCpuRenderer::new);
                    let data =
                        item.host
                            .rasterize_cpu_texture(item.mount.canvas_size, renderer, |key| {
                                t(key, language)
                            });
                    item.cpu_texture = Some(Arc::new(CpuTexture {
                        data,
                        width: item.mount.canvas_size[0],
                        height: item.mount.canvas_size[1],
                        source: format!("__game_ui/{id}/{}", self.revision).into(),
                    }));
                }
                frame
                    .commands
                    .register_texture(item.cpu_texture.as_ref().unwrap().clone())
            } else {
                if item.target.is_none() {
                    item.target = Some(native.create_ui_target(item.mount.canvas_size)?);
                    item.dirty = true;
                }
                let target = item.target.as_ref().unwrap();
                if item.dirty || item.host.has_active_motion() {
                    native.render_ui_target(&mut item.host, target, |key| t(key, language));
                }
                frame.commands.register_gpu_texture(target.view())
            };
            item.dirty = false;
            frame.commands.set_pipeline(if item.mount.always_on_top {
                BasicPipelineKind::UiSurfaceOverlay
            } else {
                BasicPipelineKind::UiSurface
            });
            frame.commands.set_shadow_receiver(false);
            let mesh = frame.commands.register_mesh(quad.clone());
            frame
                .commands
                .draw_mesh_with_texture(mesh, transform, [255; 4], Some(texture));
        }
        Ok(())
    }
    pub fn overlay_layers(
        &mut self,
        target: CanvasTargetRect,
        logical: [u32; 2],
        scale: f32,
    ) -> Vec<EditorUiLayer<'_>> {
        self.instances
            .values_mut()
            .filter(|i| i.visible && is_overlay(i.mount.kind))
            .map(|i| EditorUiLayer {
                host: &mut i.host,
                target_rect: target,
                logical_size: logical,
                raster_scale: scale.max(1.0),
            })
            .collect()
    }
}
fn instance_owner(id: &str) -> InputOwner {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    InputOwner::RetainedUi(InputRegionId(hash.finish()))
}
fn is_overlay(kind: UiPresentationKind) -> bool {
    matches!(
        kind,
        UiPresentationKind::Screen | UiPresentationKind::CameraOverlay
    )
}
fn resolve_target(world: &RuntimeWorld, key: &str) -> Option<(raf_core::SceneNodeId, uuid::Uuid)> {
    if key.is_empty() {
        return None;
    }
    let uuid = uuid::Uuid::parse_str(key).ok();
    let mut matches = world.scene.iter().filter(|(id, n)| {
        world.scene.is_valid_node(*id)
            && if let Some(uuid) = uuid {
                n.uuid == uuid
            } else {
                n.name == key
            }
    });
    let (id, node) = matches.next()?;
    matches.next().is_none().then_some((id, node.uuid))
}
fn camera_matches(
    world: &RuntimeWorld,
    entity: Option<(raf_core::SceneNodeId, uuid::Uuid)>,
) -> bool {
    entity.is_some_and(|(id, uuid)| {
        world.active_camera_uuid() == Some(uuid)
            && world.scene.get(id).is_some_and(|n| n.game_camera.is_some())
    })
}
pub fn camera_from_pose(pose: raf_core::runtime_config::RuntimeCameraPose) -> Camera {
    Camera {
        position: pose.position,
        target: pose.position + pose.forward,
        up: pose.up,
        mode: if pose.projection.orthographic {
            CameraMode::Orthographic
        } else {
            CameraMode::Perspective
        },
        fov: pose.projection.fov_degrees,
        near: pose.projection.near,
        far: pose.projection.far,
        ortho_scale: pose.projection.ortho_scale,
    }
}
fn mount_transform(
    world: &RuntimeWorld,
    mount: &UiMount,
    camera: &Camera,
    entity: Option<(raf_core::SceneNodeId, uuid::Uuid)>,
) -> Option<Mat4> {
    if is_overlay(mount.kind) {
        return None;
    }
    let local = Mat4::from_scale_rotation_translation(
        Vec3::new(mount.size_meters[0], mount.size_meters[1], 1.0),
        Quat::from_euler(
            EulerRot::XYZ,
            mount.rotation_degrees[0].to_radians(),
            mount.rotation_degrees[1].to_radians(),
            mount.rotation_degrees[2].to_radians(),
        ),
        Vec3::from_array(mount.offset),
    );
    let matrix = match mount.kind {
        UiPresentationKind::CameraPlane => {
            if !camera_matches(world, entity) {
                return None;
            }
            camera.view_matrix().inverse() * local
        }
        UiPresentationKind::Surface => world.scene.world_matrix(entity?.0) * local,
        UiPresentationKind::Billboard => {
            let position = world
                .scene
                .world_matrix(entity?.0)
                .transform_point3(Vec3::from_array(mount.offset));
            let mut facing = camera.view_matrix().inverse();
            facing.w_axis = position.extend(1.0);
            facing
                * Mat4::from_scale_rotation_translation(
                    Vec3::new(mount.size_meters[0], mount.size_meters[1], 1.0),
                    Quat::from_euler(
                        EulerRot::XYZ,
                        mount.rotation_degrees[0].to_radians(),
                        mount.rotation_degrees[1].to_radians(),
                        mount.rotation_degrees[2].to_radians(),
                    ),
                    Vec3::ZERO,
                )
        }
        _ => return None,
    };
    let position = matrix.transform_point3(Vec3::ZERO);
    (matrix.is_finite()
        && matrix.determinant().abs() > 1e-9
        && position.distance_squared(camera.position) <= mount.max_distance * mount.max_distance)
        .then_some(matrix)
}
fn pointer_ray(camera: &Camera, rect: UiRect, pointer: Option<[f32; 2]>) -> Option<(Vec3, Vec3)> {
    let p = pointer?;
    if !rect.contains(p) {
        return None;
    }
    let inverse = camera.view_projection(rect.width, rect.height).inverse();
    let x = 2.0 * (p[0] - rect.x) / rect.width - 1.0;
    let y = 1.0 - 2.0 * (p[1] - rect.y) / rect.height;
    let near = inverse.project_point3(Vec3::new(x, y, 0.0));
    let far = inverse.project_point3(Vec3::new(x, y, 1.0));
    Some((near, (far - near).try_normalize()?))
}
fn plane_hit(
    matrix: Mat4,
    origin: Vec3,
    direction: Vec3,
    size: [u32; 2],
    captured: bool,
) -> Option<(f32, [f32; 2])> {
    let inverse = matrix.inverse();
    let local_origin = inverse.transform_point3(origin);
    let local_direction = inverse.transform_vector3(direction);
    if local_direction.z.abs() < 1e-7 {
        return None;
    }
    let t = -local_origin.z / local_direction.z;
    if t < 0.0 {
        return None;
    }
    let p = local_origin + local_direction * t;
    if !captured && (p.x.abs() > 0.5 || p.y.abs() > 0.5) {
        return None;
    }
    Some((
        t,
        [(p.x + 0.5) * size[0] as f32, (0.5 - p.y) * size[1] as f32],
    ))
}
