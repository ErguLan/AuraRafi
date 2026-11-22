use std::{path::PathBuf, sync::Arc};

use glam::{Mat4, Vec3};
use raf_agb::product::{Command, Pipeline};

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

#[test]
fn scene_commands_keep_painter_order_and_state() {
    let mut commands = BasicCommandList::new();
    commands.clear([10, 20, 30, 255]);
    let mesh_id = commands.register_mesh(triangle_mesh());
    let texture_id = commands.register_texture(texture("albedo", [200, 100, 50, 255]));
    commands.set_pipeline(BasicPipelineKind::PbrLit);
    commands.set_shadow_receiver(false);
    commands.draw_mesh_with_texture(mesh_id, Mat4::IDENTITY, [255, 128, 64, 255], Some(texture_id));
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
        (BasicPipelineKind::FlatColorTwoSided, Pipeline::SCENE),
        (BasicPipelineKind::PbrLit, Pipeline::LIT),
        (BasicPipelineKind::PbrLitTwoSided, Pipeline::LIT),
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
        builder
            .append_scene(&frame(64, 64, commands), 1)
            .unwrap();
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
    builder
        .append_scene(&frame(100, 50, commands), 1)
        .unwrap();
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
    builder
        .append_scene(&frame(64, 64, commands), 1)
        .unwrap();
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

    let second = builder.append_scene(&frame(128, 96, commands.clone()), 7).unwrap();
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
    let error = builder.append_scene(&frame(64, 64, commands), 3).unwrap_err();
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
    let points = vertices.iter().map(|vertex| vertex.position).collect::<Vec<_>>();
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
    builder
        .append_scene(&frame(64, 64, commands), 1)
        .unwrap();
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
fn camera_uniforms_derive_the_eye_and_light_direction() {
    let mut commands = BasicCommandList::new();
    commands.clear([0; 4]);
    let mut scene = frame(64, 64, commands);
    scene.view_proj = Mat4::from_translation(Vec3::new(0.0, 1.0, 4.0)).inverse();
    scene.light_dir = Vec3::new(0.0, 2.0, 0.0);
    let uniforms = scene_frame_uniforms(&scene);
    assert_eq!(uniforms.camera.view_projection, scene.view_proj);
    close(uniforms.camera.world_position.x, 0.0);
    close(uniforms.camera.world_position.y, 1.0);
    close(uniforms.camera.world_position.z, 4.0);
    close(uniforms.light.toward_light.y, 1.0);
}
