//! Tests for the Assets panel surface and its catalog helpers.

use std::path::Path;

use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiIconId, UiStyleSelector};

use super::catalog::{
    asset_display_name, asset_icon, asset_matches, asset_matches_script_extension, asset_meta,
    asset_virtual_spacer_height, sort_asset_rows,
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
        let (start, end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            0.0,
            ASSET_CARD_PITCH * 3.0,
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
        let (_, end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            0.0,
            viewport,
            AssetViewMode::Grid,
        );
        let (start, next_end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            ASSET_CARD_PITCH * 10.0,
            viewport,
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
            asset_virtual_spacer_height(2, AssetViewMode::List),
            2.0 * ASSET_LIST_PITCH - ASSET_CARD_GAP
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
        let modal = build_assets_open_modal_surface(
            palette,
            "scripts/player.rs",
            "assets/scripts/player.rs",
        );
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
