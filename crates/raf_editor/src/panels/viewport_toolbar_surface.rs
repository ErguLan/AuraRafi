//! Declarative RafUI chrome for the Game viewport.
//!
//! The viewport renderer remains owned by ApiGraphicBasic. This surface only
//! exposes the existing editor tools as a bounded, retained overlay.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiRect, UiSpacing, UiStyle, UiTextStyle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportToolbarAction {
    Select,
    Move,
    Rotate,
    Scale,
    Focus,
    Solid,
    Wireframe,
    Preview,
    ToggleGrid,
    ToggleLabels,
    View2d,
    View3d,
    ResetView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportToolbarState {
    pub select_mode: bool,
    pub tool: ViewportTool,
    pub render_style: ViewportRenderStyle,
    pub grid_visible: bool,
    pub labels_visible: bool,
    pub view_mode: ViewportViewMode,
    pub view_menu_open: bool,
    pub compact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportTool {
    Select,
    Move,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportRenderStyle {
    Solid,
    Wireframe,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportViewMode {
    View2d,
    View3d,
}

const TOP_BAR_HEIGHT: f32 = 46.0;
const RAIL_TOP: f32 = 54.0;
const RAIL_WIDTH: f32 = 42.0;
const RAIL_HEIGHT: f32 = 226.0;

pub fn build_viewport_toolbar_surface(
    palette: StudioUiPalette,
    state: ViewportToolbarState,
) -> UiSurface {
    let root = UiNode::new("viewport.toolbar.root", UiNodeKind::Panel)
        .with_class("viewport-toolbar-root")
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle::transparent())
        .with_child(top_toolbar(palette, state))
        .with_child(tool_rail(palette, state));

    UiSurface::new("editor.viewport.toolbar", palette, root)
}

fn top_toolbar(palette: StudioUiPalette, state: ViewportToolbarState) -> UiNode {
    let tokens = palette.tokens();
    let width = if state.compact { 230.0 } else { 396.0 };
    let lit_width = if state.compact { 34.0 } else { 64.0 };
    let labels_width = if state.compact { 34.0 } else { 68.0 };
    let mut row = UiNode::new("viewport.toolbar.top", UiNodeKind::Toolbar)
        .with_class("viewport-toolbar-top")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, width, TOP_BAR_HEIGHT))
        })
        .with_style(UiStyle {
            fill: [
                tokens.surface_raised[0],
                tokens.surface_raised[1],
                tokens.surface_raised[2],
                184,
            ],
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 0.78,
        });
    row = row
        .with_child(view_dropdown(
            palette,
            state,
            if state.compact { 42.0 } else { 112.0 },
        ))
        .with_child(separator(
            palette,
            "viewport.toolbar.top.separator-view",
            4.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.solid",
            "app.viewport_lit_short",
            UiIconId::Shaded,
            "viewport.render.solid",
            matches!(state.render_style, ViewportRenderStyle::Solid),
            false,
            lit_width,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.wireframe",
            "app.viewport_wireframe",
            UiIconId::Wireframe,
            "viewport.render.wireframe",
            matches!(state.render_style, ViewportRenderStyle::Wireframe),
            false,
            34.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.preview",
            "app.viewport_preview",
            UiIconId::View3d,
            "viewport.render.preview",
            matches!(state.render_style, ViewportRenderStyle::Preview),
            false,
            34.0,
        ))
        .with_child(separator(
            palette,
            "viewport.toolbar.top.separator-style",
            4.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.grid",
            "app.viewport_grid",
            UiIconId::Grid,
            "viewport.toggle-grid",
            state.grid_visible,
            false,
            34.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.labels",
            if state.labels_visible {
                "app.viewport_show"
            } else {
                "app.viewport_show"
            },
            if state.labels_visible {
                UiIconId::Eye
            } else {
                UiIconId::EyeOff
            },
            "viewport.toggle-labels",
            state.labels_visible,
            false,
            labels_width,
        ));
    row
}

fn view_dropdown(palette: StudioUiPalette, state: ViewportToolbarState, width: f32) -> UiNode {
    let tokens = palette.tokens();
    let menu_width = if state.compact { 126.0 } else { 132.0 };
    let selected_icon = match state.view_mode {
        ViewportViewMode::View2d => UiIconId::View2d,
        ViewportViewMode::View3d => UiIconId::View3d,
    };
    let trigger_fill = if state.view_menu_open {
        [116, 67, 24, 154]
    } else {
        [
            tokens.surface_alt[0],
            tokens.surface_alt[1],
            tokens.surface_alt[2],
            176,
        ]
    };
    let trigger_border = if state.view_menu_open {
        tokens.accent
    } else {
        tokens.border
    };
    let mut trigger = UiNode::new("viewport.toolbar.view-mode.trigger", UiNodeKind::Button)
        .with_class(if state.view_menu_open {
            "viewport-toolbar-button-active"
        } else {
            "viewport-toolbar-button"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: if state.compact {
                UiJustify::Center
            } else {
                UiJustify::Start
            },
            gap: 5.0,
            padding: UiSpacing::xy(6.0, 4.0),
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, width, 30.0))
        })
        .with_style(UiStyle {
            fill: trigger_fill,
            border: trigger_border,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_icon(
            UiIcon::new(selected_icon)
                .with_size(UiIconSize::Toolbar)
                .with_tint(if state.view_menu_open {
                    [255, 247, 232, 255]
                } else {
                    tokens.text_muted
                }),
        )
        .with_tooltip_key("app.viewport_view")
        .with_accessibility_label_key("app.viewport_view")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "viewport.dropdown.view.toggle",
        ));
    if !state.compact {
        trigger = trigger.with_child(
            UiNode::new("viewport.toolbar.view-mode.label", UiNodeKind::Label)
                .with_text_key("app.viewport_view")
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        );
    }
    trigger = trigger.with_child(
        UiNode::new("viewport.toolbar.view-mode.chevron", UiNodeKind::Label)
            .with_icon(
                UiIcon::new(if state.view_menu_open {
                    UiIconId::ChevronDown
                } else {
                    UiIconId::ChevronRight
                })
                .with_size(UiIconSize::Small)
                .with_tint(tokens.text_muted),
            )
            .with_layout(UiLayout::fixed(16.0, 18.0)),
    );

    let mut group = UiNode::new("viewport.toolbar.view-mode", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_style(UiStyle::transparent())
        .with_child(trigger);
    if state.view_menu_open {
        let options = [
            (
                "app.viewport_2d_short",
                "viewport.view-2d",
                matches!(state.view_mode, ViewportViewMode::View2d),
            ),
            (
                "app.viewport_3d_short",
                "viewport.view-3d",
                matches!(state.view_mode, ViewportViewMode::View3d),
            ),
        ];
        let mut menu = UiNode::new("viewport.toolbar.view-mode.menu", UiNodeKind::Menu)
            .with_class("viewport-toolbar-menu")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 2.0,
                padding: UiSpacing::same(3.0),
                ..UiLayout::absolute(UiRect::new(0.0, 32.0, menu_width, 63.0)).with_z_index(120)
            })
            .with_style(UiStyle {
                fill: [
                    tokens.surface_raised[0],
                    tokens.surface_raised[1],
                    tokens.surface_raised[2],
                    244,
                ],
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 4.0,
                opacity: 1.0,
            });
        for (index, (label_key, command, selected)) in options.into_iter().enumerate() {
            menu = menu.with_child(
                raf_ui::components::dropdown_option(
                    format!("viewport.toolbar.view-mode.option.{index}"),
                    label_key,
                    command,
                    selected,
                    palette,
                )
                .with_style(UiStyle {
                    fill: if selected {
                        [116, 67, 24, 154]
                    } else {
                        [
                            tokens.surface_alt[0],
                            tokens.surface_alt[1],
                            tokens.surface_alt[2],
                            190,
                        ]
                    },
                    border: if selected {
                        tokens.accent
                    } else {
                        tokens.border
                    },
                    text: if selected {
                        tokens.text
                    } else {
                        tokens.text_muted
                    },
                    border_width: 1.0,
                    radius: 2.0,
                    opacity: 1.0,
                }),
            );
        }
        group = group.with_child(menu);
    }
    group
}

fn tool_rail(palette: StudioUiPalette, state: ViewportToolbarState) -> UiNode {
    let mut rail = UiNode::new("viewport.toolbar.rail", UiNodeKind::Toolbar)
        .with_class("viewport-toolbar-rail")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(4.0, 4.0),
            ..UiLayout::absolute(UiRect::new(0.0, RAIL_TOP, RAIL_WIDTH, RAIL_HEIGHT))
        })
        .with_style(UiStyle {
            fill: [
                palette.tokens().surface_raised[0],
                palette.tokens().surface_raised[1],
                palette.tokens().surface_raised[2],
                184,
            ],
            border: palette.tokens().border,
            text: palette.tokens().text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 0.78,
        });
    rail = rail
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.select",
            "app.viewport_select",
            UiIconId::Select,
            "viewport.select",
            matches!(state.tool, ViewportTool::Select) || state.select_mode,
            true,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.move",
            "app.viewport_move",
            UiIconId::Move,
            "viewport.move",
            matches!(state.tool, ViewportTool::Move) && !state.select_mode,
            true,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.rotate",
            "app.viewport_rotate",
            UiIconId::Rotate,
            "viewport.rotate",
            matches!(state.tool, ViewportTool::Rotate) && !state.select_mode,
            true,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.scale",
            "app.viewport_scale",
            UiIconId::Scale,
            "viewport.scale",
            matches!(state.tool, ViewportTool::Scale) && !state.select_mode,
            true,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(separator(
            palette,
            "viewport.toolbar.rail.separator",
            RAIL_WIDTH - 8.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.focus-rail",
            "app.focus_entity",
            UiIconId::Focus,
            "viewport.focus",
            false,
            false,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.reset-rail",
            "app.viewport_reset_view",
            UiIconId::Undo,
            "viewport.reset-view",
            false,
            false,
            RAIL_WIDTH - 8.0,
        ));
    rail
}

fn toolbar_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    icon: UiIconId,
    command: &str,
    active: bool,
    emphasized: bool,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let fill = if active && emphasized {
        [116, 67, 24, 154]
    } else {
        [
            tokens.surface_alt[0],
            tokens.surface_alt[1],
            tokens.surface_alt[2],
            176,
        ]
    };
    let border = if active && emphasized {
        tokens.accent
    } else {
        tokens.border
    };
    let icon_tint = if active && emphasized {
        [255, 247, 232, 255]
    } else if active {
        tokens.text
    } else {
        tokens.text_muted
    };
    let mut button = UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "viewport-toolbar-button-active"
        } else {
            "viewport-toolbar-button"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 4.0),
            ..UiLayout::fixed(width, 30.0).with_text_safe_area(width > 40.0)
        })
        .with_style(UiStyle {
            fill,
            border,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Toolbar)
                .with_tint(icon_tint),
        )
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
    if width > 40.0 {
        button = button
            .with_text_key(label_key)
            .with_text_style(UiTextStyle::button(if active {
                tokens.text
            } else {
                tokens.text_muted
            }));
    }
    button
}

fn separator(palette: StudioUiPalette, id: &str, width: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Separator)
        .with_layout(UiLayout::fixed(width, 1.0))
        .with_style(UiStyle {
            fill: palette.tokens().border,
            border: palette.tokens().border,
            text: palette.tokens().border,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

pub fn parse_viewport_toolbar_action(action: &UiAction) -> Option<ViewportToolbarAction> {
    let UiAction::Command { name } = action else {
        return None;
    };
    match name.as_str() {
        "viewport.select" => Some(ViewportToolbarAction::Select),
        "viewport.move" => Some(ViewportToolbarAction::Move),
        "viewport.rotate" => Some(ViewportToolbarAction::Rotate),
        "viewport.scale" => Some(ViewportToolbarAction::Scale),
        "viewport.focus" => Some(ViewportToolbarAction::Focus),
        "viewport.render.solid" => Some(ViewportToolbarAction::Solid),
        "viewport.render.wireframe" => Some(ViewportToolbarAction::Wireframe),
        "viewport.render.preview" => Some(ViewportToolbarAction::Preview),
        "viewport.toggle-grid" => Some(ViewportToolbarAction::ToggleGrid),
        "viewport.toggle-labels" => Some(ViewportToolbarAction::ToggleLabels),
        "viewport.view-2d" => Some(ViewportToolbarAction::View2d),
        "viewport.view-3d" => Some(ViewportToolbarAction::View3d),
        "viewport.reset-view" => Some(ViewportToolbarAction::ResetView),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    fn state(compact: bool) -> ViewportToolbarState {
        ViewportToolbarState {
            select_mode: true,
            tool: ViewportTool::Select,
            render_style: ViewportRenderStyle::Solid,
            grid_visible: true,
            labels_visible: true,
            view_mode: ViewportViewMode::View3d,
            view_menu_open: false,
            compact,
        }
    }

    #[test]
    fn toolbar_has_separate_top_bar_and_tool_rail() {
        let surface = build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, state(false));
        let top = find_node(&surface.root, "viewport.toolbar.top").expect("top bar");
        let rail = find_node(&surface.root, "viewport.toolbar.rail").expect("tool rail");

        assert_eq!(top.layout.rect, Some(UiRect::new(0.0, 0.0, 396.0, 46.0)));
        assert_eq!(rail.layout.rect, Some(UiRect::new(0.0, 54.0, 42.0, 226.0)));
        assert!(find_node(&surface.root, "viewport.toolbar.select").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.view-mode").is_some());
    }

    #[test]
    fn compact_toolbar_keeps_actions_available_as_icons() {
        let surface = build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, state(true));
        let perspective = find_node(&surface.root, "viewport.toolbar.view-mode.trigger")
            .expect("view mode trigger");
        let lit = find_node(&surface.root, "viewport.toolbar.solid").expect("render mode");
        let show = find_node(&surface.root, "viewport.toolbar.labels").expect("show toggle");

        assert!(perspective.text_key.is_none());
        assert!(lit.text_key.is_none());
        assert!(show.text_key.is_none());
        assert!(perspective.tooltip_key.is_some());
        assert!(lit.tooltip_key.is_some());
        assert!(show.tooltip_key.is_some());
    }

    #[test]
    fn view_dropdown_exposes_only_the_two_view_modes() {
        let mut toolbar_state = state(false);
        toolbar_state.view_menu_open = true;
        let surface =
            build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, toolbar_state);

        assert!(find_node(&surface.root, "viewport.toolbar.view-mode.menu").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.view-mode.option.0").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.view-mode.option.1").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.view-2d").is_none());
        assert!(find_node(&surface.root, "viewport.toolbar.view-3d").is_none());
    }
}
