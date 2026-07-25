//! Retained RafUI loading surface used before the project hub.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAlign, UiFlow, UiFontWeight, UiImage, UiImageFit, UiImageSource, UiJustify,
    UiLayout, UiNode, UiNodeKind, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleSelector,
    UiStyleSheet, UiTextRole, UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct LoadingSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for LoadingSurfaceHost {
    fn default() -> Self {
        let mut bridge = RafUiSurfaceBridge::new("raf_ui_loading");
        let _ = bridge.register_embedded_png(
            "loading.brand-mark",
            include_bytes!("../../../../editor/icon.png"),
        );
        Self { bridge }
    }
}

impl LoadingSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        progress: f32,
        lang: Language,
    ) {
        let surface = build_surface(palette, progress, lang);
        let _ = self
            .bridge
            .show_transparent(ui, render_state, palette, surface, |key| t(key, lang));
    }
}

fn build_surface(palette: StudioUiPalette, progress: f32, lang: Language) -> UiSurface {
    let tokens = palette.tokens();
    let progress = progress.clamp(0.0, 1.0);
    let progress_label = format!("{:.0}%", progress * 100.0);
    let status_width = match lang {
        Language::English => 178.0,
        Language::Spanish => 250.0,
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
            gap: 8.0,
            padding: UiSpacing::xy(36.0, 28.0),
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
                }),
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
                        .with_text_key(t("app.loading_workspace", lang))
                        .with_layout(UiLayout::fixed(status_width, 24.0))
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 18.0,
                            line_height_px: 24.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text_muted,
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
                        .with_text_key(progress_label)
                        .with_layout(UiLayout::fixed(48.0, 20.0))
                        .with_text_style(UiTextStyle::body(tokens.accent)),
                ),
        )
        .with_child(
            UiNode::new("loading.engine", UiNodeKind::Label)
                .with_text_key(t("app.engine", lang))
                .with_layout(UiLayout::fixed(56.0, 20.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("loading.version", UiNodeKind::Label)
                .with_text_key(format!("v{}", env!("CARGO_PKG_VERSION")))
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
    surface.style_sheet = style_sheet(palette);
    surface
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_by_id<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| node_by_id(child, id))
    }

    #[test]
    fn splash_text_and_percentage_reserve_nonzero_layout_tracks() {
        let surface = build_surface(StudioUiPalette::IndustrialDark, 0.63, Language::Spanish);

        for id in [
            "loading.brand",
            "loading.status",
            "loading.progress.label",
            "loading.engine",
            "loading.version",
        ] {
            let node = node_by_id(&surface.root, id).expect("splash node must exist");
            assert!(node.layout.basis[0] > 0.0, "{id} needs a width");
            assert!(node.layout.basis[1] > 0.0, "{id} needs a height");
        }
    }
}
