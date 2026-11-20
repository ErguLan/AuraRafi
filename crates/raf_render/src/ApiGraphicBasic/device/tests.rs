use super::*;

use crate::api_graphic_basic::capabilities::{GraphicsBackendId, GraphicsMemoryBudget};
use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::scene_renderer::FrameStats;
use glam::{Mat4, Vec3};

use super::frame::extract_clear_color;
use super::resources::slot_capacity;

fn uniforms(value: f32) -> MeshUniforms {
    MeshUniforms {
        mvp: [[value; 4]; 4],
        model: [[value + 1.0; 4]; 4],
        normal_matrix: [[value + 2.0; 4]; 4],
        color: [value + 3.0; 4],
        light_dir: [value + 4.0; 4],
        params: [value + 5.0; 4],
        sun: [value + 6.0; 4],
        ambient: [value + 7.0; 4],
    }
}

#[test]
fn reuses_identical_slot_values_without_upload() {
    let mut reuse = UniformReuse::<MeshUniforms>::default();
    let first = uniforms(1.0);

    assert!(reuse.should_upload(0, first));
    assert!(!reuse.should_upload(0, first));
    assert!(reuse.should_upload(0, uniforms(2.0)));
    assert!(!reuse.should_upload(0, uniforms(2.0)));
}

#[test]
fn keeps_reuse_state_independent_per_draw_slot() {
    let mut reuse = UniformReuse::<MeshUniforms>::default();
    let first = uniforms(1.0);

    assert!(reuse.should_upload(0, first));
    assert!(reuse.should_upload(1, first));
    assert!(!reuse.should_upload(0, first));
    assert!(!reuse.should_upload(1, first));
}

#[test]
fn optimized_modes_restore_pixels_release_filter_resize_and_report_denied_admission() {
    use raf_core::environment::LightingTechnology;
    for gpu in [false, true] {
        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: gpu,
            force_cpu: !gpu,
            ..Default::default()
        });
        if gpu && device.capabilities().backend != GraphicsBackendId::Wgpu {
            assert!(
                std::env::var_os("RAF_REQUIRE_GPU_QA").is_none(),
                "GPU QA required but unavailable"
            );
            eprintln!("Optimized GPU QA SKIPPED: no adapter");
            continue;
        }
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
        assert_ne!(
            baseline, upgraded,
            "improved mode must affect real pixels GPU={gpu}"
        );
        assert_eq!(device.last_frame_metrics().optimized_filter_draw_calls, 1);
        assert_eq!(
            device.last_frame_metrics().optimized_filter_resident_bytes,
            128 * 128 * 4
        );
        device.execute_scene_frame(&improved);
        assert_eq!(upgraded, device.capture_last_scene_rgba().unwrap().rgba8);
        assert_eq!(device.last_frame_metrics().shadow_map_rebuilds, 0);
        assert_eq!(device.last_frame_metrics().shadow_cache_hits, 1);
        // Exercise target replacement while the filter owns the old source binding.
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
        if gpu {
            device
                .gpu_scene
                .as_mut()
                .unwrap()
                .optimized_filter_memory_budget = 1;
        } else {
            device.memory_budget.gpu_bytes = 16;
        }
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
        if gpu {
            assert!(device
                .gpu_scene
                .as_ref()
                .unwrap()
                .optimized_filter
                .is_none());
        } else {
            assert_eq!(device.cpu_filter_scratch.capacity(), 0);
        }
    }
}

#[test]
fn stylized_stars_and_toon_change_real_pixels_on_cpu_and_gpu() {
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
    for gpu in [false, true] {
        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: gpu,
            force_cpu: !gpu,
            ..Default::default()
        });
        if gpu && device.capabilities().backend != GraphicsBackendId::Wgpu {
            eprintln!("Stylized GPU acceptance SKIPPED: no GPU adapter");
            continue;
        }
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
        assert!(count > 20, "GPU={gpu}: star pixels={count}");
        assert_eq!(device.last_frame_metrics().shadow_resident_bytes, 0);
        assert_eq!(stars, render(&mut device, &scene, &sky_camera));
        if gpu {
            assert_eq!(device.last_frame_metrics().uniform_upload_bytes, 0);
            assert_eq!(device.last_frame_metrics().environment_draw_calls, 1);
        }
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
            "GPU={gpu}: toon must affect actual geometry"
        );
        env.sun.toon_enabled = false;
        scene.set_environment(env);
        assert_eq!(smooth, render(&mut device, &scene, &camera));
        eprintln!("Stylized {} acceptance: {count} star pixels, toon changes geometry, disabled restores pixels, uniform reuse checked.", if gpu { "GPU" } else { "CPU" });
    }
}

#[test]
fn shadows_render_real_pixels_reuse_depth_and_respect_receiver_flags_on_both_backends() {
    for gpu in [false, true] {
        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: gpu,
            force_cpu: !gpu,
            ..Default::default()
        });
        if gpu && device.capabilities().backend != GraphicsBackendId::Wgpu {
            eprintln!("Shadows GPU acceptance SKIPPED: no GPU adapter");
            continue;
        }
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
        assert_eq!(second.shadow_draw_calls, 0);
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
                a[0].abs_diff(a[1]) < 3
                    && a[1].abs_diff(a[2]) < 3
                    && a[0] > b[0].saturating_add(15)
            })
            .count();
        assert!(
            darker > 10,
            "backend GPU={gpu}, changed shadow pixels={darker}"
        );
        let acne = lit
            .chunks_exact(4)
            .zip(shadowed.chunks_exact(4))
            .filter(|(a, b)| a[0] > a[1].saturating_add(20) && a[0] > b[0].saturating_add(10))
            .count();
        assert!(acne < 10,"Convex caster must not acquire self-shadow speckles; GPU={gpu}, acne pixels={acne}");
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
        eprintln!("Shadows {} acceptance: {} shadow pixels, {} bytes resident, first prep {:.3} ms, reuse {:.3} ms; GPU timestamp supported={}",if gpu {"GPU"} else {"CPU"},darker,first.shadow_resident_bytes,first.shadow_prepare_cpu_ms,second.shadow_prepare_cpu_ms,first.gpu_timing_supported);
        if let Some(dir) = std::env::var_os("RAF_LIGHTING_QA_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            for (name, pixels) in [
                ("shadows", shadowed),
                ("unshadowed", lit),
                ("filtered", device.capture_last_scene_rgba().unwrap().rgba8),
            ] {
                image::save_buffer(
                    dir.join(format!("{name}-{}.png", if gpu { "gpu" } else { "cpu" })),
                    &pixels,
                    128,
                    128,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        if gpu {
            device.gpu_scene.as_mut().unwrap().shadow_memory_budget = 1;
            env.shadows.resolution = 1024.0;
            scene.set_environment(env);
            let capped = crate::shadow_frame::tests::frame(&mut renderer, &scene);
            device.execute_scene_frame(&capped);
            assert!(device.last_frame_metrics().shadow_budget_exceeded);
            assert!(!device.last_frame_metrics().shadow_active);
            assert_eq!(device.last_frame_metrics().shadow_resident_bytes, 0);
        }
    }
}

#[test]
fn lighting_gpu_pipeline_renders_and_reuses_unchanged_environment_uniforms() {
    let mut device = BasicDevice::new(BasicDeviceConfig::default());
    if device.capabilities().backend != GraphicsBackendId::Wgpu {
        eprintln!(
            "Lighting GPU acceptance skipped: no GPU adapter; CPU coverage runs separately."
        );
        return;
    }
    let mut scene = raf_core::SceneGraph::new();
    scene.add_root_with_primitive("Cube", raf_core::scene::Primitive::Cube);
    let mut env = *scene.environment();
    env.mode = raf_core::environment::EnvironmentMode::Scene;
    env.sun.color = [255, 20, 20];
    env.ambient_intensity = 0.1;
    scene.set_environment(env);
    let mut renderer = crate::scene_renderer::SceneRenderer::new(64, 64);
    let options = crate::scene_renderer::RenderOptions {
        show_grid_3d: false,
        selection_outline: false,
        ..Default::default()
    };
    let frame = renderer.build_frame(
        &scene,
        &crate::camera::Camera::default(),
        64.0,
        64.0,
        &[],
        crate::scene_renderer::SCENE_BACKGROUND,
        crate::scene_renderer::SCENE_LIGHT_DIRECTION,
        options,
        None,
    );
    assert!(matches!(
        device.execute_scene_frame(&frame),
        SceneFrameOutput::GpuTexture { .. }
    ));
    let capture = device.capture_last_scene_rgba().unwrap();
    assert!(capture
        .rgba8
        .chunks_exact(4)
        .any(|p| p[0] > p[1].saturating_add(20)));
    assert_eq!(device.last_frame_metrics().environment_draw_calls, 1);
    device.execute_scene_frame(&frame);
    assert_eq!(device.last_frame_metrics().uniform_upload_bytes, 0);
    let neutral_options = crate::scene_renderer::RenderOptions {
        environment_override: Some(Default::default()),
        ..options
    };
    let frame = renderer.build_frame(
        &scene,
        &crate::camera::Camera::default(),
        64.0,
        64.0,
        &[],
        crate::scene_renderer::SCENE_BACKGROUND,
        crate::scene_renderer::SCENE_LIGHT_DIRECTION,
        neutral_options,
        None,
    );
    device.execute_scene_frame(&frame);
    assert_eq!(device.last_frame_metrics().environment_draw_calls, 0);
    eprintln!("Lighting GPU acceptance passed: hardware frame + readback + uniform reuse + neutral bypass.");
}

#[test]
fn cpu_scene_frame_returns_rgba_pixels() {
    let mut commands = BasicCommandList::new();
    commands.clear([12, 34, 56, 255]);
    let frame = SceneRenderFrame {
        commands,
        view_proj: Mat4::IDENTITY,
        light_dir: Vec3::Y,
        width: 2,
        height: 1,
        texture_cache_budget_bytes: 0,
        stats: FrameStats::default(),
    };

    let mut device = BasicDevice::new(BasicDeviceConfig {
        allow_gpu: false,
        force_cpu: true,
        shared_graphics_context: None,
        ..BasicDeviceConfig::default()
    });
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
    assert_eq!(device.memory_budget(), GraphicsMemoryBudget::potato());
}

#[test]
fn gpu_scene_colors_are_linearized_for_srgb_targets() {
    let color = rgba8_to_f32([9, 12, 16, 255]);
    assert!(color[0] < 9.0 / 255.0);
    assert!(color[1] < 12.0 / 255.0);
    assert!(color[2] < 16.0 / 255.0);

    let mut commands = BasicCommandList::new();
    commands.clear([9, 12, 16, 255]);
    let clear = extract_clear_color(commands.commands());
    assert!((clear.r - f64::from(color[0])).abs() < f64::EPSILON);
    assert!((clear.g - f64::from(color[1])).abs() < f64::EPSILON);
    assert!((clear.b - f64::from(color[2])).abs() < f64::EPSILON);
}

#[test]
fn line_vertex_layout_matches_the_padded_gpu_struct() {
    let layout = GpuLineVertex::desc();
    let offsets = layout
        .attributes
        .iter()
        .map(|attribute| attribute.offset)
        .collect::<Vec<_>>();

    assert_eq!(
        layout.array_stride,
        std::mem::size_of::<GpuLineVertex>() as u64
    );
    assert_eq!(offsets, vec![0, 16, 32, 48, 52]);
    assert_eq!(layout.attributes[4].shader_location, 4);
}

#[test]
fn dynamic_slots_grow_geometrically() {
    assert_eq!(slot_capacity(0), 1);
    assert_eq!(slot_capacity(1), 1);
    assert_eq!(slot_capacity(129), 256);
}

#[test]
fn gpu_line_pipeline_writes_visible_pixels() {
    let mut device = BasicDevice::new(BasicDeviceConfig::default());
    if device.backend() != BasicBackendType::GpuHardware {
        return;
    }

    let mut commands = BasicCommandList::new();
    commands.clear([0, 0, 0, 255]);
    commands.draw_line(
        Vec3::new(-0.65, 0.0, 0.0),
        Vec3::new(0.65, 0.0, 0.0),
        [255, 255, 255, 255],
        4.0,
        true,
        0.0,
    );
    let frame = SceneRenderFrame {
        commands,
        view_proj: Mat4::IDENTITY,
        light_dir: Vec3::Y,
        width: 64,
        height: 64,
        texture_cache_budget_bytes: 0,
        stats: FrameStats::default(),
    };

    let output = device.execute_scene_frame(&frame);
    let SceneFrameOutput::GpuTexture { .. } = output else {
        panic!("GPU backend must return a GPU texture");
    };
    assert!(device.last_frame_metrics().line_draw_calls > 0);

    let gpu_device = device
        .wgpu_device
        .as_ref()
        .expect("GPU device is retained by the GPU backend");
    let gpu_queue = device
        .wgpu_queue
        .as_ref()
        .expect("GPU queue is retained by the GPU backend");
    let target = device
        .gpu_scene
        .as_ref()
        .and_then(|scene| scene.target.as_ref())
        .expect("GPU scene target is created by the frame");

    let readback = gpu_device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ApiGraphicBasic.LineReadback"),
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("ApiGraphicBasic.LineReadbackEncoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &target._color_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(64 * 4),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    gpu_queue.submit(std::iter::once(encoder.finish()));

    let mapped = std::sync::Arc::new(std::sync::Mutex::new(None));
    let mapped_result = mapped.clone();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            *mapped_result.lock().expect("readback callback lock") = Some(result);
        });
    let _ = gpu_device.poll(wgpu::Maintain::Wait);
    assert_eq!(
        mapped
            .lock()
            .expect("readback result lock")
            .take()
            .expect("readback callback must complete"),
        Ok(())
    );

    let pixels = readback.slice(..).get_mapped_range();
    let has_non_black_pixel = pixels
        .chunks_exact(4)
        .any(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8);
    drop(pixels);
    readback.unmap();
    assert!(
        has_non_black_pixel,
        "GPU line pass submitted draws but wrote only the clear color"
    );
}
