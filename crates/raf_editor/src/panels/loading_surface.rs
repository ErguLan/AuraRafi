//! Native retained loading surface.
//!
//! The loading document used to be placed through a legacy bridge. Keeping
//! the document and the tiny host here preserves the splash UX while the
//! native Winit application initializes the Hub.

use std::time::{SystemTime, UNIX_EPOCH};

use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, StudioUiPalette, UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiImage, UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleSelector,
    UiStyleSheet, UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::editor_layout::EditorRect;

const CLEAR: [u8; 4] = [8, 11, 15, 255];
const LOADING_MESSAGES: [&str; 3] = [
    "Even the IDE we used to build this engine is heavier than the engine itself.",
    "Work in progress.",
    "World and UI design matter just as much as gameplay.",
];

pub struct LoadingSurfaceHost {
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    last_key: Option<LoadingKey>,
    loading_message: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
struct LoadingKey {
    rect: EditorRect,
    palette: StudioUiPalette,
    progress: u16,
    language: Language,
}

impl LoadingSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let loading_message = select_loading_message();
        let surface = build_surface(palette, 0.0, Language::English, loading_message);
        let mut host = graphics.create_ui_host(surface, CLEAR);
        if let Ok(decoded) = image::load_from_memory(include_bytes!("../../../../editor/icon.png"))
        {
            let decoded = decoded.to_rgba8();
            let _ = host.images_mut().insert_rgba(
                "loading.brand-mark".to_string(),
                [decoded.width(), decoded.height()],
                decoded.into_raw(),
            );
        }
        Self {
            rect,
            host,
            last_key: None,
            loading_message,
        }
    }

    pub fn sync(
        &mut self,
        rect: EditorRect,
        palette: StudioUiPalette,
        progress: f32,
        language: Language,
    ) {
        let key = LoadingKey {
            rect,
            palette,
            progress: (progress.clamp(0.0, 1.0) * 1000.0).round() as u16,
            language,
        };
        self.rect = rect;
        if self.last_key.as_ref() == Some(&key) {
            return;
        }
        self.host.set_surface(build_surface(
            palette,
            progress,
            language,
            self.loading_message,
        ));
        self.last_key = Some(key);
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }
}

fn select_loading_message() -> &'static str {
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as usize)
        .unwrap_or_default();
    LOADING_MESSAGES[tick % LOADING_MESSAGES.len()]
}

fn build_surface(
    palette: StudioUiPalette,
    progress: f32,
    language: Language,
    loading_message: &'static str,
) -> UiSurface {
    let tokens = palette.tokens();
    let progress = progress.clamp(0.0, 1.0);
    let progress_label = format!("{:.0}%", progress * 100.0);
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
                        .with_text_value(t("app.loading_workspace", language))
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
