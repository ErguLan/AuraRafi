//! Shared retained RafUI confirmation dialog.
//!
//! The application still owns the modal decision and persistence side effects;
//! this surface only presents the stable Save, Discard, and Cancel actions.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonDialogAction {
    Cancel,
    Discard,
    Save,
}

pub struct CommonDialogSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for CommonDialogSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_common_dialog"),
        }
    }
}

impl CommonDialogSurfaceHost {
    pub fn show_unsaved(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        title_key: &str,
        message_key: &str,
        lang: Language,
    ) -> Option<CommonDialogAction> {
        let surface = build_unsaved_dialog_surface(palette, title_key, message_key);
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .find_map(|dispatched| match dispatched.action {
                UiAction::Command { name } => match name.as_str() {
                    "common-dialog.cancel" => Some(CommonDialogAction::Cancel),
                    "common-dialog.discard" => Some(CommonDialogAction::Discard),
                    "common-dialog.save" => Some(CommonDialogAction::Save),
                    _ => None,
                },
                _ => None,
            })
    }
}

fn build_unsaved_dialog_surface(
    palette: StudioUiPalette,
    title_key: &str,
    message_key: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("common-dialog.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(16.0),
            gap: 10.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("common-dialog.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(
            UiNode::new("common-dialog.message", UiNodeKind::Label)
                .with_text_key(message_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::Column)
                }),
        )
        .with_child(
            UiNode::new("common-dialog.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: raf_ui::UiJustify::End,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 32.0)
                })
                .with_child(dialog_button(
                    "common-dialog.cancel",
                    "app.cancel",
                    "common-dialog.cancel",
                    "common-dialog-secondary",
                    tokens.text,
                ))
                .with_child(dialog_button(
                    "common-dialog.discard",
                    "app.discard_changes",
                    "common-dialog.discard",
                    "common-dialog-danger",
                    tokens.text,
                ))
                .with_child(dialog_button(
                    "common-dialog.save",
                    "app.save_changes",
                    "common-dialog.save",
                    "common-dialog-primary",
                    tokens.text,
                )),
        );

    let mut surface = UiSurface::new("common-unsaved-dialog", palette, root);
    surface.style_sheet = common_dialog_style_sheet(palette);
    surface
}

fn dialog_button(
    id: &str,
    label_key: &str,
    command: &str,
    class: &str,
    text_color: [u8; 4],
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(text_color))
        .with_layout(UiLayout {
            min_size: [92.0, 28.0],
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn common_dialog_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Root),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("common-dialog-secondary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("common-dialog-danger".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.danger),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("common-dialog-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("common-dialog-secondary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("common-dialog-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}
