//! Bounded, data-driven demo scene for the WebAssembly canvas host.
//!
//! The scene is built from an embedded RON recipe. It uses the same
//! `raf_core::SceneGraph`, `Camera`, `SceneRenderer`, and `RenderOptions`
//! contract as the desktop viewport. This is deliberately a demo scene, not a
//! project loader: no path is resolved, no asset is read, and every size is
//! validated before a node is created.

use std::f32::consts::TAU;

use glam::Vec3;
use raf_core::environment::{
    EnvironmentMode, EnvironmentSettings, LightingTechnology, SkyMode, SunControl,
};
use raf_core::scene::{NodeColor, Primitive, SceneGraph, SceneNodeId};
use raf_render::camera::{Camera, CameraMode};
use raf_render::scene_renderer::{
    RenderOptions, SceneRenderFrame, SceneRenderer, SCENE_LIGHT_DIRECTION,
};
use serde::Deserialize;

/// Maximum number of authored objects. Keeps the demo bounded even if the
/// embedded recipe is edited later.
const MAX_OBJECTS: usize = 12;
const MAX_OBJECT_NAME_LEN: usize = 64;
const MAX_ABS_POSITION: f32 = 512.0;
const MIN_SCALE: f32 = 0.01;
const MAX_SCALE: f32 = 512.0;
const MAX_SPIN_SPEED: f32 = 16.0;
const CAMERA_NEAR: f32 = 0.05;
const CAMERA_FAR: f32 = 256.0;
const ELAPSED_WRAP: f32 = TAU * 1024.0;
const BACKGROUND: [u8; 4] = [18, 22, 29, 255];

/// Embedded demo recipe. Editing this string is the supported way to change
/// the WASM demo scene.
pub(crate) const DEMO_SCENE_RECIPE: &str = r#"
(
    camera: (
        target: (0.0, 1.0, 0.0),
        distance: 8.5,
        pitch: 0.34,
        yaw: 0.65,
        yaw_speed: 0.16,
        fov: 50.0,
    ),
    environment: (
        sun_azimuth: 0.9,
        sun_elevation: 0.85,
        sun_color: (255, 244, 226),
        sun_intensity: 1.05,
        sky_top: (30, 39, 54),
        sky_horizon: (94, 114, 136),
        sky_ground: (46, 48, 54),
        sky_intensity: 1.0,
        ambient_color: (176, 188, 206),
        ambient_intensity: 0.35,
    ),
    objects: [
        (
            name: "Ground",
            primitive: plane,
            position: (0.0, 0.0, 0.0),
            scale: (18.0, 1.0, 18.0),
            color: (46, 52, 62),
            rotation: (0.0, 0.0, 0.0),
            spin_speed: 0.0,
        ),
        (
            name: "Accent Cube",
            primitive: cube,
            position: (-2.4, 0.8, 0.2),
            scale: (1.6, 1.6, 1.6),
            color: (224, 116, 24),
            rotation: (0.0, 18.0, 0.0),
            spin_speed: 0.35,
        ),
        (
            name: "Ceramic Sphere",
            primitive: sphere,
            position: (0.2, 0.75, -0.4),
            scale: (1.5, 1.5, 1.5),
            color: (232, 228, 220),
            rotation: (0.0, 0.0, 0.0),
            spin_speed: 0.0,
        ),
        (
            name: "Steel Cylinder",
            primitive: cylinder,
            position: (2.6, 0.75, 0.4),
            scale: (1.0, 1.5, 1.0),
            color: (92, 132, 168),
            rotation: (0.0, 0.0, 0.0),
            spin_speed: 0.22,
        ),
        (
            name: "Teal Capsule",
            primitive: capsule,
            position: (0.9, 1.05, 2.4),
            scale: (1.0, 1.05, 1.0),
            color: (78, 148, 136),
            rotation: (0.0, 0.0, 0.0),
            spin_speed: 0.0,
        ),
    ],
)
"#;

#[derive(Debug, Clone, Deserialize)]
struct DemoSceneRecipe {
    camera: CameraRecipe,
    environment: EnvironmentRecipe,
    objects: Vec<ObjectRecipe>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct CameraRecipe {
    target: (f32, f32, f32),
    distance: f32,
    pitch: f32,
    yaw: f32,
    yaw_speed: f32,
    fov: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct EnvironmentRecipe {
    sun_azimuth: f32,
    sun_elevation: f32,
    sun_color: (u8, u8, u8),
    sun_intensity: f32,
    sky_top: (u8, u8, u8),
    sky_horizon: (u8, u8, u8),
    sky_ground: (u8, u8, u8),
    sky_intensity: f32,
    ambient_color: (u8, u8, u8),
    ambient_intensity: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct ObjectRecipe {
    name: String,
    primitive: PrimitiveRecipe,
    position: (f32, f32, f32),
    scale: (f32, f32, f32),
    color: (u8, u8, u8),
    #[serde(default)]
    rotation: (f32, f32, f32),
    #[serde(default)]
    spin_speed: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PrimitiveRecipe {
    Cube,
    Sphere,
    Plane,
    Cylinder,
    Capsule,
}

impl PrimitiveRecipe {
    fn to_primitive(self) -> Primitive {
        match self {
            Self::Cube => Primitive::Cube,
            Self::Sphere => Primitive::Sphere,
            Self::Plane => Primitive::Plane,
            Self::Cylinder => Primitive::Cylinder,
            Self::Capsule => Primitive::Capsule,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OrbitState {
    target: Vec3,
    distance: f32,
    pitch: f32,
    yaw: f32,
    yaw_speed: f32,
    fov: f32,
}

/// The demo scene: scene graph, camera, render options, and bounded animation
/// state. The renderer is owned by the host so the scene stays presentation
/// data only.
pub(crate) struct DemoScene {
    graph: SceneGraph,
    camera: Camera,
    options: RenderOptions,
    orbit: OrbitState,
    spinning: Vec<(SceneNodeId, f32)>,
    elapsed: f32,
}

impl DemoScene {
    /// Parse and validate the embedded recipe, then build the scene graph.
    pub(crate) fn from_recipe(recipe: &str) -> Result<Self, String> {
        let recipe: DemoSceneRecipe =
            ron::from_str(recipe).map_err(|error| format!("invalid demo recipe: {error}"))?;
        validate_recipe(&recipe)?;

        let orbit = OrbitState {
            target: Vec3::new(
                recipe.camera.target.0,
                recipe.camera.target.1,
                recipe.camera.target.2,
            ),
            distance: recipe.camera.distance,
            pitch: recipe.camera.pitch,
            yaw: recipe.camera.yaw,
            yaw_speed: recipe.camera.yaw_speed,
            fov: recipe.camera.fov,
        };

        let mut graph = SceneGraph::new();
        graph.set_environment(build_environment(&recipe.environment));

        let mut spinning = Vec::new();
        for object in &recipe.objects {
            let id = graph.add_root_with_primitive(&object.name, object.primitive.to_primitive());
            if let Some(node) = graph.get_mut(id) {
                node.position = Vec3::new(object.position.0, object.position.1, object.position.2);
                node.scale = Vec3::new(object.scale.0, object.scale.1, object.scale.2);
                node.rotation = Vec3::new(object.rotation.0, object.rotation.1, object.rotation.2);
                node.color = NodeColor::rgb(object.color.0, object.color.1, object.color.2);
                node.locked = false;
                // The demo environment keeps directional shadows disabled.
                // Declaring participation would only add bookkeeping.
                node.casts_shadows = false;
                node.receives_shadows = false;
            }
            if object.spin_speed != 0.0 {
                spinning.push((id, object.spin_speed));
            }
        }

        let camera = Camera {
            position: camera_position(&orbit),
            target: orbit.target,
            up: Vec3::Y,
            mode: CameraMode::Perspective,
            fov: orbit.fov,
            near: CAMERA_NEAR,
            far: CAMERA_FAR,
            ortho_scale: 10.0,
        };

        Ok(Self {
            graph,
            camera,
            options: render_options(),
            orbit,
            spinning,
            elapsed: 0.0,
        })
    }

    /// Advance the camera orbit and the few spinning objects.
    ///
    /// `dt` is clamped so a suspended tab cannot jump the animation after it
    /// resumes. Elapsed time wraps at a bounded value to avoid precision drift
    /// during long sessions.
    pub(crate) fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        if dt == 0.0 {
            return;
        }
        self.elapsed = (self.elapsed + dt).rem_euclid(ELAPSED_WRAP);
        self.orbit.yaw = (self.orbit.yaw + self.orbit.yaw_speed * dt).rem_euclid(TAU);
        self.camera.position = camera_position(&self.orbit);
        self.camera.target = self.orbit.target;

        let elapsed = self.elapsed;
        for (id, speed) in &self.spinning {
            if let Some(node) = self.graph.get_mut(*id) {
                node.rotation.y = (elapsed * *speed).to_degrees().rem_euclid(360.0);
            }
        }
    }

    /// Record one scene frame through the shared renderer.
    pub(crate) fn render_frame(
        &mut self,
        renderer: &mut SceneRenderer,
        width: u32,
        height: u32,
    ) -> SceneRenderFrame {
        renderer.build_frame(
            &self.graph,
            &self.camera,
            width as f32,
            height as f32,
            &[],
            BACKGROUND,
            SCENE_LIGHT_DIRECTION,
            self.options,
            None,
        )
    }
}

fn camera_position(orbit: &OrbitState) -> Vec3 {
    let horizontal = orbit.distance * orbit.pitch.cos();
    Vec3::new(
        orbit.target.x + horizontal * orbit.yaw.cos(),
        orbit.target.y + orbit.distance * orbit.pitch.sin(),
        orbit.target.z + horizontal * orbit.yaw.sin(),
    )
}

fn render_options() -> RenderOptions {
    RenderOptions {
        show_grid_3d: false,
        solid_face_tonality: true,
        selection_outline: false,
        ..RenderOptions::default()
    }
}

fn build_environment(recipe: &EnvironmentRecipe) -> EnvironmentSettings {
    let mut environment = EnvironmentSettings::default();
    environment.mode = EnvironmentMode::Scene;
    environment.technology = LightingTechnology::OptimizedCurrent;
    environment.exposure = 1.0;

    environment.sun.enabled = true;
    environment.sun.control = SunControl::Manual;
    environment.sun.azimuth = recipe.sun_azimuth;
    environment.sun.elevation = recipe.sun_elevation;
    environment.sun.color = rgb(recipe.sun_color);
    environment.sun.intensity = recipe.sun_intensity;
    environment.sun.disk_visible = false;

    environment.sky.mode = SkyMode::Gradient;
    environment.sky.top_color = rgb(recipe.sky_top);
    environment.sky.horizon_color = rgb(recipe.sky_horizon);
    environment.sky.ground_color = rgb(recipe.sky_ground);
    environment.sky.intensity = recipe.sky_intensity;
    environment.sky.haze = 0.38;
    environment.sky.stars_enabled = false;

    environment.ambient_color = rgb(recipe.ambient_color);
    environment.ambient_intensity = recipe.ambient_intensity;
    environment.shadows.enabled = false;
    environment
}

fn rgb(color: (u8, u8, u8)) -> [u8; 3] {
    [color.0, color.1, color.2]
}

fn validate_recipe(recipe: &DemoSceneRecipe) -> Result<(), String> {
    if recipe.objects.is_empty() {
        return Err("the demo recipe must contain at least one object".to_string());
    }
    if recipe.objects.len() > MAX_OBJECTS {
        return Err(format!(
            "the demo recipe contains {} objects; the limit is {MAX_OBJECTS}",
            recipe.objects.len()
        ));
    }

    ranged(recipe.camera.distance, 1.0, 64.0, "camera.distance")?;
    ranged(recipe.camera.pitch, -1.45, 1.45, "camera.pitch")?;
    ranged(recipe.camera.yaw, -TAU * 8.0, TAU * 8.0, "camera.yaw")?;
    ranged(recipe.camera.yaw_speed, -2.0, 2.0, "camera.yaw_speed")?;
    ranged(recipe.camera.fov, 20.0, 100.0, "camera.fov")?;
    for value in [
        recipe.camera.target.0,
        recipe.camera.target.1,
        recipe.camera.target.2,
    ] {
        ranged(value, -MAX_ABS_POSITION, MAX_ABS_POSITION, "camera.target")?;
    }

    ranged(recipe.environment.sun_azimuth, -TAU, TAU, "sun_azimuth")?;
    ranged(recipe.environment.sun_elevation, -1.5, 1.5, "sun_elevation")?;
    ranged(recipe.environment.sun_intensity, 0.0, 4.0, "sun_intensity")?;
    ranged(recipe.environment.sky_intensity, 0.0, 2.0, "sky_intensity")?;
    ranged(
        recipe.environment.ambient_intensity,
        0.0,
        1.0,
        "ambient_intensity",
    )?;

    for object in &recipe.objects {
        if object.name.trim().is_empty() {
            return Err("every demo object needs a non-empty name".to_string());
        }
        if object.name.len() > MAX_OBJECT_NAME_LEN {
            return Err(format!(
                "object name '{}' exceeds the {MAX_OBJECT_NAME_LEN} byte limit",
                object.name
            ));
        }
        for value in [object.position.0, object.position.1, object.position.2] {
            ranged(
                value,
                -MAX_ABS_POSITION,
                MAX_ABS_POSITION,
                "object.position",
            )?;
        }
        for value in [object.scale.0, object.scale.1, object.scale.2] {
            ranged(value, MIN_SCALE, MAX_SCALE, "object.scale")?;
        }
        ranged(
            object.spin_speed,
            -MAX_SPIN_SPEED,
            MAX_SPIN_SPEED,
            "object.spin_speed",
        )?;
        ranged(object.rotation.0, -3600.0, 3600.0, "object.rotation.x")?;
        ranged(object.rotation.1, -3600.0, 3600.0, "object.rotation.y")?;
        ranged(object.rotation.2, -3600.0, 3600.0, "object.rotation.z")?;
    }

    Ok(())
}

fn ranged(value: f32, min: f32, max: f32, name: &str) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{name} must be a finite number"));
    }
    if value < min || value > max {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(())
}
