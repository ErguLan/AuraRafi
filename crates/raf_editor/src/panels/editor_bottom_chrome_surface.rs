//! Retained utility controls for the editor bottom dock.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiSpacing, UiStylePatch, UiStyleRule, UiStyleSelector, UiStyleSheet, UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, PartialEq)]
pub enum EditorBottomChromeAction {
    SetSnap(Option<f32>),
    SelectComplement(String),
}

pub struct EditorBottomChromeSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for EditorBottomChromeSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_bottom_chrome"),
        }
    }
}

impl EditorBottomChromeSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        snap_height: Option<f32>,
        complements: &[(String, String)],
        active_complement: Option<&str>,
    ) -> Vec<EditorBottomChromeAction> {
        let surface = build_surface(palette, snap_height, complements, active_complement);
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, language))
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::Command { name } => parse_command(&name),
                _ => None,
            })
            .collect()
    }
}

fn build_surface(
    palette: StudioUiPalette,
    snap_height: Option<f32>,
    complements: &[(String, String)],
    active_complement: Option<&str>,
) -> UiSurface {
    let mut root = UiNode::new("bottom-chrome.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(4.0, 2.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style())
        .with_child(
            UiNode::new("bottom-chrome.spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        );

    for (index, (height, label)) in [
        (None, "F"),
        (Some(340.0), "L"),
        (Some(220.0), "M"),
        (Some(110.0), "S"),
    ]
    .into_iter()
    .enumerate()
    {
        root = root.with_child(utility_button(
            format!("bottom-chrome.snap.{index}"),
            label.to_string(),
            format!(
                "bottom.snap:{}",
                height.map_or_else(|| "full".to_string(), |v| v.to_string())
            ),
            snap_height == height,
            palette,
        ));
    }

    for (index, (id, label)) in complements.iter().enumerate() {
        root = root.with_child(utility_button(
            format!("bottom-chrome.complement.{index}"),
            label.clone(),
            format!("bottom.complement:{id}"),
            active_complement == Some(id.as_str()),
            palette,
        ));
    }

    let mut surface = UiSurface::new("editor-bottom-chrome", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn utility_button(
    id: String,
    label: String,
    command: String,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "bottom-chrome-active"
        } else {
            "bottom-chrome-button"
        })
        .with_text_key(label)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [24.0, 24.0],
            padding: UiSpacing::xy(7.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn parse_command(command: &str) -> Option<EditorBottomChromeAction> {
    if let Some(value) = command.strip_prefix("bottom.snap:") {
        return Some(EditorBottomChromeAction::SetSnap(if value == "full" {
            None
        } else {
            value.parse().ok()
        }));
    }
    command
        .strip_prefix("bottom.complement:")
        .map(|id| EditorBottomChromeAction::SelectComplement(id.to_string()))
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Root),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-chrome-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-chrome-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}
