//! Retained RafUI status bar for the editor workbench.

use eframe::{egui, egui_wgpu};
use raf_core::config::{Language, Theme};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAlign, UiFlow, UiLayout, UiNode, UiNodeKind, UiSpacing, UiStylePatch,
    UiStyleRule, UiStyleSelector, UiStyleSheet, UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct EditorStatusSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for EditorStatusSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_status"),
        }
    }
}

impl EditorStatusSurfaceHost {
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        project: Option<(String, String)>,
        status_counts: String,
        status_color: [u8; 4],
        undo_count: usize,
        redo_count: usize,
        last_action: &str,
        language: Language,
        theme: Theme,
    ) {
        let surface = build_surface(
            palette,
            project,
            status_counts,
            status_color,
            undo_count,
            redo_count,
            last_action,
            language,
            theme,
        );
        let _ = self
            .bridge
            .show(ui, render_state, palette, surface, |key| key.to_string());
    }
}

#[allow(clippy::too_many_arguments)]
fn build_surface(
    palette: StudioUiPalette,
    project: Option<(String, String)>,
    status_counts: String,
    status_color: [u8; 4],
    undo_count: usize,
    redo_count: usize,
    last_action: &str,
    language: Language,
    theme: Theme,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("editor.status.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 2.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style());

    if let Some((name, kind)) = project {
        root = root
            .with_child(status_label("editor.status.project", name, tokens.accent))
            .with_child(separator(palette))
            .with_child(status_label("editor.status.type", kind, tokens.text_muted));
    }
    root = root
        .with_child(separator(palette))
        .with_child(status_label(
            "editor.status.counts",
            status_counts,
            status_color,
        ))
        .with_child(separator(palette))
        .with_child(status_label(
            "editor.status.history",
            format!("U:{undo_count} R:{redo_count}"),
            tokens.text_muted,
        ));

    if !last_action.is_empty() {
        root = root.with_child(separator(palette)).with_child(status_label(
            "editor.status.action",
            last_action.to_string(),
            tokens.text,
        ));
    }

    root = root.with_child(
        UiNode::new("editor.status.spacer", UiNodeKind::Panel).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::default()
        }),
    );
    let theme_name = match theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
        Theme::System => "System",
    };
    root = root.with_child(status_label(
        "editor.status.locale",
        format!("{} | {theme_name}", language.display_name()),
        tokens.text_muted,
    ));

    let mut surface = UiSurface::new("editor-status", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn status_label(id: &str, text: String, color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(text)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fixed(0.0, 20.0))
}

fn separator(palette: StudioUiPalette) -> UiNode {
    UiNode::new("editor.status.separator", UiNodeKind::Separator)
        .with_layout(UiLayout::fixed(1.0, 14.0))
        .with_style(raf_ui::UiStyle {
            fill: palette.tokens().border,
            border: palette.tokens().border,
            text: palette.tokens().border,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![UiStyleRule::new(
            UiStyleSelector::Kind(UiNodeKind::Root),
            UiStylePatch {
                fill: Some(tokens.surface),
                border: Some(tokens.border),
                border_width: Some(1.0),
                ..UiStylePatch::default()
            },
        )],
    }
}
