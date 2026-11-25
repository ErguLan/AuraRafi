use std::{path::PathBuf, sync::Arc};

use glam::{Mat4, Vec3};
use raf_agb::product::{Command, Pipeline, SkyBackdrop};
use raf_core::environment::{EnvironmentMode, EnvironmentSettings, LightingTechnology, SkyMode};

use super::*;
use crate::api_graphic_basic::command_list::{BasicCommandList, BasicScreenTriangle};
use crate::api_graphic_basic::mesh::{BasicMesh, BasicVertex};
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::scene_renderer::{FrameStats, SceneRenderFrame};
use crate::texture::CpuTexture;

fn frame(width: u32, height: u32, commands: BasicCommandList) -> SceneRenderFrame {
    SceneRenderFrame {
        commands,
        view_proj: Mat4::IDENTITY,
        light_dir: Vec3::Y,
        width,
        height,
        texture_cache_budget_bytes: 0,
        stats: FrameStats::default(),
    }
}

fn triangle_mesh() -> Arc<BasicMesh> {
    Arc::new(BasicMesh::new(
        vec![
            BasicVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: [0.0, 1.0],
            },
            BasicVertex {
                position: Vec3::X,
                normal: Vec3::Y,
                uv: [1.0, 1.0],
            },
            BasicVertex {
                position: Vec3::Y,
                normal: Vec3::Y,
                uv: [0.5, 0.0],
            },
        ],
        vec![0, 1, 2],
    ))
}

fn texture(source: &str, rgba: [u8; 4]) -> Arc<CpuTexture> {
    Arc::new(CpuTexture {
        data: rgba.to_vec(),
        width: 1,
        height: 1,
        source: PathBuf::from(source),
    })
}

fn screen_triangle_frame(width: u32, height: u32) -> SceneRenderFrame {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    commands.set_pipeline(BasicPipelineKind::PbrLit);
    commands.draw_screen_triangle(BasicScreenTriangle {
        points: [[0.0, 0.0], [width as f32, 0.0], [0.0, height as f32]],
        color: [255, 0, 0, 255],
    });
    frame(width, height, commands)
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "expected {expected}, got {actual}"
    );
}

fn environment_frame(settings: EnvironmentSettings) -> SceneRenderFrame {
    let environment = crate::environment_frame::EnvironmentFrame::new(settings, Mat4::IDENTITY)
        .expect("scene mode must resolve an environment frame");
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    commands.environment = Some(environment);
    frame(64, 64, commands)
}

fn packet_sky(frame: &SceneRenderFrame, revision: u64) -> SkyBackdrop {
    let mut builder = ProductFrameBuilder::default();
    builder.append_scene(frame, revision).unwrap();
    let mut skies = builder
        .packet()
        .commands
        .iter()
        .filter_map(|command| match command {
            Command::DrawSky(sky) => Some(*sky),
            _ => None,
        });
    let sky = skies.next().expect("an authored scene must emit a sky");
    assert!(
        skies.next().is_none(),
        "the sky must be emitted exactly once"
    );
    sky
}

fn assert_ambient(uniforms: &FrameUniforms, expected: Vec3) {
    let fraction =
        (0.2126 * expected.x + 0.7152 * expected.y + 0.0722 * expected.z).clamp(0.0, 1.0);
    close(uniforms.environment.ambient, fraction);
    let tint = Vec3::from_array(uniforms.environment.ambient_tint);
    let reconstructed = tint * uniforms.environment.ambient;
    for channel in 0..3 {
        close(reconstructed[channel], expected[channel]);
    }
}

#[test]
fn scene_commands_keep_painter_order_and_state() {
    let mut commands = BasicCommandList::new();
    commands.clear([10, 20, 30, 255]);
    let mesh_id = commands.register_mesh(triangle_mesh());
    let texture_id = commands.register_texture(texture("albedo", [200, 100, 50, 255]));
    commands.set_pipeline(BasicPipelineKind::PbrLit);
    commands.set_shadow_receiver(false);
    commands.draw_mesh_with_texture(
        mesh_id,
        Mat4::IDENTITY,
        [255, 128, 64, 255],
        Some(texture_id),
    );
    commands.draw_line(Vec3::ZERO, Vec3::X, [255, 0, 0, 255], 2.0, false, 0.0);
    commands.draw_grid(0.25, 1.5, false);

    let mut builder = ProductFrameBuilder::default();
    let metrics = builder.append_scene(&frame(320, 200, commands), 3).unwrap();
    assert_eq!(metrics.product_packet_rebuilds, 1);
    assert_eq!(metrics.product_packet_revision, 3);
    assert_eq!(metrics.product_mesh_count, 1);
    assert_eq!(metrics.product_texture_count, 1);
    assert_eq!(metrics.product_material_count, 1);

    let packet = builder.packet();
    assert_eq!(
        packet.commands.len(),
        7,
        "Clear, receiver seed, pipeline, receiver, mesh, line batch, grid"
    );
    let Command::Clear(clear) = &packet.commands[0] else {
        panic!("the scene frame must start with Clear");
    };
    close(clear[0], crate::post_process::srgb_to_linear(10.0 / 255.0));
    close(clear[3], 1.0);
    assert!(matches!(
        &packet.commands[1],
        Command::SetShadowReceiver(true)
    ));
    assert!(matches!(
        &packet.commands[2],
        Command::SetPipeline(Pipeline::LIT)
    ));
    assert!(matches!(
        &packet.commands[3],
        Command::SetShadowReceiver(false)
    ));
    let Command::DrawMesh {
        mesh,
        material,
        instance,
    } = &packet.commands[4]
    else {
        panic!("the mesh draw must stay in place");
    };
    assert_eq!((*mesh, *material), (0, 0));
    close(instance.tint[0], 1.0);
    close(
        instance.tint[1],
        crate::post_process::srgb_to_linear(128.0 / 255.0),
    );
    assert!(matches!(
        &packet.commands[5],
        Command::DrawLineBatch(lines) if lines.len() == 1 && lines[0].width_px == 2.0
    ));
    let Command::DrawGrid(grid) = &packet.commands[6] else {
        panic!("the grid must stay last");
    };
    assert_eq!((grid.y, grid.spacing), (0.25, 1.5));
}

#[test]
fn supported_pipelines_map_and_unsupported_states_keep_their_name() {
    let supported = [
        (BasicPipelineKind::FlatColor, Pipeline::SCENE),
        (
            BasicPipelineKind::FlatColorTwoSided,
            Pipeline::SCENE_TWO_SIDED,
        ),
        (BasicPipelineKind::PbrLit, Pipeline::LIT),
        (BasicPipelineKind::PbrLitTwoSided, Pipeline::LIT_TWO_SIDED),
        (BasicPipelineKind::XRay, Pipeline::OVERLAY),
        (BasicPipelineKind::UiSurface, Pipeline::OVERLAY),
        (BasicPipelineKind::UiSurfaceOverlay, Pipeline::OVERLAY),
    ];
    for (kind, expected) in supported {
        let mut commands = BasicCommandList::new();
        commands.clear([0; 4]);
        commands.set_pipeline(BasicPipelineKind::XRay);
        commands.set_pipeline(kind);
        commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, true, 0.0);
        let mut builder = ProductFrameBuilder::default();
        builder.append_scene(&frame(64, 64, commands), 1).unwrap();
        let mapped = builder
            .packet()
            .commands
            .iter()
            .rev()
            .find_map(|command| match command {
                Command::SetPipeline(pipeline) => Some(*pipeline),
                _ => None,
            });
        assert_eq!(mapped, Some(expected), "{kind:?}");
    }

    for kind in [BasicPipelineKind::UnlitWireframe] {
        let mut commands = BasicCommandList::new();
        commands.clear([0; 4]);
        commands.set_pipeline(BasicPipelineKind::XRay);
        commands.set_pipeline(kind);
        let mut builder = ProductFrameBuilder::default();
        let error = builder
            .append_scene(&frame(64, 64, commands), 1)
            .unwrap_err();
        assert!(
            error.contains(&format!("{kind:?}")),
            "the error must name the state: {error}"
        );
    }
}

#[test]
fn line_and_grid_mapping_preserves_depth_flags_and_color() {
    let mut commands = BasicCommandList::new();
    commands.clear([0, 0, 0, 255]);
    commands.draw_line(
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(4.0, 5.0, 6.0),
        [255, 128, 64, 255],
        2.5,
        false,
        0.125,
    );
    commands.draw_line(Vec3::ZERO, Vec3::Y, [0, 255, 0, 128], 3.0, true, 0.25);
    commands.draw_grid(0.5, 2.0, true);

    let mut builder = ProductFrameBuilder::default();
    builder.append_scene(&frame(100, 50, commands), 1).unwrap();
    let packet = builder.packet();

    let Command::DrawLineBatch(depth_tested) = &packet.commands[2] else {
        panic!("the depth-tested line batch is missing");
    };
    assert_eq!(depth_tested.len(), 1);
    assert!(!depth_tested[0].no_depth_test);
    assert_eq!(depth_tested[0].start, [1.0, 2.0, 3.0]);
    assert_eq!(depth_tested[0].end, [4.0, 5.0, 6.0]);
    assert_eq!(depth_tested[0].width_px, 2.5);
    assert_eq!(depth_tested[0].depth_bias, 0.125);
    close(
        depth_tested[0].rgba[1],
        crate::post_process::srgb_to_linear(128.0 / 255.0),
    );

    let Command::DrawLineBatch(xray) = &packet.commands[3] else {
        panic!("the depth-disabled line batch is missing");
    };
    assert!(xray[0].no_depth_test);
    close(xray[0].rgba[3], 128.0 / 255.0);

    let Command::DrawGrid(grid) = &packet.commands[4] else {
        panic!("the grid is missing");
    };
    assert_eq!((grid.y, grid.spacing, grid.no_depth_test), (0.5, 2.0, true));
}

#[test]
fn textures_and_materials_deduplicate_by_identity() {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    let mesh_id = commands.register_mesh(triangle_mesh());
    let albedo = commands.register_texture(texture("albedo", [200, 100, 50, 255]));
    let detail = commands.register_texture(texture("detail", [10, 20, 30, 255]));
    commands.draw_mesh_with_texture(mesh_id, Mat4::IDENTITY, [255; 4], Some(albedo));
    commands.draw_mesh_with_texture(
        mesh_id,
        Mat4::from_translation(Vec3::X),
        [128; 4],
        Some(albedo),
    );
    commands.draw_mesh_with_texture(
        mesh_id,
        Mat4::from_translation(Vec3::Y),
        [64; 4],
        Some(detail),
    );

    let mut builder = ProductFrameBuilder::default();
    builder.append_scene(&frame(64, 64, commands), 1).unwrap();
    let packet = builder.packet();
    assert_eq!(packet.meshes.len(), 1);
    assert_eq!(packet.textures.len(), 2);
    assert_eq!(packet.materials.len(), 2);
    let materials = packet
        .commands
        .iter()
        .filter_map(|command| match command {
            Command::DrawMesh { material, .. } => Some(*material),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(materials, vec![0, 0, 1]);
}

#[test]
fn camera_only_revisions_reuse_the_packet_without_rebuilding() {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    let mesh_id = commands.register_mesh(triangle_mesh());
    let albedo = commands.register_texture(texture("albedo", [200, 100, 50, 255]));
    commands.draw_mesh_with_texture(mesh_id, Mat4::IDENTITY, [255; 4], Some(albedo));

    let mut builder = ProductFrameBuilder::default();
    let first = builder
        .append_scene(&frame(128, 96, commands.clone()), 7)
        .unwrap();
    assert_eq!(first.product_packet_rebuilds, 1);
    assert_eq!(
        (first.product_mesh_count, first.product_texture_count),
        (1, 1)
    );

    let second = builder
        .append_scene(&frame(128, 96, commands.clone()), 7)
        .unwrap();
    assert_eq!(second.product_packet_rebuilds, 0);
    assert_eq!(second.product_packet_revision, 7);
    assert_eq!(second.product_command_count, first.product_command_count);
    assert_eq!(builder.packet().meshes.len(), 1);
    assert_eq!(builder.packet().textures.len(), 1);

    let third = builder.append_scene(&frame(128, 96, commands), 8).unwrap();
    assert_eq!(third.product_packet_rebuilds, 1);
    assert_eq!(third.product_packet_revision, 8);
}

#[test]
fn older_revisions_are_rejected() {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);
    let mut builder = ProductFrameBuilder::default();
    builder
        .append_scene(&frame(64, 64, commands.clone()), 4)
        .unwrap();
    let error = builder
        .append_scene(&frame(64, 64, commands), 3)
        .unwrap_err();
    assert!(error.contains("older"), "{error}");
    assert_eq!(builder.revision(), 4);
}

#[test]
fn screen_triangles_convert_pixels_and_restore_the_scene_pipeline() {
    let mut builder = ProductFrameBuilder::default();
    builder
        .append_scene(&screen_triangle_frame(320, 200), 1)
        .unwrap();
    let packet = builder.packet();
    assert_eq!(
        packet.commands.len(),
        6,
        "Clear, receiver seed, LIT, overlay switch, triangles, LIT restore"
    );
    assert!(matches!(
        &packet.commands[3],
        Command::SetPipeline(Pipeline::OVERLAY)
    ));
    let Command::DrawScreenTriangleBatch { vertices, material } = &packet.commands[4] else {
        panic!("the screen triangle batch is missing");
    };
    assert_eq!(*material, 0);
    assert_eq!(vertices.len(), 3);
    let points = vertices
        .iter()
        .map(|vertex| vertex.position)
        .collect::<Vec<_>>();
    close(points[0][0], -1.0);
    close(points[0][1], 1.0);
    close(points[1][0], 1.0);
    close(points[1][1], 1.0);
    close(points[2][0], -1.0);
    close(points[2][1], -1.0);
    close(vertices[0].rgba[0], 1.0);
    assert!(matches!(
        &packet.commands[5],
        Command::SetPipeline(Pipeline::LIT)
    ));
}

#[test]
fn pixel_space_frames_need_a_new_revision_when_the_extent_changes() {
    let mut builder = ProductFrameBuilder::default();
    builder
        .append_scene(&screen_triangle_frame(320, 200), 2)
        .unwrap();
    let error = builder
        .append_scene(&screen_triangle_frame(640, 400), 2)
        .unwrap_err();
    assert!(error.contains("extent"), "{error}");

    let world = || {
        let mut commands = BasicCommandList::new();
        commands.clear([0; 4]);
        commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);
        frame(320, 200, commands)
    };
    let mut builder = ProductFrameBuilder::default();
    builder.append_scene(&world(), 2).unwrap();
    let mut resized = world();
    resized.width = 640;
    resized.height = 400;
    let reused = builder.append_scene(&resized, 2).unwrap();
    assert_eq!(reused.product_packet_rebuilds, 0);
}

#[test]
fn handoff_keeps_revision_reuse_and_rebuilds_from_the_source() {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);
    let mut builder = ProductFrameBuilder::default();
    builder
        .append_scene(&frame(64, 64, commands.clone()), 5)
        .unwrap();
    assert_eq!(builder.packet().commands.len(), 3);

    let packet = builder.handoff_packet().unwrap();
    assert_eq!(packet.commands.len(), 3);
    assert!(!builder.has_packet());
    assert!(builder.packet().commands.is_empty());

    // A camera-only frame at the same revision must not rebuild.
    let reused = builder
        .append_scene(&frame(64, 64, commands.clone()), 5)
        .unwrap();
    assert_eq!(reused.product_packet_rebuilds, 0);
    assert!(!builder.has_packet());

    // Reconstruction after device loss materializes the retained source.
    let rebuilt = builder.handoff_packet().unwrap();
    assert_eq!(rebuilt.commands.len(), 3);
    assert_eq!(builder.revision(), 5);

    // A new revision rebuilds eagerly for the next composition.
    let changed = builder.append_scene(&frame(64, 64, commands), 6).unwrap();
    assert_eq!(changed.product_packet_rebuilds, 1);
    assert!(builder.has_packet());
}

#[test]
fn malformed_frames_return_explicit_errors() {
    let mut builder = ProductFrameBuilder::default();
    let error = builder
        .append_scene(&frame(64, 64, BasicCommandList::new()), 1)
        .unwrap_err();
    assert!(error.contains("no commands"), "{error}");

    let mut without_clear = BasicCommandList::new();
    without_clear.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);
    let error = builder
        .append_scene(&frame(64, 64, without_clear), 1)
        .unwrap_err();
    assert!(error.contains("begin with Clear"), "{error}");

    let mut late_clear = BasicCommandList::new();
    late_clear.clear([0; 4]);
    late_clear.clear([9; 4]);
    let error = builder
        .append_scene(&frame(64, 64, late_clear), 1)
        .unwrap_err();
    assert!(error.contains("only valid as the first command"), "{error}");

    let mut missing = BasicCommandList::new();
    missing.clear([0; 4]);
    missing.draw_mesh(42, Mat4::IDENTITY, [255; 4]);
    let error = builder
        .append_scene(&frame(64, 64, missing), 1)
        .unwrap_err();
    assert!(error.contains("mesh 42"), "{error}");

    assert!(!builder.has_packet());
    assert!(builder.packet().commands.is_empty());
}

#[test]
fn scene_environment_becomes_a_sky_backdrop_before_geometry() {
    let mut settings = raf_core::environment::EnvironmentSettings::default();
    settings.mode = raf_core::environment::EnvironmentMode::Scene;
    let environment =
        crate::environment_frame::EnvironmentFrame::new(settings, Mat4::IDENTITY).unwrap();
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    commands.environment = Some(environment);
    commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);

    let mut builder = ProductFrameBuilder::default();
    builder.append_scene(&frame(64, 64, commands), 1).unwrap();
    let packet = builder.packet();
    let Command::DrawSky(sky) = &packet.commands[1] else {
        panic!("the sky must follow the clear");
    };
    assert_eq!(sky.haze, 0.0);
    close(sky.zenith[3], 1.0);
    assert!(matches!(
        &packet.commands[2],
        Command::SetShadowReceiver(true)
    ));
}

#[test]
fn placed_scene_with_environment_keeps_the_sky_first() {
    let mut settings = raf_core::environment::EnvironmentSettings::default();
    settings.mode = raf_core::environment::EnvironmentMode::Scene;
    let environment =
        crate::environment_frame::EnvironmentFrame::new(settings, Mat4::IDENTITY).unwrap();
    let mut commands = BasicCommandList::new();
    commands.clear([10, 20, 30, 255]);
    commands.environment = Some(environment);
    commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);

    let mut builder = ProductFrameBuilder::default();
    builder
        .append_scene_placed(
            &frame(64, 64, commands),
            1,
            Some([0.0, 0.0, 0.0, 1.0]),
            Some([0.5, 0.5, 0.5, 0.5]),
        )
        .unwrap();
    let packet = builder.packet();
    assert!(matches!(&packet.commands[1], Command::DrawSky(_)));
    assert!(
        !packet
            .commands
            .iter()
            .any(|command| matches!(command, Command::SceneBackground(_))),
        "a sky backdrop already owns the canvas background"
    );
}

#[test]
fn placed_scene_without_environment_uses_a_scene_background() {
    let mut commands = BasicCommandList::new();
    commands.clear([10, 20, 30, 255]);
    commands.draw_line(Vec3::ZERO, Vec3::X, [255; 4], 1.0, false, 0.0);

    let mut builder = ProductFrameBuilder::default();
    builder
        .append_scene_placed(
            &frame(64, 64, commands),
            1,
            Some([0.0, 0.0, 0.0, 1.0]),
            Some([0.5, 0.5, 0.5, 0.5]),
        )
        .unwrap();
    let packet = builder.packet();
    let Command::SceneBackground(color) = &packet.commands[1] else {
        panic!("the canvas background must follow the target clear");
    };
    close(color[0], linear_rgba([10, 20, 30, 255])[0]);
}

#[test]
fn camera_uniforms_preserve_the_authored_perspective_eye() {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    commands.camera_eye = Vec3::new(0.0, 1.0, 4.0);
    let mut scene = frame(64, 64, commands);
    scene.view_proj = Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0)
        * Mat4::from_translation(-scene.commands.camera_eye);
    scene.light_dir = Vec3::new(0.0, 2.0, 0.0);
    let uniforms = scene_frame_uniforms(&scene);
    assert_eq!(uniforms.camera.view_projection, scene.view_proj);
    close(uniforms.camera.world_position.x, 0.0);
    close(uniforms.camera.world_position.y, 1.0);
    close(uniforms.camera.world_position.z, 4.0);
    close(uniforms.light.toward_light.y, 1.0);
}

#[test]
fn neutral_environment_emits_no_sky_and_keeps_neutral_uniforms() {
    let mut commands = BasicCommandList::new();
    commands.clear([10, 20, 30, 255]);
    let scene = frame(64, 64, commands);

    let mut builder = ProductFrameBuilder::default();
    builder.append_scene(&scene, 1).unwrap();
    assert!(
        !builder
            .packet()
            .commands
            .iter()
            .any(|command| matches!(command, Command::DrawSky(_) | Command::SceneBackground(_))),
        "neutral lighting must not synthesize a sky or scene background"
    );

    let uniforms = scene_frame_uniforms(&scene);
    assert!(!uniforms.environment.authored);
    assert!(!uniforms.environment.improved);
    close(uniforms.environment.exposure_ev, 0.0);
    close(uniforms.environment.ambient, 0.35);
    assert_eq!(uniforms.environment.ambient_tint, [1.0; 3]);
    close(uniforms.environment.toon_steps, 0.0);
    close(uniforms.environment.toon_softness, 0.0);
    assert!(uniforms.shadows.is_none());
    assert!(!uniforms.scene_filter);
}

#[test]
fn solid_sky_collapses_horizon_and_ground_to_the_zenith() {
    let mut settings = EnvironmentSettings::default();
    settings.mode = EnvironmentMode::Scene;
    settings.sky.mode = SkyMode::Solid;
    settings.sky.top_color = [30, 90, 220];
    settings.sky.horizon_color = [200, 221, 240];
    settings.sky.ground_color = [40, 50, 60];
    let scene = environment_frame(settings);
    let environment = scene.commands.environment.unwrap();
    let sky = packet_sky(&scene, 1);
    for channel in 0..3 {
        close(sky.zenith[channel], environment.sky_top[channel]);
        close(sky.horizon[channel], environment.sky_top[channel]);
        close(sky.ground[channel], environment.sky_top[channel]);
    }
    close(sky.zenith[3], 1.0);
}

#[test]
fn current_environment_preserves_sky_sun_and_toon_fields() {
    let mut settings = EnvironmentSettings::default();
    settings.mode = EnvironmentMode::Scene;
    settings.technology = LightingTechnology::OptimizedCurrent;
    settings.exposure = 2.0;
    settings.sky.top_color = [30, 90, 220];
    settings.sky.horizon_color = [200, 221, 240];
    settings.sky.ground_color = [40, 50, 60];
    settings.sky.haze = 0.8;
    settings.sky.stars_enabled = true;
    settings.sky.stars_density = 0.3;
    settings.sky.stars_intensity = 1.7;
    settings.sun.color = [255, 200, 100];
    settings.sun.intensity = 1.25;
    settings.sun.angular_radius = 0.01;
    settings.sun.halo = 0.9;
    settings.sun.toon_enabled = true;
    settings.sun.toon_steps = 4.0;
    settings.sun.toon_softness = 0.5;
    settings.ambient_color = [255, 128, 64];
    settings.ambient_intensity = 0.4;
    let scene = environment_frame(settings);
    let environment = scene.commands.environment.unwrap();

    let sky = packet_sky(&scene, 1);
    for channel in 0..3 {
        close(sky.zenith[channel], environment.sky_top[channel]);
        close(sky.horizon[channel], environment.sky_horizon[channel]);
        close(sky.ground[channel], environment.sky_ground[channel]);
    }
    close(sky.haze, 0.0);
    close(sky.sun_disk_cos, settings.sun.angular_radius.cos());
    close(sky.halo, 0.0);
    close(sky.halo_cos, environment.halo_cos);
    close(sky.stars_density, settings.sky.stars_density);
    close(sky.stars_intensity, settings.sky.stars_intensity);

    let uniforms = scene_frame_uniforms(&scene);
    assert!(uniforms.environment.authored);
    assert!(!uniforms.environment.improved);
    close(uniforms.environment.exposure_ev, 0.0);
    for channel in 0..3 {
        close(uniforms.light.color[channel], environment.sun[channel]);
    }
    close(
        uniforms.light.toward_light.dot(environment.sun_direction),
        1.0,
    );
    close(uniforms.environment.toon_steps, 4.0);
    close(uniforms.environment.toon_softness, 0.0);
    assert_ambient(&uniforms, environment.ambient);
}

#[test]
fn improved_environment_maps_exposure_toon_and_bounded_sky_without_double_exposure() {
    let mut settings = EnvironmentSettings::default();
    settings.mode = EnvironmentMode::Scene;
    settings.technology = LightingTechnology::OptimizedImproved;
    settings.exposure = 4.0;
    settings.sky.top_color = [30, 90, 220];
    settings.sky.horizon_color = [200, 221, 240];
    settings.sky.ground_color = [40, 50, 60];
    settings.sky.haze = 0.6;
    settings.sky.stars_enabled = true;
    settings.sky.stars_density = 0.5;
    settings.sky.stars_intensity = 2.5;
    settings.sun.halo = 0.45;
    settings.sun.angular_radius = 0.012;
    settings.sun.toon_enabled = true;
    settings.sun.toon_steps = 5.0;
    settings.sun.toon_softness = 0.7;
    settings.ambient_color = [255, 128, 64];
    settings.ambient_intensity = 0.4;
    let scene = environment_frame(settings);
    let environment = scene.commands.environment.unwrap();

    let sky = packet_sky(&scene, 1);
    close(sky.haze, 0.6);
    close(sky.halo, settings.sun.halo);
    close(sky.halo_cos, environment.halo_cos);
    close(sky.stars_density, 0.5);
    close(sky.stars_intensity, 2.5);
    for channel in 0..3 {
        close(sky.zenith[channel], environment.sky_top[channel]);
        close(sky.horizon[channel], environment.sky_horizon[channel]);
        close(sky.ground[channel], environment.sky_ground[channel]);
    }

    // Changing exposure must not alter the packet sky colors: exposure is
    // applied once by the shader from `environment.exposure_ev`.
    let mut dim = settings;
    dim.exposure = 1.0;
    let dim_scene = environment_frame(dim);
    let dim_sky = packet_sky(&dim_scene, 1);
    assert_eq!(sky.zenith, dim_sky.zenith);
    assert_eq!(sky.horizon, dim_sky.horizon);
    assert_eq!(sky.ground, dim_sky.ground);

    let uniforms = scene_frame_uniforms(&scene);
    assert!(uniforms.environment.authored);
    assert!(uniforms.environment.improved);
    close(uniforms.environment.exposure_ev, 2.0);
    close(uniforms.environment.toon_steps, 5.0);
    close(uniforms.environment.toon_softness, 0.7);
    assert_ambient(&uniforms, environment.ambient);

    let dim_uniforms = scene_frame_uniforms(&dim_scene);
    close(dim_uniforms.environment.exposure_ev, 0.0);
}

#[test]
fn shadow_contract_mapping_follows_enabled_state_and_filter_quality() {
    let (mut scene, _floor, _cube) = crate::shadow_frame::tests::fixture();
    let mut renderer = crate::scene_renderer::SceneRenderer::new(128, 128);

    let scene_frame = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    let source = scene_frame
        .commands
        .shadows
        .as_ref()
        .expect("the fixture enables shadows");
    assert!(source.settings.enabled);
    let mapped = scene_frame_uniforms(&scene_frame)
        .shadows
        .expect("enabled shadows must map");
    assert_eq!(
        mapped.light_view_projection,
        source.geometry.view_projection
    );
    assert_eq!(mapped.resolution, source.geometry.resolution);
    assert_eq!(mapped.depth_key, source.geometry.key);
    close(
        mapped.bias,
        source.settings.bias / (8.0 * source.settings.distance),
    );
    close(mapped.distance, source.settings.distance);
    close(mapped.strength, source.settings.strength);
    assert!(mapped.slope_bias_per_texel > 0.0);
    assert_eq!(mapped.filter_taps, 1);

    let mut env = *scene.environment();
    env.shadows.soft_filter = true;
    scene.set_environment(env);
    let soft = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    assert_eq!(scene_frame_uniforms(&soft).shadows.unwrap().filter_taps, 4);

    env.technology = LightingTechnology::OptimizedImproved;
    scene.set_environment(env);
    let improved_soft = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    assert_eq!(
        scene_frame_uniforms(&improved_soft)
            .shadows
            .unwrap()
            .filter_taps,
        9
    );

    env.shadows.enabled = false;
    scene.set_environment(env);
    let disabled = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    assert!(disabled.commands.shadows.is_none());
    assert!(scene_frame_uniforms(&disabled).shadows.is_none());
}

#[cfg(windows)]
#[test]
#[ignore = "requires Vulkan and DX12 hardware"]
fn native_lighting_sky_and_shadow_contract_reaches_gpu_pixels() {
    use crate::{
        camera::{Camera, CameraMode},
        scene_renderer::{RenderOptions, SceneRenderer},
    };
    use raf_agb::{
        backends::native::{NativeApi, NativeBackend, NativeRequest},
        capabilities::GraphicsMemoryBudget,
        Output,
    };
    use raf_core::{environment::EnvironmentMode, SceneGraph};

    for api in [NativeApi::Vulkan, NativeApi::Dx12] {
        let backend = NativeBackend::headless(NativeRequest::Exact(api)).unwrap();
        let mut device = ProductDevice::new(backend, GraphicsMemoryBudget::potato()).unwrap();
        assert!(device.info().hardware && device.supports_shadows(), "{api}");
        device.configure([128, 128], Output::Capture).unwrap();
        let mut builder = ProductFrameBuilder::default();
        let mut renderer = SceneRenderer::new(128, 128);
        let mut scene = SceneGraph::new();
        let mut env = *scene.environment();
        env.mode = EnvironmentMode::Scene;
        env.sky.top_color = [38, 110, 221];
        env.sky.horizon_color = [38, 110, 221];
        env.sky.ground_color = [38, 110, 221];
        env.sun.enabled = false;
        scene.set_environment(env);
        let eye = Vec3::new(1000.0, 40.0, -2000.0);
        for (revision, mode) in [(1, CameraMode::Perspective), (2, CameraMode::Orthographic)] {
            let camera = Camera {
                position: eye,
                target: eye - Vec3::Z,
                mode,
                ..Camera::default()
            };
            let frame = renderer.build_frame(
                &scene,
                &camera,
                64.0,
                64.0,
                &[],
                [0, 0, 0, 255],
                Vec3::Y,
                RenderOptions {
                    show_grid_3d: false,
                    ..RenderOptions::default()
                },
                None,
            );
            builder
                .append_scene_placed(
                    &frame,
                    revision,
                    Some([0., 0., 0., 1.]),
                    Some([0.25, 0.25, 0.5, 0.5]),
                )
                .unwrap();
            let handle = prepare_product_scene(&mut device, &mut builder).unwrap();
            let mut uniforms = scene_frame_uniforms(&frame);
            uniforms.viewport = [0.25, 0.25, 0.5, 0.5];
            let metrics = device.draw(handle, &uniforms).unwrap();
            assert_eq!(metrics.draw_calls, 1, "{api} {mode:?}");
            let rgba = device.capture().unwrap();
            for (x, y) in [(32, 32), (64, 64), (95, 95)] {
                let offset = (y * 128 + x) * 4;
                assert!(
                    (181..=187).contains(&rgba[offset + 2])
                        && (37..=43).contains(&rgba[offset + 1]),
                    "{api} {mode:?} {:?}",
                    &rgba[offset..offset + 4]
                );
            }
            assert_eq!(
                &rgba[..4],
                &[0, 0, 0, 255],
                "{api} sky must remain inside canvas"
            );
        }
        let (mut scene, floor, caster) = crate::shadow_frame::tests::fixture();
        let mut env = *scene.environment();
        env.shadows.resolution = 512.0;
        scene.set_environment(env);
        let mut before = Vec::new();
        let mut timings = Vec::new();
        for revision in 3..=7 {
            if revision == 4 {
                env.sky.top_color = [30, 90, 220];
                scene.set_environment(env);
            } else if revision == 5 {
                scene.get_mut(caster).unwrap().position.x += 0.25;
            } else if revision == 6 {
                env.shadows.soft_filter = true;
                scene.set_environment(env);
            } else if revision == 7 {
                env.shadows.enabled = false;
                scene.set_environment(env);
            }
            let frame = crate::shadow_frame::tests::frame(&mut renderer, &scene);
            builder.append_scene(&frame, revision).unwrap();
            let handle = prepare_product_scene(&mut device, &mut builder).unwrap();
            let uniforms = scene_frame_uniforms(&frame);
            let metrics = device.draw(handle, &uniforms).unwrap();
            let rgba = device.capture().unwrap();
            assert_eq!(
                metrics.shadow_resolution,
                if revision == 7 { 0 } else { 512 }
            );
            assert_eq!(
                metrics.shadow_resident_bytes,
                if revision == 7 { 0 } else { 1024 * 1024 }
            );
            assert_eq!(
                metrics.shadow_draw_calls,
                u32::from(revision == 3 || revision == 5)
            );
            if revision == 6 {
                before = rgba;
            } else if revision == 7 {
                let lit = before
                    .chunks_exact(4)
                    .zip(rgba.chunks_exact(4))
                    .filter(|(a, b)| {
                        b[0] > a[0].saturating_add(15) && b[1] > a[1].saturating_add(15)
                    })
                    .count();
                assert!(lit > 8, "{api}: {lit} shadowed pixels");
                eprintln!("{api}: {lit} shadowed pixels at 512²");
            }
            timings.push((metrics.draw_calls, metrics.gpu_ms));
        }
        assert_eq!(
            timings[0].0,
            timings[1].0 + 1,
            "{api}: sky must reuse the depth map"
        );
        assert_eq!(
            timings[2].0,
            timings[3].0 + 1,
            "{api}: moving caster must rebuild, Soft must reuse"
        );
        env.shadows.enabled = true;
        scene.set_environment(env);
        scene.get_mut(floor).unwrap().casts_shadows = true;
        let frame = crate::shadow_frame::tests::frame(&mut renderer, &scene);
        builder.append_scene(&frame, 8).unwrap();
        let handle = prepare_product_scene(&mut device, &mut builder).unwrap();
        device.draw(handle, &scene_frame_uniforms(&frame)).unwrap();
        let self_shadowed = device.capture().unwrap();
        let acne = before
            .chunks_exact(4)
            .zip(self_shadowed.chunks_exact(4))
            .filter(|(a, b)| a[0] > b[0].saturating_add(15) && a[1] > b[1].saturating_add(15))
            .count();
        assert!(
            acne < 25,
            "{api}: the floor adds {acne} self-shadowed pixels"
        );
        eprintln!("{api}: draws and GPU milliseconds {:?}", timings);
        if let Some(root) = std::env::var_os("RAF_NATIVE_LIGHTING_PROJECT") {
            capture_authored_project(&mut device, std::path::Path::new(&root), api);
        }
    }
}

#[cfg(windows)]
fn capture_authored_project<B: Backend>(
    device: &mut ProductDevice<B>,
    root: &std::path::Path,
    api: raf_agb::backends::native::NativeApi,
) {
    use crate::{
        camera::{Camera, CameraMode},
        scene_renderer::{RenderOptions, SceneRenderer},
    };
    use raf_core::{project::Project, session::ProjectSessionRegistry, SceneGraph};
    let project = Project::load(root).unwrap();
    let registry = ProjectSessionRegistry::load_or_legacy(root, project.project_type);
    let session = registry.active().unwrap();
    let path = session.path(root, &session.scene_file);
    let raw = std::fs::read_to_string(&path).unwrap();
    let scene: SceneGraph = ron::from_str(&raw).unwrap();
    let (id, node) = scene
        .iter()
        .find(|(_, node)| Some(node.uuid) == project.settings.runtime.active_camera)
        .unwrap();
    let lens = node.game_camera.as_ref().unwrap();
    let world = scene.world_matrix(id);
    let position = world.transform_point3(Vec3::ZERO);
    let camera = Camera {
        position,
        target: position - world.transform_vector3(Vec3::Z).normalize(),
        up: world.transform_vector3(Vec3::Y).normalize(),
        mode: if lens.orthographic {
            CameraMode::Orthographic
        } else {
            CameraMode::Perspective
        },
        fov: lens.fov_degrees,
        near: lens.near,
        far: lens.far,
        ortho_scale: lens.ortho_scale,
    };
    let [width, height] = [640, 360];
    device
        .configure([width, height], raf_agb::Output::Capture)
        .unwrap();
    let mut renderer = SceneRenderer::new(width, height);
    renderer.set_asset_root(Some(root.join("assets")));
    let frame = renderer.build_frame(
        &scene,
        &camera,
        width as f32,
        height as f32,
        &[],
        [0, 0, 0, 255],
        Vec3::Y,
        RenderOptions {
            show_grid_3d: false,
            ..RenderOptions::default()
        },
        None,
    );
    let mut builder = ProductFrameBuilder::new(PacketKey(2));
    builder.append_scene(&frame, 1).unwrap();
    let handle = prepare_product_scene(device, &mut builder).unwrap();
    let uniforms = scene_frame_uniforms(&frame);
    let cold = device.draw(handle, &uniforms).unwrap();
    let warm = device.draw(handle, &uniforms).unwrap();
    let mut rgba = device.capture().unwrap();
    // The two capture targets are linear UNORM. PNG viewers expect sRGB.
    for pixel in rgba.chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            *channel = crate::texture::linear_to_srgb(f32::from(*channel) / 255.0);
        }
    }
    let sky_pixels = rgba
        .chunks_exact(4)
        .filter(|p| p[2] > 140 && p[2] > p[0].saturating_add(35))
        .count();
    assert!(
        sky_pixels > (width * height / 8) as usize,
        "{api}: missing configured blue sky"
    );
    eprintln!(
        "{api} {}: {sky_pixels} blue pixels; first {cold:?}; retained {warm:?}",
        project.name
    );
    if let Some(output) = std::env::var_os("RAF_NATIVE_LIGHTING_QA") {
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        image::save_buffer(
            output.join(format!("nowgpu-{api}.png")),
            &rgba,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
}
