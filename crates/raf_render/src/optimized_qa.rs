//! Opt-in, read-only A/B capture of real authored projects. No Play or script execution.
use crate::{
    api_graphic_basic::{
        device::{BasicDevice, BasicDeviceConfig},
        GraphicsBackendId,
    },
    camera::{Camera, CameraMode},
    scene_renderer::{RenderOptions, SceneRenderer, SCENE_BACKGROUND, SCENE_LIGHT_DIRECTION},
};
use glam::Vec3;
use raf_core::{
    environment::LightingTechnology, project::Project, session::ProjectSessionRegistry, SceneGraph,
};

/// RAF_LIGHTING_PROJECTS is a semicolon-separated list of project directories.
/// Artifacts require RAF_LIGHTING_QA_DIR. Neither project nor registry is saved.
#[test]
fn optimized_project_ab_read_only_captures() {
    let Some(projects) = std::env::var_os("RAF_LIGHTING_PROJECTS") else {
        return;
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("RAF_LIGHTING_QA_DIR").expect("QA output directory required"),
    );
    std::fs::create_dir_all(&output).unwrap();
    for (index, root) in projects
        .to_string_lossy()
        .split(';')
        .filter(|s| !s.is_empty())
        .enumerate()
    {
        let root = std::path::Path::new(root);
        let project = Project::load(root).expect("read project metadata");
        let registry_path = root.join(ProjectSessionRegistry::FILE_NAME);
        let registry: ProjectSessionRegistry =
            ron::from_str(&std::fs::read_to_string(registry_path).unwrap()).unwrap();
        let session = registry.active().unwrap();
        let scene_path = session.path(root, &session.scene_file);
        let raw = std::fs::read_to_string(&scene_path).unwrap();
        let mut scene: SceneGraph = ron::from_str(&raw).expect("strict authored scene decode");
        let authored = scene
            .iter()
            .find(|(_, n)| {
                n.game_camera.is_some() && Some(n.uuid) == project.settings.runtime.active_camera
            })
            .or_else(|| scene.iter().find(|(_, n)| n.game_camera.is_some()));
        let (camera, camera_source) = if let Some((id, node)) = authored {
            let lens = node.game_camera.as_ref().unwrap();
            let matrix = scene.world_matrix(id);
            let position = matrix.transform_point3(Vec3::ZERO);
            (
                Camera {
                    position,
                    target: position + matrix.transform_vector3(-Vec3::Z).normalize(),
                    up: matrix.transform_vector3(Vec3::Y).normalize(),
                    mode: if lens.orthographic {
                        CameraMode::Orthographic
                    } else {
                        CameraMode::Perspective
                    },
                    fov: lens.fov_degrees,
                    near: lens.near,
                    far: lens.far,
                    ortho_scale: lens.ortho_scale,
                },
                "authored game camera",
            )
        } else {
            (Camera::default(), "QA default camera (no authored camera)")
        };
        eprintln!("A/B project={} session={} camera={} env={:?} shadow={}@{} distance={} soft={} resolution={:?} texture={:?}",project.name,session.name,camera_source,scene.environment().mode,scene.environment().shadows.enabled,scene.environment().shadows.resolution,scene.environment().shadows.distance,scene.environment().shadows.soft_filter,project.settings.viewport_resolution,project.settings.texture_quality);
        let saved = *scene.environment();
        for gpu in [false, true] {
            let mut device = BasicDevice::new(BasicDeviceConfig {
                allow_gpu: gpu,
                force_cpu: !gpu,
                ..Default::default()
            });
            if gpu && device.capabilities().backend != GraphicsBackendId::Wgpu {
                assert!(
                    std::env::var_os("RAF_REQUIRE_GPU_QA").is_none(),
                    "GPU required but unavailable"
                );
                eprintln!("Project A/B GPU SKIPPED: no adapter");
                continue;
            }
            let mut renderer = SceneRenderer::new(720, 450);
            renderer.set_asset_root(Some(root.join("assets")));
            for (technology, label) in [
                (LightingTechnology::OptimizedCurrent, "current"),
                (LightingTechnology::OptimizedImproved, "improved"),
            ] {
                let mut environment = saved;
                environment.technology = technology;
                scene.set_environment(environment);
                let mut cpu_times = Vec::new();
                let mut gpu_times = Vec::new();
                for sample in 0..12 {
                    let frame = renderer.build_frame(
                        &scene,
                        &camera,
                        720.0,
                        450.0,
                        &[],
                        SCENE_BACKGROUND,
                        SCENE_LIGHT_DIRECTION,
                        RenderOptions {
                            show_grid_3d: false,
                            selection_outline: false,
                            texture_quality: project.settings.texture_quality,
                            geometry_detail: project.settings.geometry_detail,
                            ..Default::default()
                        },
                        None,
                    );
                    device.execute_scene_frame(&frame);
                    let capture = device
                        .capture_last_scene_rgba()
                        .expect("actual rendered capture");
                    let metrics = device.last_frame_metrics();
                    if sample >= 2 {
                        cpu_times.push(metrics.frame_cpu_ms);
                        if metrics.gpu_timing_sampled {
                            gpu_times.push(metrics.gpu_frame_ms);
                        }
                    }
                    if sample == 11 {
                        let backend = if gpu { "gpu" } else { "cpu" };
                        image::save_buffer(
                            output.join(format!("project-{index}-{backend}-{label}.png")),
                            &capture.rgba8,
                            capture.width,
                            capture.height,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                        eprintln!("A/B {index} {backend} {label}: filter={} bytes={} shadow={} resolution={} shadow_bytes={} cache={} budget_exceeded={}",metrics.optimized_filter_draw_calls,metrics.optimized_filter_resident_bytes,metrics.shadow_active,metrics.shadow_resolution,metrics.shadow_resident_bytes,metrics.shadow_cache_hits,metrics.optimized_filter_budget_exceeded||metrics.shadow_budget_exceeded);
                    }
                }
                cpu_times.sort_by(f32::total_cmp);
                gpu_times.sort_by(f32::total_cmp);
                let backend = if gpu { "gpu" } else { "cpu" };
                let metrics = device.last_frame_metrics();
                let report = serde_json::json!({
                    "project":project.name,"session":session.name,"camera_source":camera_source,
                    "camera":camera,"backend":backend,"technology":technology,
                    "size":[720,450],"environment":environment,
                    "cpu_frame_ms_sorted":cpu_times,"gpu_frame_ms_sorted":gpu_times,
                    "filter_draw_calls":metrics.optimized_filter_draw_calls,
                    "filter_rgba8_bytes":metrics.optimized_filter_resident_bytes,
                    "filter_budget_exceeded":metrics.optimized_filter_budget_exceeded,
                    "shadow_resolution":metrics.shadow_resolution,
                    "shadow_resident_bytes":metrics.shadow_resident_bytes,
                    "shadow_cache_hits":metrics.shadow_cache_hits,
                    "acceptance":"debug offscreen only; no Play, native-window or low-end guarantee"
                });
                std::fs::write(
                    output.join(format!("project-{index}-{backend}-{label}.json")),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
                eprintln!("A/B {index} gpu={gpu} {label}: warm CPU median={:.3}ms P95={:.3}ms; GPU samples={gpu_times:?}. Debug offscreen only; not native/low-end acceptance.",cpu_times[cpu_times.len()/2],cpu_times[cpu_times.len()-1]);
            }
        }
        assert_eq!(
            std::fs::read_to_string(&scene_path).unwrap(),
            raw,
            "QA must never save authored scenes"
        );
    }
}
