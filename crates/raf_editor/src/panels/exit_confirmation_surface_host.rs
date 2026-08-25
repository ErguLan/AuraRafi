//! Native retained confirmation surface for leaving a project with changes.
//!
//! The surface only owns presentation and input dispatch. The application
//! decides whether the document is saved, discarded, or kept open.

use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction, UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiTextStyle,
};

use crate::editor_layout::EditorRect;

const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitConfirmationAction {
    Cancel,
    Discard,
    Save,
}

pub struct ExitConfirmationSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    last_rect: Option<EditorRect>,
}

impl ExitConfirmationSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        Self {
            region: InputRegionId::from_static("native.editor.exit-confirmation"),
            rect,
            host: graphics.create_ui_host(build_surface(palette), CLEAR),
            last_rect: None,
        }
    }

    pub fn sync(&mut self, palette: StudioUiPalette, rect: EditorRect) {
        self.rect = rect;
        if self.last_rect == Some(rect) {
            return;
        }
        self.host.set_surface(build_surface(palette));
        self.last_rect = Some(rect);
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Option<ExitConfirmationAction> {
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| t(key, Language::English),
            input,
            router,
            InputOwner::RetainedUi(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        actions.into_iter().find_map(action_from_dispatch)
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

fn action_from_dispatch(dispatched: UiDispatchedAction) -> Option<ExitConfirmationAction> {
    let UiAction::Command { name } = dispatched.action else {
        return None;
    };
    match name.as_str() {
        "exit-confirmation.cancel" => Some(ExitConfirmationAction::Cancel),
        "exit-confirmation.discard" => Some(ExitConfirmationAction::Discard),
        "exit-confirmation.save" => Some(ExitConfirmationAction::Save),
        _ => None,
    }
}

fn build_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let card = UiNode::new("exit-confirmation.card", UiNodeKind::Panel)
        .with_class("exit-confirmation-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 12.0,
            padding: UiSpacing::xy(24.0, 20.0),
            ..UiLayout::fixed(560.0, 184.0)
        })
        .with_child(
            UiNode::new("exit-confirmation.title", UiNodeKind::Label)
                .with_text_key("app.unsaved_changes_title")
                .with_layout(UiLayout::fixed(0.0, 28.0))
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("exit-confirmation.message", UiNodeKind::Label)
                .with_text_key("app.unsaved_changes_message")
                .with_layout(UiLayout::fixed(0.0, 56.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("exit-confirmation.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::End,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 36.0)
                })
                .with_child(button(
                    "exit-confirmation.cancel",
                    "app.cancel",
                    "exit-confirmation.cancel",
                    "exit-confirmation-secondary",
                    tokens.text,
                ))
                .with_child(button(
                    "exit-confirmation.discard",
                    "app.discard_and_exit",
                    "exit-confirmation-danger",
                    "exit-confirmation.discard",
                    tokens.text,
                ))
                .with_child(button(
                    "exit-confirmation.save",
                    "app.save_and_exit",
                    "exit-confirmation-primary",
                    "exit-confirmation.save",
                    [18, 18, 20, 255],
                )),
        );

    let root = UiNode::new("exit-confirmation.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle {
            fill: [0, 0, 0, 156],
            border: [0, 0, 0, 0],
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_child(card);

    let mut surface = UiSurface::new("exit-confirmation", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("exit-confirmation-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(8.0),
                    ..UiStylePatch::default()
                },
            ),
            button_rule(
                "exit-confirmation-secondary",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            button_rule(
                "exit-confirmation-danger",
                tokens.surface_alt,
                tokens.danger,
                tokens.text,
            ),
            button_rule(
                "exit-confirmation-primary",
                tokens.accent,
                tokens.accent_hot,
                [18, 18, 20, 255],
            ),
            hover_rule("exit-confirmation-secondary", tokens.surface_raised),
            hover_rule("exit-confirmation-danger", tokens.surface_raised),
            hover_rule("exit-confirmation-primary", tokens.accent_hot),
        ],
    };
    surface
}

fn button(id: &str, label_key: &str, class: &str, command: &str, text_color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(text_color))
        .with_layout(UiLayout {
            min_size: [112.0, 32.0],
            padding: UiSpacing::xy(10.0, 5.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn button_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            border_width: Some(1.0),
            radius: Some(4.0),
            text: Some(text),
            ..UiStylePatch::default()
        },
    )
}

fn hover_rule(class: &str, fill: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Hovered)
}
