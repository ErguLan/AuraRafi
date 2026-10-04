//! Shared startup and runtime loading document. Geometry and branding have one owner.
use super::{DirectUiSurfaceHost, StudioUiPalette, UiSurface};
use raf_core::{config::Language, i18n::t};
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiImage, UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleSelector,
    UiStyleSheet, UiTextOverflow, UiTextRole, UiTextStyle,
};

pub fn insert_loading_brand(host: &mut DirectUiSurfaceHost) {
    if let Ok(decoded) = image::load_from_memory(include_bytes!("../../../../../editor/icon.png")) {
        let decoded = decoded.to_rgba8();
        let _ = host.images_mut().insert_rgba(
            "loading.brand-mark",
            [decoded.width(), decoded.height()],
            decoded.into_raw(),
        );
    }
}
pub fn build_loading_surface(
    palette: StudioUiPalette,
    progress: f32,
    language: Language,
    loading_message: &str,
    status: &str,
    progress_label: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let progress = progress.clamp(0.0, 1.0);
    let status_width = if language == Language::Spanish {
        250.0
    } else {
        178.0
    };
    let track = UiNode::new("loading.progress.track", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(380.0, 6.0))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 0.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("loading.progress.fill", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(380.0 * progress, 6.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 3.0,
                    opacity: 1.0,
                }),
        );
    let card = UiNode::new("loading.card", UiNodeKind::Panel)
        .with_class("loading-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(36.0, 20.0),
            ..UiLayout::fixed(620.0, 500.0)
        })
        .with_child(
            UiNode::image(
                "loading.brand-mark",
                UiImage {
                    source: UiImageSource::new("loading.brand-mark"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(200.0, 200.0)),
        )
        .with_child(
            UiNode::new("loading.brand", UiNodeKind::Label)
                .with_text_key("RAFI")
                .with_layout(UiLayout::fixed(150.0, 80.0))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Brand,
                    size_px: 72.0,
                    line_height_px: 80.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(
            UiNode::new("loading.message.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::Center,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(548.0, 48.0)
                })
                .with_child(
                    UiNode::new("loading.message", UiNodeKind::Label)
                        .with_text_value(loading_message)
                        .with_layout(UiLayout {
                            width_mode: UiSizeMode::FitContent,
                            height_mode: UiSizeMode::FitContent,
                            max_size: [540.0, 48.0],
                            ..UiLayout::default()
                        })
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 19.0,
                            line_height_px: 0.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text,
                            inherit_color: false,
                        }),
                ),
        )
        .with_child(
            UiNode::new("loading.signature", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(36.0, 2.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 1.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("loading.status.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::Center,
                    ..UiLayout::fixed(0.0, 24.0)
                })
                .with_child(
                    UiNode::new("loading.status", UiNodeKind::Label)
                        .with_text_value(status)
                        .with_layout(UiLayout::fixed(status_width, 24.0))
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 18.0,
                            line_height_px: 24.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text_muted,
                            inherit_color: false,
                        }),
                ),
        )
        .with_child(
            UiNode::new("loading.progress.row", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 12.0,
                    ..UiLayout::fixed(440.0, 20.0)
                })
                .with_child(track)
                .with_child(
                    UiNode::new("loading.progress.label", UiNodeKind::Label)
                        .with_text_value(progress_label)
                        .with_layout(UiLayout::fixed(48.0, 20.0))
                        .with_text_style(UiTextStyle::body(tokens.accent)),
                ),
        )
        .with_child(
            UiNode::new("loading.engine", UiNodeKind::Label)
                .with_text_value(t("app.engine", language))
                .with_layout(UiLayout::fixed(56.0, 20.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("loading.version", UiNodeKind::Label)
                .with_text_value(format!("v{}", env!("CARGO_PKG_VERSION")))
                .with_layout(UiLayout::fixed(56.0, 20.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    let root = UiNode::new("loading.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(card);
    let mut surface = UiSurface::new("loading", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![UiStyleRule::new(
            UiStyleSelector::Class("loading-card".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(10.0),
                ..UiStylePatch::default()
            },
        )],
    };
    surface
}
