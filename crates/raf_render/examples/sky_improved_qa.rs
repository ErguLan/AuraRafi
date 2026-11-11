//! One-off QA capture of the improved sky (haze + halo + star chroma).
use raf_render::camera::{Camera, CameraMode};
use raf_render::scene_renderer::{RenderOptions, SceneRenderer};

fn main() {
    let out = std::env::var("RAF_SKY_QA_DIR").unwrap_or_else(|_| ".".to_string());
    let mut scene = raf_core::SceneGraph::new();
    let floor = scene.add_root_with_primitive("Floor", raf_core::scene::Primitive::Cube);
    scene.get_mut(floor).map(|n| {
        n.position = [0.0, -0.5, 0.0].into();
        n.scale = [24.0, 1.0, 24.0].into();
        n.color = raf_core::scene::graph::NodeColor {
            r: 84,
            g: 96,
            b: 88,
            a: 255,
        };
    });
    for i in 0..3 {
        let id = scene.add_root_with_primitive(&format!("Block{i}"), raf_core::scene::Primitive::Cube);
        scene.get_mut(id).map(|n| {
            n.position = [-1.2 + 1.2 * i as f32, 0.75, -0.5].into();
            n.scale = [1.0, 1.5, 1.0].into();
            n.color = raf_core::scene::graph::NodeColor {
                r: 186,
                g: 148,
                b: 92,
                a: 255,
            };
        });
    }
    let mut env = *scene.environment();
    env.mode = raf_core::environment::EnvironmentMode::Scene;
    env.technology = raf_core::environment::LightingTechnology::OptimizedImproved;
    env.sun.azimuth = 0.5;
    env.sun.elevation = 0.28;
    env.sun.color = [255, 176, 96];
    env.sun.intensity = 1.4;
    env.sun.halo = 0.8;
    env.sky.intensity = 1.0;
    env.sky.haze = 0.75;
    env.sky.top_color = [44, 62, 112];
    env.sky.horizon_color = [248, 148, 84];
    env.sky.ground_color = [42, 44, 48];
    env.sky.stars_enabled = true;
    env.sky.stars_density = 0.55;
    env.sky.stars_intensity = 2.5;
    env.shadows.enabled = true;
    env.shadows.resolution = 512.0;
    scene.set_environment(env);
    let camera = Camera {
        mode: CameraMode::Perspective,
        position: glam::Vec3::new(2.2, 1.35, 3.4),
        target: glam::Vec3::new(-0.4, 0.9, 0.0),
        ..Camera::default()
    };
    let mut renderer = SceneRenderer::new(360, 200);
    let options = RenderOptions {
        show_grid_3d: false,
        ..RenderOptions::default()
    };
    let frame = renderer.render(
        &scene,
        &camera,
        360.0,
        200.0,
        &[],
        [26, 30, 40, 255],
        glam::Vec3::new(0.45, 0.72, 0.38).normalize(),
        options,
        None,
    );
    let path = format!("{out}/sky-improved-dusk.png");
    image::save_buffer(&path, frame, 360, 200, image::ColorType::Rgba8).unwrap();
    println!("wrote {path}");
}
