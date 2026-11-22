//! Settings surface regressions.

use raf_core::ai::AiProvider;
use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiCompactMode, UiFlow, UiNode, UiNodeKind, UiSizeMode,
    UiSurfaceMaterial,
};
use raf_ui::UiRect;

use super::ai::ai;
use super::{
    build_settings_surface, build_settings_surface_with_state, provider_id, SettingsSection,
    SETTINGS_COMPACT_BREAKPOINT, SETTINGS_NAV_WIDTH, SETTINGS_ROW_HEIGHT,
    SETTINGS_STACKED_ROW_HEIGHT,
};

#[test]
fn provider_redesign_plan_hides_api_fields_and_picker_scrolls() {
    let mut settings = EngineSettings::default();
    settings.default_ai_provider = AiProvider::OpenAI;
    settings.agent_openai_use_plan = true;
    let palette = StudioUiPalette::IndustrialDark;
    let plan = ai(
        palette,
        &settings,
        Some("settings.ai-provider.openai.model.trigger"),
        &[],
        "",
    );
    assert!(find_node(&plan, "settings.ai-provider.openai.api-fields").is_none());
    assert!(find_node(&plan, "settings.ai-provider.openai.plan-note").is_some());
    assert!(find_node(&plan, "settings.ai-provider.openai.model.scroll").is_some());
    settings.agent_openai_use_plan = false;
    let api = ai(palette, &settings, None, &[], "");
    assert!(find_node(&api, "settings.ai-provider.openai.api-fields").is_some());
    fn unique(node: &UiNode, ids: &mut std::collections::HashSet<String>) {
        assert!(
            ids.insert(node.id.clone()),
            "duplicate node id: {}",
            node.id
        );
        for child in &node.children {
            unique(child, ids);
        }
    }
    unique(&api, &mut std::collections::HashSet::new());
}

#[test]
fn harness_ai_settings_focus_one_provider_and_hide_inactive_limits() {
    let mut settings = EngineSettings::default();
    let palette = StudioUiPalette::IndustrialDark;
    let root = ai(palette, &settings, None, &[], "");
    for provider in &settings.ai_providers {
        assert_eq!(
            find_node(
                &root,
                &format!(
                    "settings.ai-provider.{}.title",
                    provider_id(provider.provider)
                )
            )
            .is_some(),
            provider.provider == settings.default_ai_provider
        );
    }
    for id in [
        "settings.agent-max-tool-calls.slider",
        "settings.agent-max-response-tokens.slider",
    ] {
        assert!(find_node(&root, id).is_none());
    }
    settings.agent_tool_call_limit_enabled = true;
    settings.agent_response_limit_enabled = true;
    let root = ai(palette, &settings, None, &[], "");
    for id in [
        "settings.agent-max-tool-calls.slider",
        "settings.agent-max-response-tokens.slider",
    ] {
        let slider = find_node(&root, id).expect("enabled manual limit control");
        assert!(!slider.disabled);
    }
}
use raf_render::api_graphic_basic::ui_surface::{UiInputState, UiSurfaceSession};

fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}

fn collect_nodes_with_class<'a>(node: &'a UiNode, class: &str, output: &mut Vec<&'a UiNode>) {
    if node.classes.iter().any(|candidate| candidate == class) {
        output.push(node);
    }
    for child in &node.children {
        collect_nodes_with_class(child, class, output);
    }
}

fn collect_text_keys(node: &UiNode, output: &mut Vec<String>) {
    if let Some(text_key) = &node.text_key {
        output.push(text_key.clone());
    }
    for child in &node.children {
        collect_text_keys(child, output);
    }
}

#[test]
fn settings_surface_keeps_all_legacy_sections_in_a_single_navigation_contract() {
    assert_eq!(SettingsSection::ALL.len(), 9);
    assert_eq!(
        SettingsSection::from_command("settings.section.viewport"),
        Some(SettingsSection::Viewport)
    );
}

#[test]
fn settings_surface_uses_intrinsic_rows_without_vertical_text_columns() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Appearance,
    );
    assert_eq!(surface.root.layout.width_mode, UiSizeMode::Fill);
    assert!(surface.root.children.len() >= 2);
}

#[test]
fn settings_surface_uses_semantic_modal_materials_and_exposes_transparency_control() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Appearance,
    );
    let backdrop = find_node(&surface.root, "settings.backdrop").expect("settings backdrop");
    let modal = find_node(&surface.root, "settings.modal").expect("settings modal");
    let toggle = find_node(&surface.root, "settings.reduce-transparency.row")
        .expect("reduce transparency row");

    assert_eq!(backdrop.material, UiSurfaceMaterial::BackdropScrim);
    assert_eq!(modal.material, UiSurfaceMaterial::ModalSurface);
    assert!(toggle.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::SetToggle { key, value }
            if key == "settings.reduce_transparency" && *value
    )));
}

#[test]
fn settings_modal_keeps_the_navigation_rail_and_exposes_a_resize_handle() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Appearance,
    );
    let navigation = find_node(&surface.root, "settings.navigation").expect("navigation");
    assert_eq!(navigation.layout.basis[0], SETTINGS_NAV_WIDTH);
    assert!(navigation.layout.responsive.is_empty());

    let handle =
        find_node(&surface.root, "settings.modal.resize-handle").expect("modal resize handle");
    assert_eq!(
        handle.layout.rect,
        Some(UiRect::new(1060.0, 700.0, 20.0, 20.0))
    );
    assert_eq!(handle.layout.z_index, 60);
    assert!(handle.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::Command { name } if name == "settings.modal.resize.start"
    )));
    assert!(handle.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::Command { name } if name == "settings.modal.resize.move"
    )));
    assert!(handle.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::Command { name } if name == "settings.modal.resize.end"
    )));
}

#[test]
fn settings_rows_only_stack_at_a_genuinely_narrow_width() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Editor,
    );
    let row = find_node(&surface.root, "settings.grid_size.row").expect("range row");
    assert_eq!(row.layout.compact, UiCompactMode::None);
    assert_eq!(row.layout.responsive.len(), 1);
    assert_eq!(
        row.layout.responsive[0].max_width,
        SETTINGS_COMPACT_BREAKPOINT
    );
    assert_eq!(
        row.layout.responsive[0].basis,
        Some([0.0, SETTINGS_STACKED_ROW_HEIGHT])
    );
}

#[test]
fn settings_toggle_row_is_clickable_outside_the_switch() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Editor,
    );
    let row = find_node(&surface.root, "settings.show-grid.row").expect("toggle row");

    assert!(row.interactive);
    assert!(row.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::SetToggle { key, value }
            if key == "settings.show_grid" && !value
    )));
}

#[test]
fn settings_range_values_are_literal_text_with_the_configured_unit() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Editor,
    );
    let value = find_node(&surface.root, "settings.grid_size.display").expect("range value");

    assert!(value.text_key.is_none());
    assert!(value
        .text_value
        .as_deref()
        .is_some_and(|text| text.ends_with(" m")));
}

#[test]
fn settings_segments_have_stable_padded_hitboxes() {
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Editor,
    );
    let segments = find_node(&surface.root, "settings.units.control").expect("segments");

    assert!(segments.children.iter().all(|segment| {
        segment.layout.width_mode == UiSizeMode::Fixed
            && segment.layout.height_mode == UiSizeMode::Fixed
            && segment.layout.padding.left >= 8.0
            && segment.layout.padding.right >= 8.0
    }));
    assert!(segments.layout.responsive.is_empty());
}

#[test]
fn every_settings_section_keeps_rows_separated_and_labels_localized() {
    for section in SettingsSection::ALL {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            section,
        );
        let mut rows = Vec::new();
        collect_nodes_with_class(&surface.root, "settings-row", &mut rows);
        assert!(!rows.is_empty(), "section {section:?} has no settings rows");
        assert!(rows
            .iter()
            .all(|row| row.layout.basis[1] >= SETTINGS_ROW_HEIGHT));

        let mut segments = Vec::new();
        collect_nodes_with_class(&surface.root, "settings-segments", &mut segments);
        assert!(segments
            .iter()
            .all(|toolbar| toolbar.layout.responsive.is_empty()));

        let mut text_keys = Vec::new();
        collect_text_keys(&surface.root, &mut text_keys);
        assert!(text_keys
            .iter()
            .all(|key| !key.starts_with("settings.hierarchy_show_")
                && !key.starts_with("settings.electronics_show_")));

        let mut session = UiSurfaceSession::default();
        let frame = session.build_layout_frame_at_scale(&surface, 1600, 2400, [0, 0, 0, 255], 1.0);
        let mut rects = rows
            .iter()
            .map(|row| {
                frame
                    .layout_boxes
                    .iter()
                    .find(|layout| layout.id == row.id)
                    .unwrap_or_else(|| panic!("missing layout box {}", row.id))
                    .rect
            })
            .collect::<Vec<_>>();
        rects.sort_by(|left, right| left.y.total_cmp(&right.y));
        assert!(
            rects
                .windows(2)
                .all(|pair| { pair[1].y + 0.01 >= pair[0].y + pair[0].height }),
            "overlapping settings rows in {section:?}: {rects:?}"
        );
    }
}

#[test]
fn automatic_ui_scale_disables_manual_range_input() {
    let mut settings = EngineSettings::default();
    settings.auto_ui_scale = true;
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &settings,
        SettingsSection::Appearance,
    );
    let slider = find_node(&surface.root, "settings.ui_scale.slider").expect("ui scale slider");
    let input = find_node(&surface.root, "settings.ui_scale.value").expect("ui scale input");

    assert!(slider.disabled);
    assert!(input.disabled);
}

#[test]
fn settings_is_a_movable_dialog_with_a_real_select_popup_contract() {
    let surface = build_settings_surface_with_state(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Performance,
        UiRect::new(32.0, 40.0, 920.0, 640.0),
        "",
        Some("settings.quality.control"),
    );
    let modal = find_node(&surface.root, "settings.modal").expect("settings modal");
    assert_eq!(modal.kind, UiNodeKind::FloatingPanel);
    assert_eq!(modal.layout.position_mode, raf_ui::UiPositionMode::Absolute);
    assert_eq!(
        modal.layout.rect,
        Some(UiRect::new(32.0, 40.0, 920.0, 640.0))
    );
    assert!(find_node(&surface.root, "settings.backdrop")
        .expect("settings backdrop")
        .event_handlers
        .iter()
        .any(|binding| matches!(
            &binding.action,
            UiAction::Command { name } if name == "settings.dismiss"
        )));

    let trigger = find_node(&surface.root, "settings.quality.control").expect("select");
    let select = trigger.control.select().expect("UiSelect control");
    assert!(select.open);
    assert_eq!(select.popup_id.as_deref(), Some("settings.quality.menu"));
    let option = find_node(&surface.root, "settings.quality.option.high").expect("option");
    assert!(option.focusable);
    assert!(option.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::SetSelect { key, value, .. }
            if key == "settings.quality" && value == "high"
    )));
    let reset = find_node(&surface.root, "settings.reset-defaults").expect("reset defaults");
    assert!(reset.focusable);
    assert!(reset.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::Command { name } if name == "settings.reset_defaults"
    )));
}

#[test]
fn settings_select_popup_keeps_ordered_hitboxes_and_selection_indices() {
    let surface = build_settings_surface_with_state(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Performance,
        UiRect::new(32.0, 40.0, 920.0, 640.0),
        "",
        Some("settings.render_execution_policy.control"),
    );
    let menu = find_node(&surface.root, "settings.render_execution_policy.menu")
        .expect("render policy menu");
    assert_eq!(menu.layout.flow, UiFlow::Column);
    assert_eq!(
        menu.children
            .iter()
            .filter_map(|option| option.text_key.as_deref())
            .collect::<Vec<_>>(),
        vec![
            "settings.render_execution_policy.auto",
            "settings.render_execution_policy.cpu_only",
            "settings.render_execution_policy.gpu_preferred",
        ]
    );

    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 1280, 800, [0, 0, 0, 255], |key| {
            key.to_string()
        });
    let mut option_rects = menu
        .children
        .iter()
        .filter_map(|option| {
            frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == option.id)
                .map(|layout| layout.rect)
        })
        .collect::<Vec<_>>();
    option_rects.sort_by(|left, right| left.y.total_cmp(&right.y));
    assert_eq!(option_rects.len(), 3);
    assert!(option_rects
        .iter()
        .all(|rect| (rect.height - 30.0).abs() < f32::EPSILON));
    assert!(option_rects
        .windows(2)
        .all(|pair| pair[1].y >= pair[0].bottom()));

    let option = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "settings.render_execution_policy.option.cpu_only")
        .expect("CPU-only option");
    let point = [option.rect.x + 12.0, option.rect.y + 12.0];
    let _ = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some(point),
            pointer_down: true,
            time_seconds: 0.1,
            ..UiInputState::default()
        },
    );
    let actions = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some(point),
            time_seconds: 0.2,
            ..UiInputState::default()
        },
    );
    assert!(actions.iter().any(|action| matches!(
        &action.action,
        UiAction::SetSelect {
            key,
            value,
            index: 1,
        } if key == "settings.render_execution_policy" && value == "cpu_only"
    )));
}

#[test]
fn settings_omits_nodes_but_keeps_game_and_electronics_sections_distinct() {
    let mut text_keys = Vec::new();
    let game = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Viewport,
    );
    collect_text_keys(&game.root, &mut text_keys);
    assert!(text_keys.iter().all(|key| !key.contains("nodes")));
    assert!(find_node(&game.root, "settings.group.game.label").is_some());

    let electronics = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Electronics,
    );
    assert!(find_node(&electronics.root, "settings.group.electronics.label").is_some());
    assert!(find_node(&electronics.root, "settings.electronics-grid-step.row").is_some());
}

#[test]
fn viewport_render_settings_expose_only_lit() {
    let surface = build_settings_surface_with_state(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Viewport,
        UiRect::new(32.0, 40.0, 920.0, 640.0),
        "",
        Some("settings.viewport-render-mode.control"),
    );
    let trigger = find_node(&surface.root, "settings.viewport-render-mode.control")
        .expect("viewport render select");
    let select = trigger.control.select().expect("UiSelect control");

    assert_eq!(select.options.len(), 1);
    assert_eq!(select.options[0].value, "solid");
    assert_eq!(
        select.options[0].label_key,
        "settings.viewport_render_mode.solid"
    );
    assert!(find_node(
        &surface.root,
        "settings.viewport-render-mode.option.wireframe"
    )
    .is_none());
    assert!(find_node(
        &surface.root,
        "settings.viewport-render-mode.option.preview"
    )
    .is_none());
}

#[test]
fn changed_appearance_rows_expose_a_reset_action() {
    let defaults = EngineSettings::default();
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &defaults,
        SettingsSection::Appearance,
    );
    assert!(find_node(&surface.root, "settings.font_size.reset").is_none());

    let mut changed = EngineSettings::default();
    changed.font_size = 16.0;
    let surface = build_settings_surface(
        StudioUiPalette::IndustrialDark,
        &changed,
        SettingsSection::Appearance,
    );
    assert!(find_node(&surface.root, "settings.font_size.reset").is_some());
}

#[test]
fn settings_search_hides_rows_that_do_not_match() {
    let surface = build_settings_surface_with_state(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Appearance,
        UiRect::new(0.0, 0.0, 1080.0, 720.0),
        "font",
        None,
    );
    let font = find_node(&surface.root, "settings.font_size.row").expect("font row");
    assert!(!font.hidden);
    let language = find_node(&surface.root, "settings.language.row").expect("language row");
    assert!(language.hidden);
}

#[test]
fn settings_footer_reports_dirty_and_clean_states() {
    let rect = UiRect::new(0.0, 0.0, 1080.0, 720.0);
    let clean = super::build_settings_surface_with_state_and_api_keys(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Appearance,
        rect,
        "",
        None,
        &[],
        "",
        false,
    );
    let dirty = super::build_settings_surface_with_state_and_api_keys(
        StudioUiPalette::IndustrialDark,
        &EngineSettings::default(),
        SettingsSection::Appearance,
        rect,
        "",
        None,
        &[],
        "",
        true,
    );
    assert_eq!(
        find_node(&clean.root, "settings.footer.status-label")
            .and_then(|node| node.text_key.as_deref()),
        Some("settings.all_saved")
    );
    assert_eq!(
        find_node(&dirty.root, "settings.footer.status-label")
            .and_then(|node| node.text_key.as_deref()),
        Some("app.unsaved_changes_title")
    );
}
