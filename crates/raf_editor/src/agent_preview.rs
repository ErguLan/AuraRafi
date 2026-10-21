//! Demand-only, read-only scene preview. No editor, tools, runtime or scene copy.
use glam::Vec3;
use raf_core::session::SessionId;
use raf_core::{
    CaptureMode, EngineSettings, InputKey, InputOwner, InputRegionId, InputRouter, InputSnapshot,
    PointerButton, Project, SceneGraph,
};
use raf_render::api_graphic_basic::device::SharedGraphicsContext;
use raf_render::api_graphic_basic::{CanvasTargetRect, EditorCanvasLayer};
use raf_render::{
    bridge::{GraphicsSurfaceKind, RenderRuntime},
    camera::Camera,
    scene_renderer::{RenderOptions, SceneRenderer},
};
use raf_ui::UiRect;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) const PREVIEW_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreviewTarget {
    pub root: PathBuf,
    pub document: SessionId,
}

struct PreviewResources {
    renderer: SceneRenderer,
    graphics: RenderRuntime,
    output: Option<EditorCanvasLayer>,
    key: Option<(
        u64,
        u64,
        [u32; 2],
        bool,
        raf_core::config::RenderExecutionPolicy,
    )>,
}

#[cfg(test)]
#[path = "agent_preview_tests.rs"]
mod tests;

pub(crate) struct AgentPreview {
    pub target: Option<PreviewTarget>,
    pub visible: bool,
    resources: Option<PreviewResources>,
    hidden_at: Option<Instant>,
    camera: Camera,
    yaw: f32,
    pitch: f32,
    distance: f32,
    camera_revision: u64,
    fit_requested: bool,
    pub navigating: bool,
    pub moving: bool,
    pub rendered_frames: u64,
}
impl Default for AgentPreview {
    fn default() -> Self {
        Self {
            target: None,
            visible: false,
            resources: None,
            hidden_at: None,
            camera: Camera::default(),
            yaw: 0.785,
            pitch: 0.5,
            distance: 8.0,
            camera_revision: 0,
            fit_requested: true,
            navigating: false,
            moving: false,
            rendered_frames: 0,
        }
    }
}
impl AgentPreview {
    pub fn owner() -> InputOwner {
        InputOwner::Custom(InputRegionId::from_static("agent.workspace.preview.camera"))
    }
    pub fn resident(&self) -> bool {
        self.resources.is_some()
    }
    /// Returns true when the compositor must release its retained source too.
    pub fn sync(&mut self, target: Option<PreviewTarget>, visible: bool, now: Instant) -> bool {
        let mut released = false;
        if self.target != target {
            released = self.resident();
            self.resources = None;
            self.camera = Camera::default();
            self.yaw = 0.785;
            self.pitch = 0.5;
            self.distance = 8.0;
            self.fit_requested = true;
            self.target = target;
        }
        let visible = visible && self.target.is_some();
        if !visible && self.visible {
            self.hidden_at = Some(now);
            self.navigating = false;
            self.moving = false;
        }
        if visible {
            self.hidden_at = None;
        }
        self.visible = visible;
        released | self.expire(now)
    }
    pub fn expire(&mut self, now: Instant) -> bool {
        if !self.visible
            && self
                .hidden_at
                .is_some_and(|at| now.saturating_duration_since(at) >= PREVIEW_TTL)
        {
            self.hidden_at = None;
            return self.resources.take().is_some();
        }
        false
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.hidden_at.map(|at| at + PREVIEW_TTL)
    }
    pub fn close(&mut self) -> bool {
        let resident = self.resident();
        *self = Self::default();
        resident
    }
    pub fn cancel_input(&mut self, router: &mut InputRouter) {
        self.navigating = false;
        self.moving = false;
        router.cancel_owner(Self::owner());
    }
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
        self.camera_revision = self.camera_revision.wrapping_add(1);
    }
    fn update_camera(&mut self) {
        self.camera.position = self.camera.target
            + Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.cos() * self.pitch.cos(),
            ) * self.distance;
        self.camera_revision = self.camera_revision.wrapping_add(1);
    }
    pub fn input(&mut self, input: &InputSnapshot, router: &mut InputRouter, rect: UiRect) -> bool {
        self.moving = false;
        if !self.visible || !input.window_focused || input.key_pressed(InputKey::Escape) {
            self.cancel_input(router);
            return false;
        }
        let hovered = input.pointer_position.is_some_and(|p| rect.contains(p));
        if hovered && input.button_pressed(PointerButton::Secondary) {
            if router.try_capture_pointer(
                PointerButton::Secondary,
                Self::owner(),
                CaptureMode::Exclusive,
                input.pointer_position.unwrap(),
                input.time_seconds,
            ) {
                self.navigating = true;
                router.try_capture_keyboard(Self::owner());
            }
        }
        let mut changed = false;
        if hovered && input.scroll_delta[1] != 0.0 && !router.has_exclusive_pointer_capture() {
            self.distance =
                (self.distance * (-input.scroll_delta[1] * 0.002).exp()).clamp(0.1, 100_000.0);
            changed = true;
        }
        if self.navigating {
            if !input.button_down(PointerButton::Secondary)
                || !router.is_pointer_owned_by(PointerButton::Secondary, Self::owner())
            {
                self.cancel_input(router);
            } else {
                self.yaw -= input.pointer_delta[0] * 0.005;
                self.pitch = (self.pitch + input.pointer_delta[1] * 0.005).clamp(-1.45, 1.45);
                changed |= input.pointer_delta != [0.0, 0.0];
                if router.keyboard_owner() == Some(Self::owner()) {
                    let axis = |positive, negative| {
                        (input.key_down(positive) as i32 - input.key_down(negative) as i32) as f32
                    };
                    let forward = Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
                    let right = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
                    let movement = forward * axis(InputKey::W, InputKey::S)
                        + right * axis(InputKey::D, InputKey::A)
                        + Vec3::Y * axis(InputKey::E, InputKey::Q);
                    self.moving = movement.length_squared() > 0.0;
                    if self.moving {
                        self.camera.target += movement.normalize()
                            * self.distance.max(2.0)
                            * input.delta_seconds.clamp(0.001, 0.05);
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.update_camera();
        }
        changed
    }
    pub fn scene_changed(&self, scene: &SceneGraph) -> bool {
        self.visible
            && self.resources.as_ref().is_none_or(|r| {
                r.key
                    .as_ref()
                    .is_none_or(|key| key.0 != scene.render_fingerprint())
            })
    }
    pub fn render_layer(
        &mut self,
        scene: &SceneGraph,
        project: &Project,
        settings: &EngineSettings,
        target_rect: CanvasTargetRect,
        context: impl FnOnce() -> Option<SharedGraphicsContext>,
    ) -> Option<EditorCanvasLayer> {
        if !self.visible {
            return None;
        }
        if self.resources.is_none() {
            let mut renderer = SceneRenderer::new(1, 1);
            renderer.set_asset_root(Some(project.path.join("assets")));
            let mut graphics = RenderRuntime::default();
            graphics.configure(settings.render_execution_policy, false);
            graphics.set_memory_budget(raf_render::api_graphic_basic::GraphicsMemoryBudget {
                gpu_bytes: 64 * 1024 * 1024,
                mesh_cache_entries: 512,
                texture_cache_entries: 64,
                staging_bytes: 4 * 1024 * 1024,
                frame_upload_bytes: 2 * 1024 * 1024,
                ..raf_render::api_graphic_basic::GraphicsMemoryBudget::potato()
            });
            graphics.set_shared_graphics_context(context());
            graphics.activate_surface(GraphicsSurfaceKind::SceneViewport);
            self.resources = Some(PreviewResources {
                renderer,
                graphics,
                output: None,
                key: None,
            });
        }
        if self.fit_requested {
            let mut bounds: Option<(Vec3, Vec3)> = None;
            for id in scene.all_live_ids() {
                if let Some((min, max)) = raf_core::scene::geometry::node_geometry_bounds(scene, id)
                {
                    bounds = Some(bounds.map_or((min, max), |(a, b)| (a.min(min), b.max(max))));
                }
            }
            if let Some((min, max)) = bounds {
                self.camera.target = (min + max) * 0.5;
                self.distance = (max - min).length().max(2.0) * 1.5;
            }
            self.camera.far = (self.distance * 50.0).max(1000.0);
            self.update_camera();
            self.fit_requested = false;
        }
        let resources = self.resources.as_mut().unwrap();
        resources
            .graphics
            .configure(settings.render_execution_policy, false);
        resources
            .graphics
            .activate_surface(GraphicsSurfaceKind::SceneViewport);
        let cpu = !resources.graphics.snapshot().is_gpu_active();
        let limit = if cpu {
            [480.0f32, 360.0]
        } else {
            [1024.0, 768.0]
        };
        let scale = (limit[0] / target_rect.width.max(1) as f32)
            .min(limit[1] / target_rect.height.max(1) as f32)
            .min(1.0);
        let size = [
            (target_rect.width as f32 * scale).max(1.0) as u32,
            (target_rect.height as f32 * scale).max(1.0) as u32,
        ];
        let dark = settings.theme != raf_core::Theme::Light;
        let key = (
            scene.render_fingerprint(),
            self.camera_revision,
            size,
            dark,
            settings.render_execution_policy,
        );
        if resources.key != Some(key) {
            let mut environment = *scene.environment();
            environment.shadows.enabled = false;
            let options = RenderOptions {
                environment_override: Some(environment),
                show_grid_3d: false,
                selection_outline: false,
                geometry_detail: raf_core::project::GeometryDetailMode::Efficient,
                texture_quality: raf_core::project::TextureQualityMode::Efficient,
                ..Default::default()
            };
            let frame = resources.renderer.build_frame(
                scene,
                &self.camera,
                size[0] as f32,
                size[1] as f32,
                &[],
                if dark {
                    [16, 20, 28, 255]
                } else {
                    [225, 228, 234, 255]
                },
                Vec3::new(-0.5, 1.0, 0.3),
                options,
                None,
            );
            let output = resources.graphics.render_scene_frame(&frame);
            resources.output = Some(EditorCanvasLayer {
                output: Arc::new(output),
                source_size: size,
                target_rect,
            });
            resources.key = Some(key);
            self.rendered_frames += 1;
        }
        resources.output.as_ref().map(|layer| EditorCanvasLayer {
            target_rect,
            ..layer.clone()
        })
    }
}
