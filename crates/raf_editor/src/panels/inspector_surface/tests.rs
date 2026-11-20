use super::*;
use super::runtime::camera_section;
use raf_core::project::ProjectType;
use raf_core::scene::SceneGraph;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
    }

    #[test]
    fn camera_inspector_exposes_only_authored_component_controls() {
        let mut scene = SceneGraph::new();
        let part = scene.add_root_with_primitive("Part", Primitive::Cube);
        let section = camera_section(StudioUiPalette::IndustrialDark, scene.get(part).unwrap());
        assert!(find_node(&section, "inspector.camera.enabled").is_some());
        assert!(find_node(&section, "inspector.camera.startup").is_none());
        assert!(scene.get(part).unwrap().game_camera.is_none());
        scene.get_mut(part).unwrap().game_camera =
            Some(raf_core::runtime_config::GameCamera::default());
        let section = camera_section(StudioUiPalette::IndustrialDark, scene.get(part).unwrap());
        for field in ["fov_degrees", "near", "far", "ortho_scale"] {
            let input = find_node(&section, &format!("inspector.camera.{field}.text")).unwrap();
            assert!(input.focusable);
            assert_eq!(input.kind, UiNodeKind::TextInput);
            let config = input.control.text_input().expect("camera lens input");
            assert_eq!(
                config.submit_command.as_deref(),
                Some(format!("inspector.camera.commit:{field}").as_str())
            );
        }
        assert!(find_node(&section, "inspector.camera.startup").is_some());
        assert_eq!(scene.get(part).unwrap().primitive, Primitive::Cube);
    }

    #[test]
    fn inspector_properties_shows_compact_session_context() {
        let scene = SceneGraph::default();
        let sessions = ProjectSessionRegistry::new(ProjectType::Game);
        let surface = build_inspector_surface(
            StudioUiPalette::IndustrialDark,
            &scene,
            None,
            &sessions,
            1.0,
            InspectorViewState::default(),
        );

        let context = find_node(&surface.root, "inspector.session.context")
            .expect("session context bar should exist in properties");
        assert!(find_node(context, "inspector.session.context.manage").is_some());
    }

    #[test]
    fn inspector_sessions_list_uses_compact_rows() {
        let sessions = ProjectSessionRegistry::new(ProjectType::Game);
        let list = sessions_content(StudioUiPalette::IndustrialDark, &sessions);
        let row = find_node(
            &list,
            &format!("inspector.session.row.{}", sessions.active_session.0),
        )
        .expect("active session row should exist");

        assert_eq!(row.layout.basis, [0.0, 28.0]);
    }

    #[test]
    fn texture_dropdown_lists_only_safe_supported_project_images() {
        let mut scene = SceneGraph::new();
        let selected = scene.add_root_with_primitive("Textured", raf_core::scene::Primitive::Cube);
        let sessions = ProjectSessionRegistry::new(ProjectType::Game);
        let assets = vec![
            "textures/albedo.png".to_string(),
            "notes.txt".to_string(),
            "../outside.png".to_string(),
        ];
        let surface = build_inspector_surface_with_assets(
            StudioUiPalette::IndustrialDark,
            &scene,
            Some(selected),
            &sessions,
            1.0,
            InspectorViewState {
                dropdown: Some(InspectorDropdown::Texture),
                ..InspectorViewState::default()
            },
            DisplayUnit::Metric,
            &assets,
        );

        assert!(find_node(
            &surface.root,
            "inspector.base-color-texture.option.textures/albedo.png"
        )
        .is_some());
        assert!(find_node(
            &surface.root,
            "inspector.base-color-texture.option.notes.txt"
        )
        .is_none());
        assert!(find_node(
            &surface.root,
            "inspector.base-color-texture.option.../outside.png"
        )
        .is_none());
    }
