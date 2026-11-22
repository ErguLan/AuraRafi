use super::*;

use crate::api_graphic_basic::capabilities::{GraphicsBackendId, GraphicsMemoryBudget};
use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::scene_renderer::FrameStats;
use glam::{Mat4, Vec3};

fn blank_frame(commands: BasicCommandList, width: u32, height: u32) -> SceneRenderFrame {
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

fn cpu_device() -> BasicDevice {
    BasicDevice::new(BasicDeviceConfig {
        allow_gpu: false,
        force_cpu: true,
        ..BasicDeviceConfig::default()
    })
}

fn native_device() -> Option<(NativeApi, BasicDevice)> {
    let api = preferred_native_api()?;
    let device = BasicDevice::new(BasicDeviceConfig {
        allow_gpu: true,
        force_cpu: false,
        ..BasicDeviceConfig::default()
    });
    Some((api, device))
}

#[test]
fn cpu_scene_frame_returns_rgba_pixels() {
    let mut commands = BasicCommandList::new();
    commands.clear([12, 34, 56, 255]);
    let frame = blank_frame(commands, 2, 1);

    let mut device = cpu_device();
    assert!(device.capture_last_scene_rgba().is_err());
    let output = device.execute_scene_frame(&frame);
    let SceneFrameOutput::CpuPixels(pixels) = output else {
        panic!("expected cpu pixel output");
    };

    assert_eq!(pixels.len(), 8);
    assert_eq!(pixels[0], 12);
    assert_eq!(pixels[1], 34);
    assert_eq!(pixels[2], 56);
    assert_eq!(pixels[3], 255);
    let capture = device
        .capture_last_scene_rgba()
        .expect("rendered CPU frame should be capturable");
    assert_eq!((capture.width, capture.height), (2, 1));
    assert_eq!(capture.rgba8, pixels);
    assert_eq!(
        device.capabilities().backend,
        GraphicsBackendId::CpuSoftware
    );
    assert!(!device.is_gpu_active());
    assert_eq!(device.native_api(), None);
    assert_eq!(device.memory_budget(), GraphicsMemoryBudget::potato());
}

#[test]
fn backend_labels_and_ids_are_stable() {
    assert_eq!(BasicBackendType::CpuSoftware.label(), "cpu_software");
    assert_eq!(
        BasicBackendType::CpuSoftware.id(),
        GraphicsBackendId::CpuSoftware
    );
    assert!(!BasicBackendType::CpuSoftware.is_native());
    assert!(!BasicBackendType::CpuSoftware.is_gpu_active());
    assert_eq!(
        BasicBackendType::CpuSoftware.capabilities().backend,
        GraphicsBackendId::CpuSoftware
    );

    for (api, label, id) in [
        (NativeApi::Vulkan, "vulkan", GraphicsBackendId::Vulkan),
        (NativeApi::Dx12, "dx12", GraphicsBackendId::DirectX12),
        (NativeApi::Metal, "metal", GraphicsBackendId::Metal),
    ] {
        let backend = BasicBackendType::Native(api);
        assert_eq!(backend.label(), label);
        assert_eq!(backend.id(), id);
        assert!(backend.is_native());
        assert!(backend.is_gpu_active());
        assert!(backend.capabilities().gpu_hardware);
        assert_eq!(backend.capabilities().backend, id);
    }

    // The serialized label round-trips so diagnostics keep one spelling.
    let encoded = serde_json::to_string(&BasicBackendType::Native(NativeApi::Vulkan)).unwrap();
    assert_eq!(encoded, "\"vulkan\"");
    let decoded: BasicBackendType = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, BasicBackendType::Native(NativeApi::Vulkan));
}

#[test]
fn native_handoff_returns_the_frame_without_upload() {
    let Some((api, mut device)) = native_device() else {
        eprintln!("Native handoff test skipped: no native executor in this build.");
        return;
    };
    assert_eq!(device.backend(), BasicBackendType::Native(api));
    assert_eq!(device.native_api(), Some(api));
    assert!(device.is_gpu_active());
    assert_eq!(
        device.capabilities().backend,
        BasicBackendType::Native(api).id()
    );

    let mut commands = BasicCommandList::new();
    commands.clear([5, 6, 7, 255]);
    let frame = blank_frame(commands, 8, 4);

    let output = device.execute_scene_frame(&frame);
    let SceneFrameOutput::Scene(retained) = output else {
        panic!("native backend must hand over the retained frame");
    };
    assert_eq!((retained.width, retained.height), (8, 4));
    assert_eq!(retained.view_proj, frame.view_proj);
    assert_eq!(retained.light_dir, frame.light_dir);
    assert_eq!(
        retained.commands.commands().len(),
        frame.commands.commands().len()
    );

    let metrics = device.last_frame_metrics();
    assert_eq!(metrics.total_upload_bytes(), 0);
    assert_eq!(metrics.total_draw_calls(), 0);
    assert_eq!(metrics.command_count, 1);

    // A second handoff of equal work keeps the same zero-work contract.
    let second = device.execute_scene_frame(&frame);
    let SceneFrameOutput::Scene(second) = second else {
        panic!("native backend must keep handing over frames");
    };
    assert_eq!(second.width, retained.width);
    assert_eq!(device.last_frame_metrics().total_upload_bytes(), 0);
}

#[test]
fn capture_after_native_handoff_uses_the_cpu_rasterizer() {
    let Some((_, mut device)) = native_device() else {
        eprintln!("Native capture test skipped: no native executor in this build.");
        return;
    };
    assert!(
        device.capture_last_scene_rgba().is_err(),
        "capture must fail before any frame executes"
    );

    let mut commands = BasicCommandList::new();
    commands.clear([12, 34, 56, 255]);
    let frame = blank_frame(commands, 2, 1);
    device.execute_scene_frame(&frame);

    let capture = device
        .capture_last_scene_rgba()
        .expect("native handoff must be inspectable through the CPU rasterizer");
    assert_eq!((capture.width, capture.height), (2, 1));
    assert_eq!(capture.rgba8, vec![12, 34, 56, 255, 12, 34, 56, 255]);
    assert_eq!(
        capture,
        device
            .capture_last_scene_rgba()
            .expect("repeat capture stays stable")
    );
}

#[test]
fn native_handoff_pixels_match_cpu_recovery() {
    let Some((_, mut device)) = native_device() else {
        eprintln!("Native parity test skipped: no native executor in this build.");
        return;
    };
    let mut commands = BasicCommandList::new();
    commands.clear([90, 40, 10, 255]);
    commands.draw_line(Vec3::ZERO, Vec3::X, [255, 255, 255, 255], 2.0, true, 0.0);
    let frame = blank_frame(commands, 16, 16);

    device.execute_scene_frame(&frame);
    let native_capture = device.capture_last_scene_rgba().unwrap();

    let mut cpu = cpu_device();
    let SceneFrameOutput::CpuPixels(pixels) = cpu.execute_scene_frame(&frame) else {
        panic!("CPU device must return pixels");
    };
    assert_eq!(cpu_capture(&frame, cpu.memory_budget()).rgba8, pixels);
    assert_eq!(native_capture.rgba8, pixels);
}

fn cpu_capture(frame: &SceneRenderFrame, budget: GraphicsMemoryBudget) -> SceneFrameCapture {
    capture_scene_frame_cpu(frame, budget).unwrap()
}

#[test]
fn standalone_capture_matches_executed_cpu_pixels() {
    let mut device = cpu_device();
    let mut commands = BasicCommandList::new();
    commands.clear([7, 17, 27, 255]);
    let frame = blank_frame(commands, 4, 2);

    let SceneFrameOutput::CpuPixels(pixels) = device.execute_scene_frame(&frame) else {
        panic!("CPU device must return pixels");
    };
    let capture = capture_scene_frame_cpu(&frame, device.memory_budget()).unwrap();
    assert_eq!((capture.width, capture.height), (4, 2));
    assert_eq!(capture.rgba8, pixels);
}

#[test]
fn optimized_modes_restore_pixels_release_filter_resize_and_report_denied_admission() {
    use raf_core::environment::LightingTechnology;
    let mut device = cpu_device();
    let (mut scene, _, _) = crate::shadow_frame::tests::fixture();
    let mut renderer = crate::scene_renderer::SceneRenderer::new(128, 128);
    let current = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&current);
    let baseline = device.capture_last_scene_rgba().unwrap().rgba8;
    assert_eq!(
        device.last_frame_metrics().optimized_filter_resident_bytes,
        0
    );
    let mut env = *scene.environment();
    env.technology = LightingTechnology::OptimizedImproved;
    env.shadows.soft_filter = true;
    scene.set_environment(env);
    let improved = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&improved);
    let upgraded = device.capture_last_scene_rgba().unwrap().rgba8;
    assert_ne!(baseline, upgraded, "improved mode must affect real pixels");
    assert_eq!(device.last_frame_metrics().optimized_filter_draw_calls, 1);
    assert_eq!(
        device.last_frame_metrics().optimized_filter_resident_bytes,
        128 * 128 * 4
    );
    device.execute_scene_frame(&improved);
    assert_eq!(upgraded, device.capture_last_scene_rgba().unwrap().rgba8);
    assert_eq!(device.last_frame_metrics().shadow_map_rebuilds, 0);
    assert_eq!(device.last_frame_metrics().shadow_cache_hits, 1);
    // Exercise target replacement while the filter owns the previous size.
    let mut resized = improved.clone();
    resized.width = 96;
    resized.height = 64;
    device.execute_scene_frame(&resized);
    let captured = device.capture_last_scene_rgba().unwrap();
    assert_eq!(
        (captured.width, captured.height, captured.rgba8.len()),
        (96, 64, 96 * 64 * 4)
    );
    assert_eq!(
        device.last_frame_metrics().optimized_filter_resident_bytes,
        96 * 64 * 4
    );
    device.memory_budget.gpu_bytes = 16;
    device.execute_scene_frame(&improved);
    assert!(device.last_frame_metrics().optimized_filter_budget_exceeded);
    assert_eq!(
        device.last_frame_metrics().optimized_filter_resident_bytes,
        0
    );
    device.execute_scene_frame(&current);
    assert_eq!(baseline, device.capture_last_scene_rgba().unwrap().rgba8);
    assert_eq!(device.last_frame_metrics().optimized_filter_draw_calls, 0);
    assert!(!device.last_frame_metrics().optimized_filter_budget_exceeded);
    assert_eq!(device.cpu_filter_scratch.capacity(), 0);
}

#[test]
fn stylized_stars_and_toon_change_real_pixels_on_cpu() {
    use crate::{
        camera::Camera,
        scene_renderer::{
            RenderOptions, SceneRenderer, SCENE_BACKGROUND, SCENE_LIGHT_DIRECTION,
        },
    };
    let options = RenderOptions {
        show_grid_3d: false,
        selection_outline: false,
        ..Default::default()
    };
    let mut device = cpu_device();
    let mut scene = raf_core::SceneGraph::new();
    let mut env = *scene.environment();
    env.mode = raf_core::environment::EnvironmentMode::Scene;
    env.sun.disk_visible = false;
    env.sky.stars_density = 1.0;
    let mut renderer = SceneRenderer::new(192, 192);
    let sky_camera = Camera {
        position: Vec3::ZERO,
        target: Vec3::Y,
        up: Vec3::Z,
        ..Default::default()
    };
    let mut render =
        |device: &mut BasicDevice, scene: &raf_core::SceneGraph, camera: &Camera| {
            let frame = renderer.build_frame(
                scene,
                camera,
                192.0,
                192.0,
                &[],
                SCENE_BACKGROUND,
                SCENE_LIGHT_DIRECTION,
                options,
                None,
            );
            device.execute_scene_frame(&frame);
            device.capture_last_scene_rgba().unwrap().rgba8
        };
    scene.set_environment(env);
    let no_stars = render(&mut device, &scene, &sky_camera);
    env.sky.stars_enabled = true;
    scene.set_environment(env);
    let stars = render(&mut device, &scene, &sky_camera);
    let count = no_stars
        .chunks_exact(4)
        .zip(stars.chunks_exact(4))
        .filter(|(a, b)| b[0] > a[0].saturating_add(10))
        .count();
    assert!(count > 20, "star pixels={count}");
    assert_eq!(device.last_frame_metrics().shadow_resident_bytes, 0);
    assert_eq!(stars, render(&mut device, &scene, &sky_camera));
    env.sky.stars_enabled = false;
    scene.set_environment(env);
    assert_eq!(no_stars, render(&mut device, &scene, &sky_camera));
    scene.add_root_with_primitive("Sphere", raf_core::scene::Primitive::Sphere);
    let camera = Camera {
        position: Vec3::new(1.3, 1.0, 1.7),
        ..Default::default()
    };
    let smooth = render(&mut device, &scene, &camera);
    env.sun.toon_enabled = true;
    env.sun.toon_steps = 2.0;
    scene.set_environment(env);
    let toon = render(&mut device, &scene, &camera);
    assert!(
        smooth
            .chunks_exact(4)
            .zip(toon.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
            > 20,
        "toon must affect actual geometry"
    );
    env.sun.toon_enabled = false;
    scene.set_environment(env);
    assert_eq!(smooth, render(&mut device, &scene, &camera));
    eprintln!(
        "Stylized CPU acceptance: {count} star pixels, toon changes geometry, disabled restores pixels."
    );
}

#[test]
fn shadows_render_real_pixels_reuse_depth_and_respect_receiver_flags_on_cpu() {
    let mut device = cpu_device();
    let (mut scene, floor, cube) = crate::shadow_frame::tests::fixture();
    let mut renderer = crate::scene_renderer::SceneRenderer::new(128, 128);
    let f = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&f);
    let shadowed = device.capture_last_scene_rgba().unwrap().rgba8;
    let first = device.last_frame_metrics();
    assert!(first.shadow_active);
    assert_eq!(first.shadow_map_rebuilds, 1);
    assert!(first.shadow_resident_bytes > 0);
    device.execute_scene_frame(&f);
    let second = device.last_frame_metrics();
    assert_eq!(second.shadow_map_rebuilds, 0);
    assert_eq!(second.shadow_cache_hits, 1);
    let mut env = *scene.environment();
    env.shadows.enabled = false;
    scene.set_environment(env);
    let disabled = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&disabled);
    let lit = device.capture_last_scene_rgba().unwrap().rgba8;
    let darker = lit
        .chunks_exact(4)
        .zip(shadowed.chunks_exact(4))
        .filter(|(a, b)| {
            a[0].abs_diff(a[1]) < 3 && a[1].abs_diff(a[2]) < 3 && a[0] > b[0].saturating_add(15)
        })
        .count();
    assert!(darker > 10, "changed shadow pixels={darker}");
    let acne = lit
        .chunks_exact(4)
        .zip(shadowed.chunks_exact(4))
        .filter(|(a, b)| a[0] > a[1].saturating_add(20) && a[0] > b[0].saturating_add(10))
        .count();
    assert!(
        acne < 10,
        "Convex caster must not acquire self-shadow speckles; acne pixels={acne}"
    );
    assert_eq!(device.last_frame_metrics().shadow_resident_bytes, 0);
    scene.get_mut(floor).unwrap().receives_shadows = false;
    scene.get_mut(cube).unwrap().receives_shadows = false;
    env.shadows.enabled = true;
    scene.set_environment(env);
    let non_receivers = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&non_receivers);
    assert_eq!(lit, device.capture_last_scene_rgba().unwrap().rgba8);
    scene.get_mut(floor).unwrap().receives_shadows = true;
    env.shadows.soft_filter = true;
    env.shadows.bias = 0.03;
    scene.set_environment(env);
    let filtered = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&filtered);
    assert_eq!(device.last_frame_metrics().shadow_map_rebuilds, 0);
    assert_eq!(device.last_frame_metrics().shadow_cache_hits, 1);
    scene.get_mut(cube).unwrap().position.x += 0.5;
    let moved = crate::shadow_frame::tests::frame(&mut renderer, &scene);
    device.execute_scene_frame(&moved);
    assert_eq!(device.last_frame_metrics().shadow_map_rebuilds, 1);
    eprintln!(
        "Shadows CPU acceptance: {darker} shadow pixels, {} bytes resident, first prep {:.3} ms, reuse {:.3} ms",
        first.shadow_resident_bytes, first.shadow_prepare_cpu_ms, second.shadow_prepare_cpu_ms
    );
    if let Some(dir) = std::env::var_os("RAF_LIGHTING_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, pixels) in [
            ("shadows", shadowed),
            ("unshadowed", lit),
            ("filtered", device.capture_last_scene_rgba().unwrap().rgba8),
        ] {
            image::save_buffer(
                dir.join(format!("{name}-cpu.png")),
                &pixels,
                128,
                128,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
