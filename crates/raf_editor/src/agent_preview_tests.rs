use super::*;
use raf_core::{config::RenderExecutionPolicy, scene::Primitive, ProjectType};
use raf_render::api_graphic_basic::device::SceneFrameOutput;

fn fixture() -> (Project, PathBuf, SceneGraph, PreviewTarget) {
    let directory =
        std::env::temp_dir().join(format!("raf-agent-preview-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let project = Project::create("Preview", ProjectType::Game, &directory).unwrap();
    let mut scene = SceneGraph::new();
    scene.add_root_with_primitive("Visible cube", Primitive::Cube);
    let target = PreviewTarget {
        root: project.path.clone(),
        document: SessionId(uuid::Uuid::new_v4()),
    };
    (project, directory, scene, target)
}
fn cpu_settings() -> EngineSettings {
    EngineSettings {
        render_execution_policy: RenderExecutionPolicy::CpuOnly,
        ..Default::default()
    }
}

#[test]
fn agent_workspace_preview_is_lazy_cached_bounded_and_readonly() {
    let (project, directory, scene, target) = fixture();
    let before = scene.document_revision();
    let project_before = ron::to_string(&project).unwrap();
    let mut preview = AgentPreview::default();
    preview.sync(Some(target.clone()), false, Instant::now());
    assert!(!preview.resident());
    assert!(preview.deadline().is_none());
    assert!(preview
        .render_layer(
            &scene,
            &project,
            &cpu_settings(),
            CanvasTargetRect::full([1600, 900]),
            || panic!("hidden preview allocated graphics")
        )
        .is_none());
    preview.sync(Some(target), true, Instant::now());
    assert!(
        !preview.resident(),
        "visibility alone does not allocate a renderer"
    );
    let layer = preview
        .render_layer(
            &scene,
            &project,
            &cpu_settings(),
            CanvasTargetRect::full([1600, 900]),
            || None,
        )
        .unwrap();
    assert!(layer.source_size[0] <= 480 && layer.source_size[1] <= 360);
    match layer.output.as_ref() {
        SceneFrameOutput::CpuPixels(pixels) => {
            assert!(
                pixels.chunks_exact(4).any(|p| p != &pixels[..4]),
                "a real scene must be rendered, not an empty placeholder"
            );
            if let Some(directory) = std::env::var_os("RAF_AGENT_UI_QA_DIR") {
                std::fs::create_dir_all(&directory).unwrap();
                image::save_buffer(
                    PathBuf::from(directory).join("agents-preview-scene.png"),
                    pixels,
                    layer.source_size[0],
                    layer.source_size[1],
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        _ => panic!("CPU fixture selected a GPU"),
    }
    for _ in 0..120 {
        let cached = preview
            .render_layer(
                &scene,
                &project,
                &cpu_settings(),
                CanvasTargetRect::full([1600, 900]),
                || panic!("cached frame recreated context"),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&cached.output, &layer.output));
    }
    assert_eq!(preview.rendered_frames, 1);
    assert!(!preview.scene_changed(&scene));
    assert_eq!(scene.document_revision(), before);
    assert_eq!(ron::to_string(&project).unwrap(), project_before);
    let now = Instant::now();
    preview.sync(preview.target.clone(), false, now);
    assert!(!preview.expire(now + Duration::from_secs(4)));
    assert!(preview.resident());
    assert!(preview.expire(now + PREVIEW_TTL));
    assert!(!preview.resident());
    assert!(preview.deadline().is_none());
    drop(layer);
    drop(preview);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn agent_workspace_preview_target_switch_and_exit_drop_resources_immediately() {
    let (project, directory, scene, target) = fixture();
    let mut preview = AgentPreview::default();
    preview.sync(Some(target.clone()), true, Instant::now());
    preview
        .render_layer(
            &scene,
            &project,
            &cpu_settings(),
            CanvasTargetRect::full([320, 240]),
            || None,
        )
        .unwrap();
    let other = PreviewTarget {
        document: SessionId(uuid::Uuid::new_v4()),
        ..target
    };
    assert!(preview.sync(Some(other.clone()), true, Instant::now()));
    assert!(!preview.resident());
    preview
        .render_layer(
            &scene,
            &project,
            &cpu_settings(),
            CanvasTargetRect::full([320, 240]),
            || None,
        )
        .unwrap();
    assert_eq!(preview.target, Some(other));
    assert!(preview.close());
    assert!(preview.target.is_none() && !preview.visible && !preview.resident());
    assert!(preview.deadline().is_none());
    assert!(!preview.close());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn agent_workspace_preview_camera_never_consumes_typing_or_other_keyboard_owners() {
    let mut preview = AgentPreview::default();
    preview.sync(
        Some(PreviewTarget {
            root: PathBuf::from("unused"),
            document: SessionId(uuid::Uuid::new_v4()),
        }),
        true,
        Instant::now(),
    );
    let mut input = InputSnapshot::default();
    input.pointer_position = Some([80.0, 80.0]);
    input.keys_down.insert(InputKey::W);
    input.delta_seconds = 1.0 / 60.0;
    let rect = UiRect::new(0.0, 0.0, 300.0, 300.0);
    let mut router = InputRouter::default();
    let camera_before = preview.camera.target;
    assert!(!preview.input(&input, &mut router, rect));
    assert_eq!(preview.camera.target, camera_before);
    let text = InputOwner::Custom(InputRegionId::from_static("test.text"));
    assert!(router.try_capture_keyboard(text));
    input
        .pointer_pressed_buttons
        .insert(PointerButton::Secondary);
    input.pointer_buttons_down.insert(PointerButton::Secondary);
    assert!(!preview.input(&input, &mut router, rect));
    assert_eq!(preview.camera.target, camera_before);
    assert_eq!(router.keyboard_owner(), Some(text));
    preview.cancel_input(&mut router);
    router.release_keyboard(text);
    preview.input(&input, &mut router, rect);
    assert_ne!(preview.camera.target, camera_before);
    input.window_focused = false;
    preview.input(&input, &mut router, rect);
    assert!(!preview.navigating && !preview.moving);
    assert!(!router.has_pointer_capture());
    assert!(router.keyboard_owner().is_none());
}
