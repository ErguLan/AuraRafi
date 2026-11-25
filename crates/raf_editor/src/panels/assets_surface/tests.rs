//! Tests for the Assets panel surface and its catalog helpers.

use std::path::Path;

use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiIconId, UiStyleSelector};

use super::catalog::{
    asset_display_name, asset_grid_columns, asset_grid_row_count, asset_grid_row_range, asset_icon,
    asset_matches, asset_matches_script_extension, asset_meta, asset_virtual_bottom_spacer_height,
    asset_virtual_top_spacer_height, asset_visible_range_for_width, sort_asset_rows,
};

use super::*;

#[test]
fn rafui_interaction_assets_rebuilds_only_when_virtual_rows_change() {
    for view in [AssetViewMode::Grid, AssetViewMode::List] {
        let pitch = match view {
            AssetViewMode::Grid => ASSET_CARD_PITCH,
            AssetViewMode::List => ASSET_LIST_PITCH,
        };
        assert!(!asset_scroll_projection_changed(
            pitch + 0.25,
            pitch + 0.75,
            view
        ));
        assert!(asset_scroll_projection_changed(
            pitch - 0.25,
            pitch + 0.25,
            view
        ));
    }
}

fn params<'a>(palette: StudioUiPalette, rows: &'a [String]) -> AssetsSurfaceParams<'a> {
    AssetsSurfaceParams {
        palette,
        rows,
        language: raf_core::Language::English,
        query: "",
        filter: AssetFilter::All,
        script_extension: "",
        sort: AssetSort::NameAsc,
        view: AssetViewMode::Grid,
        selected: None,
        highlight: None,
        status: None,
        pending: false,
        catalog_error: None,
        visible_range: None,
        create_open: false,
        motion: AssetsMotion::default(),
        visuals: None,
    }
}

#[test]
fn asset_icons_follow_known_project_file_types() {
    assert_eq!(asset_icon("main.scene"), UiIconId::Scene);
    assert_eq!(asset_icon("board.pcb"), UiIconId::Pcb);
    assert_eq!(asset_icon("config.ron"), UiIconId::Project);
    assert_eq!(asset_icon("Building_A.glb"), UiIconId::Assets);
    assert_eq!(asset_icon("scripts/player.rs"), UiIconId::Script);
    assert_eq!(asset_icon("readme.unknown"), UiIconId::File);
}

#[test]
fn game_assets_include_truthful_builtin_primitive_rows() {
    let rows = asset_rows_with_builtins(&["models/tree.glb".to_string()]);
    assert_eq!(rows.len(), BUILTIN_ASSET_ROWS.len() + 1);
    assert_eq!(asset_display_name(&rows[0]), "Built-in / Cube");
    assert_eq!(asset_icon(&rows[1]), UiIconId::Sphere);
    assert!(rows.iter().any(|r| r == "builtin://camera"));
    assert_eq!(asset_icon("builtin://camera"), UiIconId::Camera);
    assert_eq!(asset_meta("builtin://camera"), (None, None));
    assert_eq!(rows.last().map(String::as_str), Some("models/tree.glb"));
}

#[test]
fn builtin_rows_stay_above_files_and_sorting_is_reversible() {
    let owned = vec![
        "scripts/zeta.rs".to_string(),
        "models/tree.glb".to_string(),
        "builtin://primitive/cube".to_string(),
        "scripts/alpha.rs".to_string(),
    ];
    let mut rows: Vec<&String> = owned.iter().collect();
    sort_asset_rows(&mut rows, AssetSort::NameAsc);
    assert_eq!(rows[0], "builtin://primitive/cube");
    assert_eq!(rows[1], "scripts/alpha.rs");
    assert_eq!(rows[2], "models/tree.glb");
    assert_eq!(rows[3], "scripts/zeta.rs");

    sort_asset_rows(&mut rows, AssetSort::NameDesc);
    assert_eq!(rows[0], "builtin://primitive/cube");
    assert_eq!(rows[1], "scripts/zeta.rs");
    assert_eq!(
        rows.last().map(|row| row.as_str()),
        Some("scripts/alpha.rs")
    );
}

#[test]
fn script_language_filter_narrows_rows_only_in_the_scripts_category() {
    let rows = asset_rows_with_builtins(&[
        "scripts/player.rs".to_string(),
        "scripts/level.cpp".to_string(),
    ]);
    let visible = |filter, extension: &str| {
        rows.iter()
            .filter(|row| {
                asset_matches(row, "", filter)
                    && asset_matches_script_extension(row, filter, extension)
            })
            .count()
    };
    assert_eq!(visible(AssetFilter::Scripts, "rs"), 1);
    assert_eq!(visible(AssetFilter::Scripts, ""), 2);
    // The chip only narrows the Scripts category, never the full listing.
    assert_eq!(visible(AssetFilter::All, "rs"), rows.len());
}

#[test]
fn card_meta_shows_a_language_chip_for_scripts_and_a_folder_for_media() {
    assert_eq!(
        asset_meta("scripts/player.rs"),
        (Some("RS".to_string()), None)
    );
    assert_eq!(
        asset_meta("models/tree.glb"),
        (None, Some("models".to_string()))
    );
    // Built-in rows are authoring shortcuts, so they show no file meta.
    assert_eq!(asset_meta("builtin://primitive/cube"), (None, None));
}

#[test]
fn the_visible_range_materializes_every_row_that_fits() {
    let rows: Vec<String> = (0..40)
        .map(|index| format!("models/tree{index}.glb"))
        .collect();
    let (start, end) = asset_visible_range(
        &rows,
        "",
        AssetFilter::Models,
        "",
        AssetSort::NameAsc,
        0.0,
        ASSET_CARD_PITCH * 40.0,
        AssetViewMode::Grid,
    );
    assert_eq!((start, end), (0, 40));

    // A short viewport only builds the window plus the overscan.
    let (start, end) = asset_visible_range_for_width(
        &rows,
        "",
        AssetFilter::Models,
        "",
        AssetSort::NameAsc,
        0.0,
        ASSET_CARD_PITCH * 3.0,
        300.0,
        AssetViewMode::Grid,
    );
    assert_eq!(start, 0);
    assert!(end < 40);
    assert!(end - start >= 3);
}

#[test]
fn the_visible_range_follows_the_scroll_offset() {
    let rows: Vec<String> = (0..40)
        .map(|index| format!("models/tree{index:02}.glb"))
        .collect();
    let viewport = ASSET_CARD_PITCH * 3.0;
    let (_, end) = asset_visible_range_for_width(
        &rows,
        "",
        AssetFilter::Models,
        "",
        AssetSort::NameAsc,
        0.0,
        viewport,
        300.0,
        AssetViewMode::Grid,
    );
    let (start, next_end) = asset_visible_range_for_width(
        &rows,
        "",
        AssetFilter::Models,
        "",
        AssetSort::NameAsc,
        ASSET_CARD_PITCH * 10.0,
        viewport,
        300.0,
        AssetViewMode::Grid,
    );
    assert!(start > 0);
    assert!(next_end > end);
    assert!(start < 40);
}

#[test]
fn the_visible_range_uses_the_same_filter_and_order_as_the_panel() {
    let rows = asset_rows_with_builtins(&[
        "scripts/player.rs".to_string(),
        "scripts/level.cpp".to_string(),
        "models/tree.glb".to_string(),
    ]);
    // The Scripts category only matches real script files, so the two
    // built-in primitives stay out of the window the panel builds.
    let visible = visible_asset_rows(&rows, "", AssetFilter::Scripts, "", AssetSort::NameAsc);
    assert_eq!(visible.len(), 2);
    let (start, end) = asset_visible_range(
        &rows,
        "",
        AssetFilter::Scripts,
        "",
        AssetSort::NameAsc,
        0.0,
        ASSET_CARD_PITCH * 10.0,
        AssetViewMode::Grid,
    );
    assert_eq!((start, end), (0, visible.len()));
}

#[test]
fn grid_cards_are_much_taller_than_list_rows_so_the_pitch_follows_the_view() {
    assert!(ASSET_CARD_PITCH > ASSET_LIST_PITCH * 2.0);
    assert_eq!(
        asset_virtual_bottom_spacer_height(2, 800.0, AssetViewMode::List),
        2.0 * ASSET_LIST_PITCH - ASSET_LIST_ROW_GAP
    );
    assert_eq!(
        asset_virtual_top_spacer_height(2, 800.0, AssetViewMode::List),
        2.0 * ASSET_LIST_PITCH
    );
}

#[test]
fn panel_and_overlays_build_from_data_only() {
    let rows = vec!["scripts/player.rs".to_string()];
    let palette = StudioUiPalette::IndustrialDark;
    let surface = build_assets_surface(params(palette, &rows));
    assert_eq!(surface.id, "editor.bottom.assets");

    let backdrop = build_assets_backdrop_surface(palette);
    assert_eq!(backdrop.id, "editor.assets.backdrop");
    assert!(build_assets_open_modal_surface(
        palette,
        "scripts/player.rs",
        "assets/scripts/player.rs"
    )
    .style_sheet
    .rules
    .iter()
    .any(|rule| rule.selector == UiStyleSelector::Class("assets-modal".to_string())));
    assert!(build_assets_context_menu_surface(
        palette,
        "scripts/player.rs",
        false,
        "assets/scripts/player.rs"
    )
    .root
    .find("assets.context.delete")
    .is_some());
}

#[test]
fn the_open_modal_exposes_the_four_documented_ways_to_open_a_row() {
    let palette = StudioUiPalette::IndustrialDark;
    let modal =
        build_assets_open_modal_surface(palette, "scripts/player.rs", "assets/scripts/player.rs");
    for id in [
        "assets.open-modal.primary",
        "assets.open-modal.editor",
        "assets.open-modal.manager",
        "assets.open-modal.with",
    ] {
        assert!(modal.root.find(id).is_some(), "missing {id}");
    }
}

#[test]
fn an_absolute_path_is_built_from_the_project_root_and_falls_back_to_the_row() {
    assert_eq!(
        asset_absolute_path(Some(Path::new("C:/project")), "scripts/player.rs"),
        std::path::Path::new("C:/project")
            .join("assets")
            .join("scripts/player.rs")
            .to_string_lossy()
    );
    assert_eq!(
        asset_absolute_path(None, "scripts/player.rs"),
        "assets/scripts/player.rs"
    );
    assert_eq!(
        asset_absolute_path(Some(Path::new("C:/project")), "builtin://primitive/cube"),
        "builtin://primitive/cube"
    );
}

#[test]
fn grid_columns_come_from_the_real_panel_width() {
    assert_eq!(asset_grid_columns(0.0), 1);
    assert_eq!(asset_grid_columns(f32::NAN), 1);
    assert_eq!(
        asset_grid_viewport_width(ASSET_SIDEBAR_WIDTH + 560.0),
        560.0
    );
    assert_eq!(asset_grid_viewport_width(40.0), 0.0);
    // 300 px leaves 276 px after padding: two 116 px cards plus one gap.
    assert_eq!(asset_grid_columns(300.0), 2);
    // 560 px leaves 536 px: four cards fit.
    assert_eq!(asset_grid_columns(560.0), 4);
    assert_eq!(asset_grid_row_count(0, 4), 0);
    assert_eq!(asset_grid_row_count(9, 4), 3);
    assert!(asset_grid_columns(1920.0) > asset_grid_columns(640.0));
}

#[test]
fn the_grid_window_is_a_row_aligned_card_range() {
    let rows: Vec<String> = (0..40)
        .map(|index| format!("models/tree{index:02}.glb"))
        .collect();
    let width = 560.0;
    let columns = asset_grid_columns(width);
    assert_eq!(columns, 4);
    let viewport = ASSET_CARD_PITCH * 3.0;
    let (start, end) = asset_visible_range_for_width(
        &rows,
        "",
        AssetFilter::Models,
        "",
        AssetSort::NameAsc,
        0.0,
        viewport,
        width,
        AssetViewMode::Grid,
    );
    assert_eq!(start, 0);
    assert!(end >= columns * 3, "a 3-row viewport materializes 3 rows");
    assert_eq!(end % columns, 0, "the window ends on a row boundary");
    assert!(end < 40);

    let (start, end) = asset_visible_range_for_width(
        &rows,
        "",
        AssetFilter::Models,
        "",
        AssetSort::NameAsc,
        ASSET_CARD_PITCH * 4.0,
        viewport,
        width,
        AssetViewMode::Grid,
    );
    assert!(start > 0);
    assert_eq!(start % columns, 0, "scrolled windows start on a row");
    assert_eq!(end % columns, 0);
}

#[test]
fn stale_scroll_offsets_are_clamped_to_the_last_grid_row() {
    let total_rows = 7;
    let (start, end) = asset_grid_row_range(
        total_rows,
        f32::MAX,
        ASSET_CARD_PITCH * 2.0,
        ASSET_CARD_PITCH,
        2,
    );
    assert!(start < total_rows);
    assert_eq!(end, total_rows);
}

#[test]
fn grid_spacers_speak_in_rows_not_cards() {
    let width = 560.0;
    let columns = asset_grid_columns(width);
    assert_eq!(columns, 4);
    // 8 skipped cards are exactly two rows of four cards.
    assert_eq!(
        asset_virtual_top_spacer_height(8, width, AssetViewMode::Grid),
        2.0 * ASSET_CARD_PITCH
    );
    assert_eq!(
        asset_virtual_bottom_spacer_height(8, width, AssetViewMode::Grid),
        2.0 * ASSET_CARD_PITCH - ASSET_CARD_GAP
    );
    // List spacers still treat every row as one card.
    assert_eq!(
        asset_virtual_top_spacer_height(8, width, AssetViewMode::List),
        8.0 * ASSET_LIST_PITCH
    );
}

#[test]
fn new_builtins_are_named_iconified_and_insertable() {
    let rows = asset_rows_with_builtins(&["models/tree.glb".to_string()]);
    assert_eq!(rows.len(), BUILTIN_ASSET_ROWS.len() + 1);
    for row in [
        "builtin://primitive/wedge",
        "builtin://primitive/torus",
        "builtin://primitive/prism",
        "builtin://primitive/sphere2d",
    ] {
        assert!(rows.iter().any(|value| value == row), "missing {row}");
        let slug = &row["builtin://primitive/".len()..];
        let primitive = crate::panels::primitive_create::parse_primitive_slug(slug).unwrap();
        assert_eq!(
            crate::panels::primitive_create::primitive_slug(primitive),
            slug
        );
        assert_eq!(asset_meta(row), (None, None));
    }
    assert_eq!(asset_file_name("builtin://primitive/wedge"), "Wedge");
    assert_eq!(
        asset_display_name("builtin://primitive/torus"),
        "Built-in / Torus"
    );
    assert_eq!(asset_icon("builtin://primitive/wedge"), UiIconId::Cube);
    assert_eq!(asset_icon("builtin://primitive/torus"), UiIconId::Sphere);
    assert_eq!(asset_file_name("builtin://light/point"), "Point Light");
    assert_eq!(
        asset_display_name("builtin://light/spot"),
        "Built-in / Spot Light"
    );
    assert_eq!(asset_icon("builtin://light/point"), UiIconId::Sun);
    assert!(is_builtin_light_row("builtin://light/spot"));
    assert_eq!(
        builtin_asset_command("builtin://light/point").as_deref(),
        Some("hierarchy.create-light:point")
    );
    assert_eq!(
        builtin_asset_command("builtin://light/spot").as_deref(),
        Some("hierarchy.create-light:spot")
    );
    assert!(builtin_asset_command("builtin://primitive/cube").is_none());
    assert!(rows.iter().any(|row| row == "builtin://light/point"));
}

fn click_commands(node: &raf_render::api_graphic_basic::ui_surface::UiNode) -> Vec<String> {
    node.event_handlers
        .iter()
        .filter(|binding| binding.event == raf_ui::UiEventKind::Click)
        .filter_map(|binding| match &binding.action {
            raf_ui::UiAction::Command { name } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn light_cards_emit_the_documented_create_commands_on_click() {
    let palette = StudioUiPalette::IndustrialDark;
    let point = super::catalog::asset_card(
        palette,
        0,
        "builtin://light/point",
        AssetViewMode::Grid,
        false,
        false,
        AssetsMotion::default(),
    );
    assert_eq!(
        click_commands(&point),
        vec!["hierarchy.create-light:point".to_string()]
    );
    let spot = super::catalog::asset_card(
        palette,
        0,
        "builtin://light/spot",
        AssetViewMode::Grid,
        false,
        false,
        AssetsMotion::default(),
    );
    assert_eq!(
        click_commands(&spot),
        vec!["hierarchy.create-light:spot".to_string()]
    );
}

#[test]
fn the_toolbar_import_button_emits_the_import_open_command() {
    let rows = vec!["scripts/player.rs".to_string()];
    let palette = StudioUiPalette::IndustrialDark;
    let surface = build_assets_surface(params(palette, &rows));
    let trigger = surface
        .root
        .find("assets.import-trigger")
        .expect("the toolbar exposes a primary Import button");
    assert_eq!(
        click_commands(trigger),
        vec!["assets.import-open".to_string()]
    );
    assert!(trigger
        .classes
        .iter()
        .any(|class| class == "asset-import-trigger"));
}

#[test]
fn the_import_modal_is_declarative_and_covers_every_option() {
    use super::preview::asset_card_lifecycle;
    use super::preview::AssetCardLifecycle;
    use crate::asset_previews::PreviewState;

    assert_eq!(
        asset_card_lifecycle(&PreviewState::Loading),
        AssetCardLifecycle::Importing
    );
    assert_eq!(
        asset_card_lifecycle(&PreviewState::Missing),
        AssetCardLifecycle::Missing
    );
    assert_eq!(
        asset_card_lifecycle(&PreviewState::Error("decode".to_string())),
        AssetCardLifecycle::Error
    );
    assert_eq!(
        asset_card_lifecycle(&PreviewState::Ready {
            key: "k".to_string(),
            dimensions: Some([1.0, 2.0, 3.0]),
        }),
        AssetCardLifecycle::Ready
    );

    let palette = StudioUiPalette::IndustrialDark;
    let options = AssetsImportOptions {
        name: "tree".to_string(),
        scale: 1.0,
        recenter: true,
        generate_collider: true,
        import_textures: false,
    };
    let state = AssetsImportModalState {
        file_name: Some("tree.glb"),
        source_path: Some("C:/project/assets/models/tree.glb"),
        file_size: Some(2048),
        options,
        phase: AssetsImportPhase::Importing { progress: 0.5 },
        preview: Some(PreviewState::Unavailable("assets.model_import_pending")),
        error: Some("Importer is not connected"),
    };
    let modal = build_assets_import_modal_surface(palette, &state);
    assert_eq!(modal.root.kind, raf_ui::UiNodeKind::FloatingPanel);
    assert_eq!(
        modal.root.accessibility_role,
        raf_ui::UiAccessibilityRole::Dialog
    );
    assert_eq!(modal.root.material, raf_ui::UiSurfaceMaterial::ModalSurface);
    for id in [
        "assets.import-modal.dropzone",
        "assets.import-modal.file",
        "assets.import-modal.preview",
        ASSETS_IMPORT_NAME_ID,
        "assets.import.scale",
        "assets.import-modal.options.recenter",
        "assets.import-modal.options.collider",
        "assets.import-modal.options.textures",
        "assets.import-modal.progress",
        "assets.import-modal.footer.confirm",
        "assets.import-modal.footer.cancel",
    ] {
        assert!(modal.root.find(id).is_some(), "missing {id}");
    }
    let name = modal.root.find(ASSETS_IMPORT_NAME_ID).unwrap();
    assert_eq!(
        name.control
            .text_input()
            .map(|input| input.value_key.as_str()),
        Some(ASSETS_IMPORT_NAME_ID)
    );
    assert!(modal
        .root
        .find("assets.import-modal.footer.confirm")
        .is_some_and(|confirm| confirm.disabled));
    assert!(modal.root.event_handlers.iter().any(|binding| {
        binding.event == raf_ui::UiEventKind::KeyPress("escape".to_string())
            && binding.action
                == raf_ui::UiAction::Command {
                    name: "assets.import.cancel".to_string(),
                }
    }));
    let backdrop = build_assets_import_backdrop_surface(palette);
    assert_eq!(
        backdrop.root.material,
        raf_ui::UiSurfaceMaterial::BackdropScrim
    );
}

#[test]
fn card_lifecycle_exposes_badges_and_tooltips() {
    use crate::asset_previews::PreviewState;
    use raf_assets::authoring_catalog::{AssetClass, AuthoringAsset};

    let palette = StudioUiPalette::IndustrialDark;
    let visual = AssetVisual {
        entry: AuthoringAsset {
            path: "models/tree.glb".to_string(),
            name: "tree.glb".to_string(),
            class: AssetClass::Model,
            usages: 2,
            dimensions: None,
        },
        preview: PreviewState::Missing,
        can_apply_texture: false,
    };
    let card = super::catalog::asset_card(
        palette,
        0,
        "models/tree.glb",
        AssetViewMode::Grid,
        false,
        false,
        AssetsMotion::default(),
    );
    let card = super::preview::decorate_asset_card(
        card,
        Some(&visual),
        AssetViewMode::Grid,
        raf_core::Language::English,
        palette,
    );
    assert!(card
        .classes
        .iter()
        .any(|class| class == "asset-card-state-missing"));
    assert!(card
        .children
        .iter()
        .any(|child| child.id == format!("{}.preview-state", card.id)));
    let tooltip = card.tooltip_value.unwrap();
    assert!(tooltip.contains("Triangles: —"));
    assert!(tooltip.contains("Materials: —"));
}
