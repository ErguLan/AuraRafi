//! Settings surface regressions.

use raf_core::ai::AiProvider;
use raf_core::config::{EngineSettings, Language};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiCompactMode, UiFlow, UiNode, UiNodeKind, UiSizeMode,
    UiSurfaceMaterial,
};
use raf_ui::UiRect;

use super::ai::ai;
use super::{
    build_settings_surface, build_settings_surface_with_state, provider_id, SettingsSection,
    PERFORMANCE_FILTER_COMMAND, SETTINGS_COMPACT_BREAKPOINT, SETTINGS_IMPACT_ROW_HEIGHT,
    SETTINGS_IMPACT_STACKED_ROW_HEIGHT, SETTINGS_NAV_WIDTH, SETTINGS_ROW_HEIGHT,
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

fn layout_rect(
    frame: &raf_render::api_graphic_basic::ui_surface::UiSurfaceFrame,
    id: &str,
) -> UiRect {
    frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == id)
        .unwrap_or_else(|| panic!("missing layout box {id}"))
        .rect
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
        false,
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

#[test]
fn settings_performance_metadata_matches_the_visible_rows() {
    let palette = StudioUiPalette::IndustrialDark;
    let expected = [
        (SettingsSection::Appearance, 3, 0),
        (SettingsSection::Performance, 14, 8),
        (SettingsSection::Editor, 5, 0),
        (SettingsSection::Viewport, 4, 0),
        (SettingsSection::Electronics, 2, 0),
        (SettingsSection::Scripting, 0, 0),
        (SettingsSection::Runtime, 8, 1),
        (SettingsSection::Ai, 0, 0),
        (SettingsSection::Platform, 1, 1),
    ];

    for (section, performance, high) in expected {
        let surface = build_settings_surface(palette, &EngineSettings::default(), section);
        let card = find_node(&surface.root, section.key())
            .unwrap_or_else(|| panic!("missing card for {section:?}"));
        let summary = super::impact::summarize(card);
        assert_eq!(
            (summary.performance, summary.high),
            (performance, high),
            "unexpected performance metadata for {section:?}"
        );

        let filter = find_node(&surface.root, "settings.performance-filter");
        assert_eq!(
            filter.is_some(),
            performance > 0,
            "filter visibility mismatch for {section:?}"
        );
        if performance > 0 {
            let summary_label =
                find_node(&surface.root, "settings.impact.summary").expect("section summary label");
            let text = summary_label.text_value.as_deref().unwrap_or_default();
            assert!(text.contains(&performance.to_string()));
            if high > 0 {
                assert!(text.contains(&high.to_string()));
            }
        }
    }

    let surface = build_settings_surface(
        palette,
        &EngineSettings::default(),
        SettingsSection::Performance,
    );
    let performance = find_node(&surface.root, "settings.quality.impact.performance")
        .expect("quality performance badge");
    assert!(performance
        .classes
        .iter()
        .any(|class| class == "settings-impact-performance-high"));
    assert_eq!(
        performance.tooltip_key.as_deref(),
        Some("settings.impact.tooltip.performance.high.lowers_cost")
    );
    assert!(find_node(&surface.root, "settings.quality.impact.visual").is_some());
    let quality_row = find_node(&surface.root, "settings.quality.row").expect("quality row");
    assert_eq!(quality_row.layout.basis[1], SETTINGS_IMPACT_ROW_HEIGHT);
    assert_eq!(
        quality_row.layout.responsive[0].basis,
        Some([0.0, SETTINGS_IMPACT_STACKED_ROW_HEIGHT])
    );
}

#[test]
fn performance_filter_hides_only_rows_without_a_performance_contract() {
    let palette = StudioUiPalette::IndustrialDark;
    let rect = UiRect::new(0.0, 0.0, 1080.0, 720.0);
    let normal = super::build_settings_surface_with_state_and_api_keys(
        palette,
        &EngineSettings::default(),
        SettingsSection::Performance,
        rect,
        "",
        None,
        &[],
        "",
        false,
        false,
    );
    let filter =
        find_node(&normal.root, "settings.performance-filter").expect("performance filter");
    assert!(!filter
        .classes
        .iter()
        .any(|class| class == "settings-impact-filter-active"));
    assert!(filter.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::Command { name } if name == PERFORMANCE_FILTER_COMMAND
    )));
    assert!(
        !find_node(&normal.root, "settings.particle_preview.time_scale.row")
            .expect("time scale row")
            .hidden
    );

    let filtered = super::build_settings_surface_with_state_and_api_keys(
        palette,
        &EngineSettings::default(),
        SettingsSection::Performance,
        rect,
        "",
        None,
        &[],
        "",
        false,
        true,
    );
    assert!(find_node(&filtered.root, "settings.performance-filter")
        .expect("performance filter")
        .classes
        .iter()
        .any(|class| class == "settings-impact-filter-active"));
    assert!(
        !find_node(&filtered.root, "settings.quality.row")
            .expect("quality row")
            .hidden
    );
    assert!(
        find_node(&filtered.root, "settings.particle_preview.time_scale.row")
            .expect("time scale row")
            .hidden
    );
    assert!(
        find_node(&filtered.root, "settings.particle_preview.wind_x.row")
            .expect("wind row")
            .hidden
    );

    // A section without performance rows shows the empty state, but keeps the
    // active filter control visible so the user can always turn it off.
    let ai = super::build_settings_surface_with_state_and_api_keys(
        palette,
        &EngineSettings::default(),
        SettingsSection::Ai,
        rect,
        "",
        None,
        &[],
        "",
        false,
        true,
    );
    assert!(find_node(&ai.root, "settings.performance-filter")
        .expect("active filter remains available in empty sections")
        .classes
        .iter()
        .any(|class| class == "settings-impact-filter-active"));
    assert!(find_node(&ai.root, "settings.ai-persist-credentials.row").is_none());
    assert!(find_node(&ai.root, "settings.ai-provider.openrouter").is_none());
    assert_eq!(
        find_node(&ai.root, "settings.no-results")
            .expect("empty performance section message")
            .text_key
            .as_deref(),
        Some("settings.impact.no_results")
    );
}

#[test]
fn impact_badges_and_filter_stay_inside_the_settings_header_and_title_line() {
    let palette = StudioUiPalette::IndustrialDark;
    for (width, height, language) in [
        (1080.0, 720.0, Language::English),
        (720.0, 520.0, Language::English),
        (720.0, 520.0, Language::Spanish),
    ] {
        let surface = super::build_settings_surface_with_state_and_api_keys(
            palette,
            &EngineSettings::default(),
            SettingsSection::Performance,
            UiRect::new(0.0, 0.0, width, height),
            "",
            None,
            &[],
            "",
            false,
            false,
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text(
            &surface,
            width as u32,
            height as u32,
            [0, 0, 0, 255],
            |key| raf_core::i18n::t(key, language),
        );

        let header = layout_rect(&frame, "settings.workspace.header-content");
        let header_copy = layout_rect(&frame, "settings.workspace.header-copy");
        let filter = layout_rect(&frame, "settings.performance-filter");
        let summary = layout_rect(&frame, "settings.impact.summary");
        assert!(header_copy.right() <= filter.x + 0.01);
        assert!(summary.right() <= header_copy.right() + 0.01);
        assert!(summary.bottom() <= header_copy.bottom() + 0.01);
        assert!(filter.right() <= header.right() + 0.01);
        assert!(filter.bottom() <= header.bottom() + 0.01);

        let copy = layout_rect(&frame, "settings.quality.copy");
        let label = layout_rect(&frame, "settings.quality.label");
        let badge = layout_rect(&frame, "settings.quality.impact.performance");
        let visual = layout_rect(&frame, "settings.quality.impact.visual");
        assert!(badge.y >= label.bottom() - 0.01);
        assert!(badge.right() <= copy.right() + 0.01);
        assert!(visual.right() <= copy.right() + 0.01);
        assert!(visual.bottom() <= copy.bottom() + 0.01);
    }
}
