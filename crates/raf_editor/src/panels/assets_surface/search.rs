//! Query field, empty states and category tabs for the Assets panel.
//!
//! Moved from the surface facade without behavior changes.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiCompactMode, UiEventBinding, UiEventKind,
    UiFlow, UiFontWeight, UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiSizeMode, UiSpacing, UiStyle, UiTextOverflow, UiTextRole, UiTextStyle,
};

use super::{AssetFilter, ASSET_SIDEBAR_WIDTH};

pub(super) fn empty_state(
    palette: StudioUiPalette,
    no_rows: bool,
    query: &str,
    filter: AssetFilter,
) -> UiNode {
    let tokens = palette.tokens();
    let title_key = if !query.trim().is_empty() {
        "app.search_no_results"
    } else {
        filter.empty_title_key()
    };
    let empty = UiNode::new("assets.empty", UiNodeKind::Panel)
        .with_class("assets-empty")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            min_size: [0.0, 96.0],
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("assets.empty.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if filter == AssetFilter::Scripts {
                        UiIconId::Script
                    } else {
                        UiIconId::Assets
                    })
                    .with_size(UiIconSize::Custom(28))
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.empty.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::body(tokens.text)),
        );
    let hint_key = if !query.trim().is_empty() {
        "app.search_no_results_hint"
    } else if no_rows {
        "app.drag_drop_hint"
    } else {
        "app.assets_empty_hint"
    };
    empty.with_child(
        UiNode::new("assets.empty.hint", UiNodeKind::Label)
            .with_text_key(hint_key)
            .with_layout(UiLayout::fit_content())
            .with_text_style(UiTextStyle::body(tokens.text_muted)),
    )
}

pub(super) fn asset_sidebar(
    palette: StudioUiPalette,
    language: raf_core::Language,
    active: AssetFilter,
    script_extension: &str,
    asset_count: usize,
) -> UiNode {
    let tokens = palette.tokens();
    let mut sidebar = UiNode::new("assets.sidebar", UiNodeKind::Panel)
        .with_class("assets-sidebar")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [ASSET_SIDEBAR_WIDTH, 0.0],
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fill,
            min_size: [ASSET_SIDEBAR_WIDTH, 0.0],
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 10.0),
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::text_input(
                "assets.search",
                raf_ui::UiTextInput {
                    value_key: "assets.search".to_string(),
                    placeholder_key: Some("app.search".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("asset-search")
            .with_icon(UiIcon::new(UiIconId::Search).with_size(UiIconSize::Small))
            .with_accessibility_label_key("app.search")
            .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("assets.sidebar.title", UiNodeKind::Label)
                .with_text_key("app.assets_category_title")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        );
    for (id, icon, label_key) in [
        ("all", UiIconId::Assets, "app.assets_category_all"),
        ("models", UiIconId::Folder, "app.models"),
        ("images", UiIconId::Shaded, "app.images"),
        ("audio", UiIconId::Play, "app.audio"),
        ("scripts", UiIconId::Script, "app.scripts_filter"),
        ("interfaces", UiIconId::Frame, "ui.designer.title"),
        ("animations", UiIconId::Play, "animation.tab"),
    ] {
        sidebar = sidebar.with_child(asset_category_row(
            palette,
            id,
            icon,
            label_key,
            matches!(
                (id, active),
                ("all", AssetFilter::All)
                    | ("models", AssetFilter::Models)
                    | ("images", AssetFilter::Images)
                    | ("audio", AssetFilter::Audio)
                    | ("scripts", AssetFilter::Scripts)
                    | ("interfaces", AssetFilter::Interfaces)
                    | ("animations", AssetFilter::Animations)
            ),
        ));
    }
    if active == AssetFilter::Scripts {
        sidebar = sidebar.with_child(script_extension_chips(script_extension));
    }
    sidebar.with_child(
        UiNode::new("assets.sidebar.count", UiNodeKind::Label)
            .with_class("asset-sidebar-count")
            .with_text_value(format_count("app.assets_count", asset_count, language))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)),
    )
}

fn script_extension_chips(active_extension: &str) -> UiNode {
    let active = if active_extension.is_empty() {
        "all"
    } else {
        active_extension
    };
    let mut chips = UiNode::new("assets.script-languages", UiNodeKind::Panel)
        .with_class("asset-script-languages")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Wrap,
            gap: 4.0,
            padding: UiSpacing::xy(0.0, 2.0),
            // Fixed height on purpose: a wrapped row reports no intrinsic
            // height, which would let the footer count overlap the last chip.
            ..UiLayout::fixed(0.0, 52.0).with_width_mode(UiSizeMode::Fill)
        });
    for (extension, label_key) in [
        ("all", "app.assets_lang_all"),
        ("rs", "app.assets_lang_rust"),
        ("cpp", "app.assets_lang_cpp"),
        ("rhai", "app.assets_lang_rhai"),
        ("lua", "app.assets_lang_lua"),
        ("py", "app.assets_lang_python"),
    ] {
        chips = chips.with_child(
            UiNode::new(format!("assets.script-ext.{extension}"), UiNodeKind::Button)
                .with_class(if active == extension {
                    "asset-chip asset-chip-active"
                } else {
                    "asset-chip"
                })
                .with_text_key(label_key)
                .with_accessibility_label_key(label_key)
                .with_layout(UiLayout::fixed(84.0, 22.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("assets.script-ext:{extension}"),
                )),
        );
    }
    chips
}

fn asset_category_row(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    active: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let command = format!("assets.filter.{id}");
    UiNode::new(format!("assets.sidebar.row.{id}"), UiNodeKind::Button)
        .with_class(if active {
            "asset-category asset-category-active"
        } else {
            "asset-category"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(if active {
                    tokens.accent
                } else {
                    tokens.text_muted
                }),
        )
        .with_text_key(label_key)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_accessibility_selected(active)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command.clone()))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
}

/// Translates `key` and substitutes its `{count}` placeholder.
fn format_count(key: &str, count: usize, language: raf_core::Language) -> String {
    raf_core::i18n::t(key, language).replace("{count}", &count.to_string())
}
