//! Retained RafUI actions and status indicators for the editor context row.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout,
    UiNode, UiNodeKind, UiSpacing, UiStylePatch, UiStyleRule, UiStyleSelector, UiStyleSheet,
    UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorContextAction {
    Build,
    Undo,
    Redo,
}

pub struct EditorContextActionsSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for EditorContextActionsSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_context_actions"),
        }
    }
}

impl EditorContextActionsSurfaceHost {
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        electronics_project: bool,
        can_undo: bool,
        can_redo: bool,
        show_fps: bool,
        fps: u32,
        saved: bool,
        mode_label: String,
    ) -> Vec<EditorContextAction> {
        let surface = build_surface(
            palette,
            electronics_project,
            can_undo,
            can_redo,
            show_fps,
            fps,
            saved,
            mode_label,
            language,
        );
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, language))
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::Command { name } if name == "editor.context.build" => {
                    Some(EditorContextAction::Build)
                }
                UiAction::Command { name } if name == "editor.context.undo" => {
                    Some(EditorContextAction::Undo)
                }
                UiAction::Command { name } if name == "editor.context.redo" => {
                    Some(EditorContextAction::Redo)
                }
                _ => None,
            })
            .collect()
    }

    /// Compact Game status row. This is display-only by design: the current
    /// editor has no connected Play/Run action to expose here.
    pub fn show_game_status(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        scene_name: String,
        show_fps: bool,
        fps: u32,
    ) {
        let surface = build_game_status_surface(palette, language, scene_name, show_fps, fps);
        let _ = self
            .bridge
            .show(ui, render_state, palette, surface, |key| t(key, language));
    }
}

fn build_game_status_surface(
    palette: StudioUiPalette,
    language: Language,
    scene_name: String,
    show_fps: bool,
    fps: u32,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("editor.game-status.root", UiNodeKind::Root)
        .with_class("editor-game-status")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::End,
            gap: 14.0,
            padding: UiSpacing::xy(8.0, 3.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style())
        .with_child(label_node(
            "editor.game-status.scene",
            format!("{}: {scene_name}", t("app.scene", language)),
            tokens.text,
        ));

    if show_fps {
        root = root.with_child(label_node(
            "editor.game-status.fps",
            format!("{} {fps}", t("app.fps", language)),
            tokens.text_muted,
        ));
    }

    let mut surface = UiSurface::new("editor-game-status", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn build_surface(
    palette: StudioUiPalette,
    electronics_project: bool,
    can_undo: bool,
    can_redo: bool,
    show_fps: bool,
    fps: u32,
    saved: bool,
    mode_label: String,
    language: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let build_key = if electronics_project {
        "app.electrical_test_btn"
    } else {
        "app.runtime_temporarily_disabled_btn"
    };
    let mut root = UiNode::new("editor.context.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::End,
            gap: 5.0,
            padding: UiSpacing::xy(6.0, 3.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style())
        .with_child(command_button(
            "editor.context.build",
            build_key,
            "editor.context.build",
            "editor-context-primary",
            false,
            palette,
        ));

    if electronics_project {
        root = root
            .with_child(command_button(
                "editor.context.undo",
                "app.undo",
                "editor.context.undo",
                "editor-context-button",
                !can_undo,
                palette,
            ))
            .with_child(command_button(
                "editor.context.redo",
                "app.redo",
                "editor.context.redo",
                "editor-context-button",
                !can_redo,
                palette,
            ));
    }

    if show_fps {
        root = root.with_child(label_node(
            "editor.context.fps",
            format!("{}: {fps}", t("app.fps", language)),
            tokens.text_muted,
        ));
    }
    if electronics_project {
        root = root.with_child(label_node(
            "editor.context.saved",
            if saved {
                t("app.electronics_saved", language)
            } else {
                t("app.hub_modified", language)
            },
            if saved {
                [104, 204, 132, 255]
            } else {
                tokens.text_muted
            },
        ));
    }
    root = root.with_child(label_node("editor.context.mode", mode_label, tokens.accent));

    let mut surface = UiSurface::new("editor-context-actions", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn label_node(id: &str, text: String, color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(text)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fixed(0.0, 26.0))
}

fn command_button(
    id: &str,
    label_key: &str,
    command: &str,
    class: &str,
    disabled: bool,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [46.0, 26.0],
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::default()
        })
        .disabled(disabled)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
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
                UiStyleSelector::Class("editor-game-status".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-context-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-context-primary".to_string()),
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
                UiStyleSelector::Kind(UiNodeKind::Button),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Disabled),
        ],
    }
}
