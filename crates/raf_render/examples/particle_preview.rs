//! Renders a particle template into a PNG for visual verification.
//!
//! Run with `cargo run -p raf_render --example particle_preview -- <template> <out.png>`.

use std::path::PathBuf;

use glam::Vec3;
use raf_core::scene::ParticleSystemComponent;
use raf_core::SceneGraph;
use raf_render::api_graphic_basic::device::capture_scene_frame_cpu;
use raf_render::camera::{Camera, CameraMode};
use raf_render::particles::ParticleRenderOptions;
use raf_render::scene_renderer::{RenderOptions, SceneRenderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let template = args.next().unwrap_or_else(|| "fire".to_string());
    let output = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "particle_preview.png".to_string()),
    );
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = std::env::temp_dir().join(format!("raf-particle-preview-{}", std::process::id()));
    let assets = project.join("assets");
    std::fs::create_dir_all(assets.join("particles/textures"))?;
    std::fs::write(
        assets.join(format!("particles/{template}.rafparticle")),
        std::fs::read(repo.join(format!("assets/particles/templates/{template}.rafparticle")))?,
    )?;
    for texture in [
        "soft.png",
        "smoke.png",
        "spark.png",
        "flame.png",
        "fire_atlas.png",
        "smoke_atlas.png",
        "magic_atlas.png",
    ] {
        std::fs::copy(
            repo.join("assets/particles/textures").join(texture),
            assets.join("particles/textures").join(texture),
        )?;
    }

    let mut scene = SceneGraph::new();
    let ground = scene.add_root_with_primitive("Ground", raf_core::scene::Primitive::Plane);
    if let Some(node) = scene.get_mut(ground) {
        node.scale = Vec3::new(6.0, 1.0, 6.0);
        node.color = raf_core::scene::graph::NodeColor::rgba(48, 52, 60, 255);
    }
    let effect = scene.add_root("Effect");
    if let Some(node) = scene.get_mut(effect) {
        node.position = Vec3::new(0.0, 0.15, 0.0);
        node.particle_system = Some(ParticleSystemComponent::new(format!(
            "particles/{template}.rafparticle"
        )));
    }

    let camera = Camera {
        position: Vec3::new(0.0, 1.4, 4.6),
        target: Vec3::new(0.0, 1.0, 0.0),
        mode: CameraMode::Perspective,
        fov: 55.0,
        near: 0.05,
        far: 200.0,
        ..Camera::default()
    };

    let mut renderer = SceneRenderer::new(640, 480);
    renderer.set_asset_root(Some(assets.clone()));
    let options = RenderOptions {
        show_grid_3d: false,
        selection_outline: false,
        particles: ParticleRenderOptions {
            time_scale: 1.0,
            ..ParticleRenderOptions::default()
        },
        particle_dt: 1.0 / 60.0,
        ..RenderOptions::default()
    };
    let mut frame = None;
    for _ in 0..120 {
        frame = Some(renderer.build_frame(
            &scene,
            &camera,
            640.0,
            480.0,
            &[],
            [0; 4],
            Vec3::Y,
            options.clone(),
            None,
        ));
    }
    let frame = frame.ok_or("no frame")?;
    let capture = capture_scene_frame_cpu(
        &frame,
        raf_agb::capabilities::GraphicsMemoryBudget::default(),
    )?;
    let image = image::RgbaImage::from_raw(capture.width, capture.height, capture.rgba8)
        .ok_or("invalid capture")?;
    image.save(&output)?;
    println!(
        "wrote {} ({}x{}, batches={}, active={})",
        output.display(),
        capture.width,
        capture.height,
        renderer.particles().draw_batches(),
        renderer.particles().stats().active
    );
    let _ = std::fs::remove_dir_all(project);
    Ok(())
}
