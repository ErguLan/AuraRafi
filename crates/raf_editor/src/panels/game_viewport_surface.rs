//! Retained RafUI controls for the Game viewport toolbar.
//!
//! Rendering, picking, gizmo math, and camera state remain in `ViewportPanel`.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_render::gizmo::GizmoMode;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiImage, UiImageFit,
    UiImageSource, UiLayout, UiNode, UiNodeKind, UiSpacing, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextStyle, UiToggle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::viewport::{EditMode, RenderStyle, ViewportMode, ViewportPanel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameViewportSurfaceAction {
    SetGizmo(GizmoMode),
    ToggleSelect,
    SetMode(ViewportMode),
    ToggleGrid,
    ToggleLabels,
    ToggleFocusLock,
    ToggleEditMode,
    SetRenderStyle(RenderStyle),
    ResetView,
    Undo,
    Redo,
}

pub struct GameViewportSurfaceHost {
    toolbar_bridge: RafUiSurfaceBridge,
    top_overlay_bridge: RafUiSurfaceBridge,
    bottom_overlay_bridge: RafUiSurfaceBridge,
    icons_registered: bool,
}

impl Default for GameViewportSurfaceHost {
    fn default() -> Self {
        Self {
            toolbar_bridge: RafUiSurfaceBridge::new("raf_ui_game_viewport_toolbar"),
            top_overlay_bridge: RafUiSurfaceBridge::new("raf_ui_game_viewport_top_overlay"),
            bottom_overlay_bridge: RafUiSurfaceBridge::new("raf_ui_game_viewport_bottom_overlay"),
            icons_registered: false,
        }
    }
}

impl GameViewportSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        viewport: &ViewportPanel,
        lang: Language,
    ) -> Vec<GameViewportSurfaceAction> {
        self.register_icons();
        let surface = build_viewport_surface(palette, viewport, lang);
        self.toolbar_bridge
            .show_transparent(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::Command { name } => parse_command(&name),
                UiAction::SetToggle { key, .. } => parse_command(&key),
                _ => None,
            })
            .collect()
    }

    /// Presents the compact view-mode controls inside the viewport itself.
    ///
    /// This intentionally uses a second bridge: each retained surface owns a
    /// texture, interaction state, and tooltip canvas. Reusing the toolbar
    /// bridge here would replace the top row's retained texture on the same
    /// frame and make one of the two surfaces appear stale or duplicated.
    pub fn show_top_overlay(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        viewport: &ViewportPanel,
        lang: Language,
    ) -> Vec<GameViewportSurfaceAction> {
        self.register_icons();
        let surface = build_viewport_top_overlay_surface(palette, viewport, lang);
        self.top_overlay_bridge
            .show_transparent(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::Command { name } => parse_command(&name),
                UiAction::SetToggle { key, .. } => parse_command(&key),
                _ => None,
            })
            .collect()
    }

    /// Presents the compact camera/history controls at the lower edge of the
    /// viewport. It has an independent retained bridge because each surface
    /// owns its texture, input state, and tooltip state.
    pub fn show_bottom_overlay(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        viewport: &ViewportPanel,
        lang: Language,
    ) -> Vec<GameViewportSurfaceAction> {
        self.register_icons();
        let surface = build_viewport_bottom_overlay_surface(palette, viewport);
        self.bottom_overlay_bridge
            .show_transparent(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::Command { name } => parse_command(&name),
                UiAction::SetToggle { key, .. } => parse_command(&key),
                _ => None,
            })
            .collect()
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        for bridge in [
            &mut self.toolbar_bridge,
            &mut self.top_overlay_bridge,
            &mut self.bottom_overlay_bridge,
        ] {
            register_viewport_icons(bridge);
        }
        self.icons_registered = true;
    }
}

fn register_viewport_icons(bridge: &mut RafUiSurfaceBridge) {
    for (key, bytes) in [
        (
            "game.viewport.icon.move",
            include_bytes!("../../../../editor/assets/ui_icons/move.png").as_slice(),
        ),
        (
            "game.viewport.icon.rotate",
            include_bytes!("../../../../editor/assets/ui_icons/rotate.png").as_slice(),
        ),
        (
            "game.viewport.icon.scale",
            include_bytes!("../../../../editor/assets/ui_icons/gizmo_scale.png").as_slice(),
        ),
        (
            "game.viewport.icon.select",
            include_bytes!("../../../../editor/assets/ui_icons/select.png").as_slice(),
        ),
        (
            "game.viewport.icon.grid",
            include_bytes!("../../../../editor/assets/ui_icons/grid.png").as_slice(),
        ),
        (
            "game.viewport.icon.focus",
            include_bytes!("../../../../editor/assets/ui_icons/focus.png").as_slice(),
        ),
        (
            "game.viewport.icon.object-mode",
            include_bytes!("../../../../editor/assets/ui_icons/object_mode.png").as_slice(),
        ),
        (
            "game.viewport.icon.reset",
            include_bytes!("../../../../editor/assets/ui_icons/reset.png").as_slice(),
        ),
        (
            "game.viewport.icon.undo",
            include_bytes!("../../../../editor/assets/ui_icons/undo.png").as_slice(),
        ),
        (
            "game.viewport.icon.redo",
            include_bytes!("../../../../editor/assets/ui_icons/redo.png").as_slice(),
        ),
        (
            "game.viewport.icon.preview",
            include_bytes!("../../../../editor/assets/ui_icons/preview.png").as_slice(),
        ),
    ] {
        let _ = bridge.register_embedded_png(key, bytes);
    }
}

fn build_viewport_surface(
    palette: StudioUiPalette,
    viewport: &ViewportPanel,
    _lang: Language,
) -> UiSurface {
    let root = UiNode::new("game.viewport-toolbar.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(tool_icon_button(
            "game.viewport-toolbar.select",
            "app.electronics_tool_select",
            "game.viewport.select",
            viewport.select_mode,
            "game.viewport.icon.select",
        ))
        .with_child(tool_icon_button(
            "game.viewport-toolbar.move",
            "viewport.hud.move",
            "game.viewport.move",
            viewport.gizmo_mode() == GizmoMode::Translate,
            "game.viewport.icon.move",
        ))
        .with_child(tool_icon_button(
            "game.viewport-toolbar.rotate",
            "viewport.hud.rotate",
            "game.viewport.rotate",
            viewport.gizmo_mode() == GizmoMode::Rotate,
            "game.viewport.icon.rotate",
        ))
        .with_child(tool_icon_button(
            "game.viewport-toolbar.scale",
            "viewport.hud.scale",
            "game.viewport.scale",
            viewport.gizmo_mode() == GizmoMode::Scale,
            "game.viewport.icon.scale",
        ))
        .with_child(separator("game.viewport-toolbar.separator", palette))
        .with_child(toggle_button(
            "game.viewport-toolbar.grid",
            "viewport.hud.toggle_grid",
            "game.viewport.grid",
            viewport.grid_visible,
            "game.viewport.icon.grid",
            palette,
        ))
        .with_child(toggle_button(
            "game.viewport-toolbar.focus",
            "viewport.hud.focus",
            "game.viewport.focus",
            viewport.focus_locked,
            "game.viewport.icon.focus",
            palette,
        ))
        .with_child(toggle_button(
            "game.viewport-toolbar.edit-mode",
            "viewport.hud.toggle_edit_mode",
            "game.viewport.edit-mode",
            viewport.edit_mode() == EditMode::Vertex,
            "game.viewport.icon.object-mode",
            palette,
        ))
        .with_child(
            UiNode::new("game.viewport-toolbar.spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        );

    let mut surface = UiSurface::new("game-viewport-toolbar", palette, root);
    surface.style_sheet = viewport_style_sheet(palette);
    surface
}

fn build_viewport_top_overlay_surface(
    palette: StudioUiPalette,
    viewport: &ViewportPanel,
    _lang: Language,
) -> UiSurface {
    let root = UiNode::new("game.viewport-overlay.root", UiNodeKind::Root)
        .with_class("game-viewport-overlay-root")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(4.0, 4.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(mode_segmented(viewport.mode))
        .with_child(render_style_button(viewport.render_style));

    let mut surface = UiSurface::new("game-viewport-top-overlay", palette, root);
    surface.style_sheet = viewport_style_sheet(palette);
    surface
}

fn build_viewport_bottom_overlay_surface(
    palette: StudioUiPalette,
    viewport: &ViewportPanel,
) -> UiSurface {
    let root = UiNode::new("game.viewport-bottom-overlay.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::End,
            padding: UiSpacing::xy(4.0, 4.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(
            UiNode::new("game.viewport-bottom-overlay.actions", UiNodeKind::Toolbar)
                .with_class("game-viewport-bottom-actions")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 2.0,
                    padding: UiSpacing::same(2.0),
                    ..UiLayout::fixed(148.0, 28.0)
                })
                .with_child(bottom_action_button(
                    "game.viewport-bottom.focus",
                    "viewport.hud.focus",
                    "game.viewport.focus",
                    "game.viewport.icon.focus",
                    viewport.focus_locked,
                ))
                .with_child(bottom_action_button(
                    "game.viewport-bottom.grid",
                    "viewport.hud.toggle_grid",
                    "game.viewport.grid",
                    "game.viewport.icon.grid",
                    viewport.grid_visible,
                ))
                .with_child(bottom_action_button(
                    "game.viewport-bottom.reset",
                    "viewport.hud.reset_iso",
                    "game.viewport.reset",
                    "game.viewport.icon.reset",
                    false,
                ))
                .with_child(separator("game.viewport-bottom.history-separator", palette))
                .with_child(bottom_action_button(
                    "game.viewport-bottom.undo",
                    "app.undo_menu",
                    "game.viewport.undo",
                    "game.viewport.icon.undo",
                    false,
                ))
                .with_child(bottom_action_button(
                    "game.viewport-bottom.redo",
                    "app.redo_menu",
                    "game.viewport.redo",
                    "game.viewport.icon.redo",
                    false,
                )),
        );

    let mut surface = UiSurface::new("game-viewport-bottom-overlay", palette, root);
    surface.style_sheet = viewport_style_sheet(palette);
    surface
}

fn mode_segmented(mode: ViewportMode) -> UiNode {
    UiNode::new("game.viewport-toolbar.mode", UiNodeKind::Toolbar)
        .with_class("game-viewport-mode")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 0.0,
            ..UiLayout::fixed(64.0, 28.0)
        })
        .with_child(mode_segmented_button(
            "game.viewport-toolbar.2d",
            "viewport.hud.2d_view.label",
            "viewport.hud.2d_view",
            "game.viewport.2d",
            mode == ViewportMode::View2D,
            true,
        ))
        .with_child(mode_segmented_button(
            "game.viewport-toolbar.3d",
            "viewport.hud.3d_view.label",
            "viewport.hud.3d_view",
            "game.viewport.3d",
            mode == ViewportMode::View3D,
            false,
        ))
}

fn mode_segmented_button(
    id: &str,
    label_key: &str,
    tooltip_key: &str,
    command: &str,
    active: bool,
    left: bool,
) -> UiNode {
    let class = if active {
        if left {
            "game-viewport-segment-left-active"
        } else {
            "game-viewport-segment-right-active"
        }
    } else if left {
        "game-viewport-segment-left"
    } else {
        "game-viewport-segment-right"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .with_text_style(UiTextStyle::button(if active {
            [18, 18, 20, 255]
        } else {
            [151, 159, 170, 255]
        }))
        .with_layout(UiLayout::fixed(32.0, 28.0))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn render_style_button(style: RenderStyle) -> UiNode {
    let (label_key, command) = match style {
        RenderStyle::Solid => ("viewport.hud.shaded_menu", "game.viewport.render-wireframe"),
        RenderStyle::Wireframe => (
            "settings.viewport_render_mode.wireframe",
            "game.viewport.render-preview",
        ),
        RenderStyle::Preview => (
            "settings.viewport_render_mode.preview",
            "game.viewport.render-solid",
        ),
    };
    UiNode::new("game.viewport-overlay.render-style", UiNodeKind::Button)
        .with_class("game-viewport-render-style")
        .with_text_key(label_key)
        .with_tooltip_key("viewport.hud.render_style")
        .with_accessibility_label_key("viewport.hud.render_style")
        .with_text_style(UiTextStyle::button([224, 228, 235, 235]))
        .with_layout(UiLayout::fixed(80.0, 28.0))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn tool_icon_button(
    id: &str,
    tooltip_key: &str,
    command: &str,
    active: bool,
    icon_key: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "game-viewport-tool-active"
        } else {
            "game-viewport-tool"
        })
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            padding: UiSpacing::same(4.0),
            min_size: [34.0, 30.0],
            max_size: [34.0, 30.0],
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: Some(if active {
                        [255, 255, 255, 255]
                    } else {
                        [236, 239, 244, 214]
                    }),
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
}

fn bottom_action_button(
    id: &str,
    tooltip_key: &str,
    command: &str,
    icon_key: &str,
    active: bool,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "game-viewport-bottom-action-active"
        } else {
            "game-viewport-bottom-action"
        })
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .with_layout(UiLayout {
            basis: [26.0, 24.0],
            min_size: [26.0, 24.0],
            max_size: [26.0, 24.0],
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: Some(if active {
                        [255, 255, 255, 255]
                    } else {
                        [236, 239, 244, 220]
                    }),
                },
            )
            .with_layout(UiLayout::fixed(15.0, 15.0)),
        )
}

fn toggle_button(
    id: &str,
    label_key: &str,
    command: &str,
    value: bool,
    icon_key: &str,
    _palette: StudioUiPalette,
) -> UiNode {
    UiNode::toggle(id, UiToggle::new(command, value))
        .with_class(if value {
            "game-viewport-toggle-active"
        } else {
            "game-viewport-toggle"
        })
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .with_layout(UiLayout {
            basis: [28.0, 28.0],
            min_size: [28.0, 28.0],
            max_size: [28.0, 28.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: Some(if value {
                        [255, 255, 255, 255]
                    } else {
                        [236, 239, 244, 214]
                    }),
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
}

fn separator(id: &str, palette: StudioUiPalette) -> UiNode {
    UiNode::new(id, UiNodeKind::Separator)
        .with_layout(UiLayout::fixed(1.0, 18.0))
        .with_style(raf_ui::UiStyle {
            fill: palette.tokens().border,
            border: palette.tokens().border,
            text: palette.tokens().border,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn parse_command(name: &str) -> Option<GameViewportSurfaceAction> {
    match name {
        "game.viewport.move" => Some(GameViewportSurfaceAction::SetGizmo(GizmoMode::Translate)),
        "game.viewport.rotate" => Some(GameViewportSurfaceAction::SetGizmo(GizmoMode::Rotate)),
        "game.viewport.scale" => Some(GameViewportSurfaceAction::SetGizmo(GizmoMode::Scale)),
        "game.viewport.select" => Some(GameViewportSurfaceAction::ToggleSelect),
        "game.viewport.2d" => Some(GameViewportSurfaceAction::SetMode(ViewportMode::View2D)),
        "game.viewport.3d" => Some(GameViewportSurfaceAction::SetMode(ViewportMode::View3D)),
        "game.viewport.grid" => Some(GameViewportSurfaceAction::ToggleGrid),
        "game.viewport.labels" => Some(GameViewportSurfaceAction::ToggleLabels),
        "game.viewport.focus" => Some(GameViewportSurfaceAction::ToggleFocusLock),
        "game.viewport.edit-mode" => Some(GameViewportSurfaceAction::ToggleEditMode),
        "game.viewport.render-solid" => Some(GameViewportSurfaceAction::SetRenderStyle(
            RenderStyle::Solid,
        )),
        "game.viewport.render-wireframe" => Some(GameViewportSurfaceAction::SetRenderStyle(
            RenderStyle::Wireframe,
        )),
        "game.viewport.render-preview" => Some(GameViewportSurfaceAction::SetRenderStyle(
            RenderStyle::Preview,
        )),
        "game.viewport.reset" => Some(GameViewportSurfaceAction::ResetView),
        "game.viewport.undo" => Some(GameViewportSurfaceAction::Undo),
        "game.viewport.redo" => Some(GameViewportSurfaceAction::Redo),
        _ => None,
    }
}

fn viewport_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Root),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-tool".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-tool".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-tool".to_string()),
                UiStylePatch {
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-tool-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-toggle".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-toggle".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [30, 34, 40, 255],
                        StudioUiPalette::PaperLight => [220, 222, 228, 255],
                    }),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-toggle-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-mode".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [18, 22, 28, 255],
                        StudioUiPalette::PaperLight => [240, 242, 246, 255],
                    }),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-left".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-left".to_string()),
                UiStylePatch {
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-right".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-right".to_string()),
                UiStylePatch {
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-left-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-right-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(6.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-render-style".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-render-style".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [18, 22, 28, 245],
                        StudioUiPalette::PaperLight => [240, 242, 246, 245],
                    }),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-render-style-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-tool-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-toggle-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-mode".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-left-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-segment-right-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-render-style".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-render-style".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-bottom-actions".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-bottom-action".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-bottom-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-viewport-bottom-action-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}
