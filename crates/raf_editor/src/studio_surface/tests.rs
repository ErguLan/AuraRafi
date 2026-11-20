use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    UiAction, UiInputState, UiNodeKind, UiSurfaceSession,
};
use raf_ui::UiIconId;
use std::path::PathBuf;

use super::*;

mod project_opening_tests {
    use super::super::*;
    #[test]
    fn agent_workspace_hub_loading_is_not_an_agents_surface() {
        let mut model = HubSurfaceModel {
            opening_project: Some("Game".into()),
            opening_size: [900.0, 700.0],
            ..Default::default()
        };
        for failed in [false, true] {
            model.opening_error = failed.then(|| "agent_workspace.project_busy".into());
            let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
            let mut session =
                raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
            let frame =
                session.build_frame_with_resolved_text(&surface, 900, 700, [0, 0, 0, 255], |key| {
                    crate::native_studio::resolve_hub_text(key, &model, raf_core::Language::Spanish)
                });
            assert!(frame
                .layout_boxes
                .iter()
                .any(|item| item.id == "hub.open.overlay"));
            assert!(frame
                .layout_boxes
                .iter()
                .any(|item| item.id == "hub.open.cancel"));
            assert_eq!(
                frame
                    .layout_boxes
                    .iter()
                    .any(|item| item.id == "hub.open.retry"),
                failed
            );
            assert!(!frame
                .layout_boxes
                .iter()
                .any(|item| item.id == "agent.workspace.root"));
        }
    }
}

#[test]
fn rafui_regression_hub_type_picker_uses_a_registered_semantic_chevron() {
    let picker =
        hub_create_type_picker(StudioUiPalette::IndustrialDark, &HubSurfaceModel::default());
    let chevron = picker.find("hub.create.type-chevron").unwrap();
    assert_eq!(
        chevron.icon.map(|icon| icon.id),
        Some(UiIconId::ChevronDown)
    );
    assert_eq!(chevron.control, raf_ui::UiControl::None);
}

#[test]
fn studio_surface_has_fixed_docks_and_a_growing_canvas() {
    let surface = build_studio_surface(StudioUiPalette::IndustrialDark);
    let frame = surface.build_frame(1440, 900, [8, 8, 8, 255]);

    let viewport = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "studio.viewport")
        .expect("viewport box");
    let hierarchy = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "studio.hierarchy")
        .expect("hierarchy box");
    let inspector = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "studio.inspector")
        .expect("inspector box");

    assert_eq!(hierarchy.rect.width, 230.0);
    assert_eq!(inspector.rect.width, 300.0);
    assert!(viewport.rect.width > 700.0);
    assert!(frame
        .hit_regions
        .iter()
        .any(|region| region.id == "studio.run"));
}

#[test]
fn hub_surface_uses_creation_and_recent_controls() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let frame = surface.build_frame(520, 720, [8, 8, 8, 255]);

    assert!(frame
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "hub.create-form"));
    assert!(frame
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "hub.recent"));
}

#[test]
fn hub_places_creation_before_showcase_and_recent_content() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let frame = surface.build_frame(1_000, 760, [8, 11, 15, 255]);

    let header = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.header")
        .expect("workspace header");
    let create = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create-form")
        .expect("create panel");
    let showcase = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.showcase")
        .expect("showcase panel");
    let recent = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.recent")
        .expect("recent panel");

    assert!(create.rect.y >= header.rect.bottom());
    assert!(showcase.rect.y >= create.rect.bottom());
    assert!(recent.rect.y >= showcase.rect.bottom());
}

#[test]
fn hub_centers_workspace_at_its_maximum_content_width() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let frame = surface.build_frame(1_920, 1_080, [8, 11, 15, 255]);
    let workspace = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.main-column")
        .expect("workspace column");

    assert_eq!(workspace.rect.width, 1_280.0);
    assert_eq!(workspace.rect.x, 320.0);
}

#[test]
fn hub_project_cards_expose_a_visible_hover_state() {
    let project = HubSurfaceProject {
        name: "Demo".to_string(),
        path: PathBuf::from("C:/Projects/Demo"),
        project_type: ProjectType::Game,
        last_opened_label: "14/07/2026".to_string(),
    };
    let electronics = HubSurfaceProject {
        name: "Board".to_string(),
        path: PathBuf::from("C:/Projects/Board"),
        project_type: ProjectType::Electronics,
        last_opened_label: "13/07/2026".to_string(),
    };
    let model = HubSurfaceModel {
        projects: vec![project.clone(), electronics.clone()],
        featured_project: Some(project.clone()),
        recent_activity: vec![project, electronics.clone(), electronics],
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
    let mut session = UiSurfaceSession::default();
    let frame = session.build_frame(&surface, 1_300, 900, [8, 11, 15, 255]);
    let small = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.showcase-small.0")
        .expect("showcase card");

    let _ = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some([
                small.rect.x + small.rect.width * 0.5,
                small.rect.y + small.rect.height * 0.5,
            ]),
            ..UiInputState::default()
        },
    );
    assert_eq!(
        session.interaction.focus.hovered.as_deref(),
        Some("hub.showcase-small.0")
    );
    let hovered_frame = session.build_frame(&surface, 1_300, 900, [8, 11, 15, 255]);
    let hovered = hovered_frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.showcase-small.0")
        .expect("hovered showcase card");

    assert_eq!(
        hovered.style.border,
        hub_tokens(StudioUiPalette::IndustrialDark).accent
    );
    assert_eq!(hovered.style.border_width, 1.0);
}

#[test]
fn project_menu_button_dispatches_the_retained_context_action() {
    let project = HubSurfaceProject {
        name: "Demo".to_string(),
        path: PathBuf::from("C:/Projects/Demo"),
        project_type: ProjectType::Game,
        last_opened_label: "14/07/2026".to_string(),
    };
    let model = HubSurfaceModel {
        projects: vec![project.clone()],
        featured_project: Some(project),
        hovered_project_path: Some(PathBuf::from("C:/Projects/Demo")),
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
    let mut session = UiSurfaceSession::default();
    let frame = session.build_frame(&surface, 1_300, 900, [8, 11, 15, 255]);
    let menu = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.recent.0.menu")
        .expect("project menu box");
    let pointer_position = Some([
        menu.rect.x + menu.rect.width * 0.5,
        menu.rect.y + menu.rect.height * 0.5,
    ]);
    let _ = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position,
            pointer_down: true,
            ..UiInputState::default()
        },
    );
    assert_eq!(
        session.interaction.focus.active.as_deref(),
        Some("hub.recent.0.menu")
    );
    let actions = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position,
            pointer_down: false,
            ..UiInputState::default()
        },
    );

    assert!(actions.iter().any(|dispatched| {
        matches!(
            &dispatched.action,
            UiAction::Custom { channel, payload }
                if channel == "hub.project"
                    && payload.get("action").and_then(|value| value.as_str()) == Some("menu")
        )
    }));
}

#[test]
fn hub_keeps_recent_project_menu_available_in_each_row() {
    let project = HubSurfaceProject {
        name: "Demo".to_string(),
        path: PathBuf::from("C:/Projects/Demo"),
        project_type: ProjectType::Game,
        last_opened_label: "14/07/2026".to_string(),
    };
    let model = HubSurfaceModel {
        projects: vec![project.clone()],
        ..HubSurfaceModel::default()
    };

    let frame = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model)
        .build_frame(1_300, 900, [8, 11, 15, 255]);

    assert!(frame
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "hub.recent.0.menu"));
}

#[test]
fn hub_uses_top_navigation_instead_of_the_legacy_rail() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let topbar = surface
        .root
        .children
        .iter()
        .find(|child| child.id == "hub.topbar")
        .expect("hub topbar");

    assert_eq!(topbar.kind, UiNodeKind::Toolbar);
    assert!(!matches!(
        topbar.control,
        raf_render::api_graphic_basic::ui_surface::UiControl::ScrollView { .. }
    ));
    assert!(!surface
        .root
        .children
        .iter()
        .any(|child| child.id == "hub.navigation"));
}

#[test]
fn hub_create_type_menu_has_no_dead_space() {
    let model = HubSurfaceModel {
        create_type_menu_open: true,
        ..HubSurfaceModel::default()
    };
    let frame = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model)
        .build_frame(1_300, 900, [8, 11, 15, 255]);

    let menu = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create.type-menu")
        .expect("type menu box");
    assert_eq!(menu.rect.height, 84.0);
    assert_eq!(menu.rect.width, 156.0);

    let game = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create.type-game")
        .expect("game option box");
    let electronics = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create.type-electronics")
        .expect("electronics option box");
    assert_eq!(game.rect.height, 42.0);
    assert_eq!(electronics.rect.height, 42.0);
    assert!(game.rect.width > 120.0);
    assert!(electronics.rect.width > 120.0);
}

#[test]
fn hub_window_buttons_use_compact_centered_hit_areas() {
    let frame = build_hub_surface(StudioUiPalette::IndustrialDark).build_frame(
        1_300,
        900,
        [8, 11, 15, 255],
    );

    for id in ["hub.minimize", "hub.maximize", "hub.close"] {
        let button = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == id)
            .unwrap_or_else(|| panic!("{id} box"));
        assert_eq!(button.rect.width, 32.0);
        assert_eq!(button.rect.height, 32.0);
    }
}

#[test]
fn hub_create_row_keeps_aligned_controls() {
    let frame = build_hub_surface(StudioUiPalette::IndustrialDark).build_frame(
        1_300,
        900,
        [8, 11, 15, 255],
    );

    let picker = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create.type-picker")
        .expect("type picker box");
    let submit = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create.submit")
        .expect("submit box");
    let folder = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create.choose-path")
        .expect("folder button box");

    assert_eq!(picker.rect.height, 52.0);
    assert_eq!(submit.rect.height, 52.0);
    assert_eq!(folder.rect.height, 44.0);
    assert_eq!(folder.rect.width, 40.0);
}

#[test]
fn hub_create_row_is_a_set_of_separate_pills() {
    let frame = build_hub_surface(StudioUiPalette::IndustrialDark).build_frame(
        1_300,
        900,
        [8, 11, 15, 255],
    );
    let rect = |id: &str| {
        frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == id)
            .unwrap_or_else(|| panic!("{id} box"))
            .rect
    };

    let name = rect("hub.create.name");
    let name_field = rect("hub.create.name-field");
    let picker = rect("hub.create.type-picker");
    let location = rect("hub.create.location-row");
    let agent = rect("hub.create.agent");
    let submit = rect("hub.create.submit");

    assert!(picker.x >= name.right() + 10.0);
    assert!((picker.x - name_field.right() - 12.0).abs() < 0.6);
    assert!(name_field.width > 420.0);
    assert!(name.width > 350.0);
    assert!(location.x >= picker.right() + 10.0);
    assert!(agent.x >= location.right() + 10.0);
    assert!(submit.x >= agent.right() + 10.0);
    assert_eq!(location.width, 200.0);
    assert_eq!(submit.width, 132.0);
}

#[test]
fn hub_create_name_field_uses_the_ai_sparkle() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let serialized = format!("{:?}", surface.root);

    assert!(serialized.contains("hub.create.name-sparkle"));
    assert!(serialized.contains("editor.hub.sparkle"));
}

#[test]
fn hub_create_submit_uses_the_crisp_arrow_asset() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let serialized = format!("{:?}", surface.root);
    assert!(
        !serialized.contains("hub.create.submit-icon"),
        "the stale low-resolution arrow node must not remain"
    );
    assert!(
        !serialized.contains("editor.hub.arrow-right"),
        "stale arrow key must not remain in the hub document"
    );
    assert!(serialized.contains("hub.create.submit-arrow"));
    assert!(serialized.contains("editor.hub.submit-arrow"));
}

#[test]
fn hub_create_submit_label_is_bold_for_hierarchy() {
    use raf_render::api_graphic_basic::ui_surface::{UiStyleRuleState, UiStyleSelector};
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let rule = surface
        .style_sheet
        .rules
        .iter()
        .find(|rule| {
            matches!(
                &rule.selector,
                UiStyleSelector::Class(name) if name == "hub-primary-button"
            ) && matches!(rule.state, UiStyleRuleState::Always)
        })
        .expect("primary button rule");
    assert_eq!(rule.patch.border, Some([168, 88, 15, 255]));
}

#[test]
fn hub_primary_button_uses_dark_ink_for_contrast() {
    use raf_render::api_graphic_basic::ui_surface::{UiStyleRuleState, UiStyleSelector};
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let rule = surface
        .style_sheet
        .rules
        .iter()
        .find(|rule| {
            matches!(
                &rule.selector,
                UiStyleSelector::Class(name) if name == "hub-primary-button"
            ) && matches!(rule.state, UiStyleRuleState::Always)
        })
        .expect("primary button rule");

    assert_eq!(rule.patch.text, Some([18, 18, 20, 255]));
    assert_eq!(rule.patch.radius, Some(4.0));
}

fn hub_test_project(name: &str) -> HubSurfaceProject {
    HubSurfaceProject {
        name: name.to_string(),
        path: PathBuf::from(format!("C:/Projects/{name}")),
        project_type: ProjectType::Game,
        last_opened_label: "14/07/2026".to_string(),
    }
}

#[test]
fn hub_search_without_matches_reports_no_results_instead_of_missing_projects() {
    let project = hub_test_project("Demo");
    let model = HubSurfaceModel {
        projects: vec![project.clone()],
        featured_project: Some(project),
        search_query: "zzz".to_string(),
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    let empty_title = surface
        .root
        .find("hub.showcase-empty-title")
        .expect("showcase empty title");
    assert_eq!(
        empty_title.text_key.as_deref(),
        Some("app.hub_no_results_title")
    );
    let list_empty = surface
        .root
        .find("hub.recent-empty")
        .expect("list empty copy");
    assert_eq!(
        list_empty.text_key.as_deref(),
        Some("app.hub_no_results_subtitle")
    );
}

#[test]
fn hub_loading_state_waits_for_discovery_before_reporting_empty() {
    let model = HubSurfaceModel {
        discovery_pending: true,
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    let empty = surface
        .root
        .find("hub.recent-empty")
        .expect("list loading copy");
    assert_eq!(empty.text_key.as_deref(), Some("app.hub_loading_projects"));
}

#[test]
fn hub_create_row_offers_open_when_the_path_is_a_project() {
    let model = HubSurfaceModel {
        create_path_is_project: true,
        create_path: "C:/Projects/Demo".to_string(),
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    let label = surface
        .root
        .find("hub.create.submit-label")
        .expect("submit label");
    assert_eq!(label.text_key.as_deref(), Some("app.hub_open"));
    assert!(surface.root.find("hub.create-open-hint").is_some());

    let frame = surface.build_frame(1_300, 900, [8, 11, 15, 255]);
    let form = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.create-form")
        .expect("create form");
    assert_eq!(form.rect.height, 102.0);
}

#[test]
fn hub_context_menu_confirms_destructive_removal_before_acting() {
    let project = hub_test_project("Demo");
    let model = HubSurfaceModel {
        projects: vec![project.clone()],
        context_project: Some(project),
        context_menu_position: Some([12.0, 12.0]),
        context_confirm_forget: true,
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    let label = surface
        .root
        .find("hub.context-forget.label")
        .expect("forget row label");
    assert_eq!(label.text_key.as_deref(), Some("app.hub_remove_confirm"));
}

#[test]
fn hub_list_bounds_its_height_and_scrolls_large_libraries() {
    let projects: Vec<_> = (0..20)
        .map(|index| hub_test_project(&format!("Project {index}")))
        .collect();
    let model = HubSurfaceModel {
        projects,
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
    let frame = surface.build_frame(1_300, 900, [8, 11, 15, 255]);

    let panel = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "hub.recent")
        .expect("projects panel");
    assert_eq!(
        panel.rect.height,
        66.0 + HUB_LIST_VISIBLE_ROWS as f32 * 43.0
    );
    assert!(surface.root.find("hub.recent-scroll").is_some());
    assert!(frame
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "__rafui.scrollbar.hub.recent-scroll.track"));
}

#[test]
fn hub_top_action_is_the_get_in_menu_trigger() {
    let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
    let trigger = surface.root.find("hub.new").expect("get in trigger");
    let label = trigger.find("hub.new.label").expect("trigger label");

    assert_eq!(label.text_key.as_deref(), Some("app.hub_get_in"));
    assert!(trigger.find("hub.new.chevron").is_some());
}

#[test]
fn hub_get_in_menu_offers_create_and_open_entries() {
    let model = HubSurfaceModel {
        get_in_menu_open: true,
        get_in_menu_position: Some([40.0, 40.0]),
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    for id in [
        "hub.getin.new",
        "hub.getin.open-folder",
        "hub.getin.open-file",
    ] {
        let row = surface.root.find(id).unwrap_or_else(|| panic!("{id} row"));
        assert_eq!(row.kind, UiNodeKind::Button);
    }
}

#[test]
fn hub_adopt_overlay_confirms_before_creating_a_project() {
    let model = HubSurfaceModel {
        pending_adopt: Some(HubPendingAdopt {
            path: PathBuf::from("C:/Projects/Imported"),
            name: "Imported".to_string(),
            project_type: ProjectType::Game,
        }),
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    assert!(surface.root.find("hub.adopt.overlay").is_some());
    assert_eq!(
        surface.root.find("hub.adopt.confirm").map(|node| node.kind),
        Some(UiNodeKind::Button)
    );
    assert_eq!(
        surface.root.find("hub.adopt.cancel").map(|node| node.kind),
        Some(UiNodeKind::Button)
    );
    let path = surface.root.find("hub.adopt.path").expect("adopt path");
    assert_eq!(path.text_value.as_deref(), Some("C:/Projects/Imported"));
    let kind = surface.root.find("hub.adopt.type").expect("adopt kind");
    assert_eq!(kind.text_key.as_deref(), Some("hub.adopt.type"));
}

#[test]
fn hub_open_error_overlay_keeps_the_reason_readable() {
    let detail = "Rafi could not read this project: bad ron";
    let model = HubSurfaceModel {
        pending_open_error: Some(detail.to_string()),
        ..HubSurfaceModel::default()
    };
    let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

    assert!(surface.root.find("hub.open-error.overlay").is_some());
    let reason = surface
        .root
        .find("hub.open-error.detail")
        .expect("error detail");
    assert_eq!(reason.text_value.as_deref(), Some(detail));
    assert!(surface.root.find("hub.open-error.close").is_some());
}
