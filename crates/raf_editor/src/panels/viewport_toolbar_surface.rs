//! Declarative RafUI chrome for the Game viewport.
//!
//! The viewport renderer remains owned by ApiGraphicBasic. This surface only
//! exposes the existing editor tools as a bounded, retained overlay.

use super::primitive_create::{primitive_key, primitive_slug, CREATEABLE_PRIMITIVES};
use raf_core::project::BuildingStyle;
use raf_core::scene::Primitive;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify,
    UiLayout, UiNode, UiNodeKind, UiRect, UiSelect, UiSelectOption, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurfaceMaterial,
    UiTextOverflow, UiTextStyle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportToolbarAction {
    Select,
    Move,
    Rotate,
    Scale,
    Focus,
    Solid,
    TogglePolygons,
    ToggleGrid,
    ToggleLabels,
    View2d,
    View3d,
    ResetView,
    CreatePrimitive(Primitive),
    SetBuildingStyle(BuildingStyle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportToolbarState {
    pub select_mode: bool,
    pub tool: ViewportTool,
    pub render_style: ViewportRenderStyle,
    pub polygons_visible: bool,
    pub grid_visible: bool,
    pub labels_visible: bool,
    pub view_mode: ViewportViewMode,
    pub view_menu_open: bool,
    pub shading_menu_open: bool,
    pub primitive_menu_open: bool,
    pub building_style: BuildingStyle,
    pub building_menu_open: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportViewMode {
    View2d,
    View3d,
}

const TOP_BAR_HEIGHT: f32 = 46.0;
const RAIL_TOP: f32 = 54.0;
const RAIL_WIDTH: f32 = 42.0;
const DROPDOWN_MIN_WIDTH: f32 = 152.0;
const DROPDOWN_OPTION_HEIGHT: f32 = 27.0;
const DROPDOWN_GAP: f32 = 4.0;
const DROPDOWN_PADDING: f32 = 8.0;
/// Eight tool buttons, two separators and padding: the building-style dropdown
/// sits below Create primitive at the bottom of the rail.
const RAIL_HEIGHT: f32 = 308.0;

pub fn build_viewport_toolbar_surface(
    palette: StudioUiPalette,
    state: ViewportToolbarState,
) -> UiSurface {
    let mut root = UiNode::new("viewport.toolbar.root", UiNodeKind::Panel)
        .with_class("viewport-toolbar-root")
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle::transparent())
        .with_child(top_toolbar(palette, state))
        .with_child(tool_rail(palette, state));
    if state.primitive_menu_open {
        root = root.with_child(primitive_menu(palette));
    }

    let mut surface = UiSurface::new("editor.viewport.toolbar", palette, root);
    surface.style_sheet = viewport_toolbar_style_sheet(palette);
    surface
}

fn top_toolbar(palette: StudioUiPalette, state: ViewportToolbarState) -> UiNode {
    let tokens = palette.tokens();
    let width = if state.compact { 230.0 } else { 396.0 };
    let shading_width = if state.compact { 42.0 } else { 96.0 };
    let labels_width = if state.compact { 34.0 } else { 68.0 };
    let mut row = UiNode::new("viewport.toolbar.top", UiNodeKind::Toolbar)
        .with_class("viewport-toolbar-top")
        .with_material(UiSurfaceMaterial::TranslucentChrome)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, width, TOP_BAR_HEIGHT))
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
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
        .with_child(shading_dropdown(palette, state, shading_width))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.polygons",
            "app.viewport_polygons",
            UiIconId::Wireframe,
            "viewport.toggle-polygons",
            state.polygons_visible,
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
                "app.viewport_hide_labels"
            } else {
                "app.viewport_show_labels"
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

fn toolbar_select_trigger(
    palette: StudioUiPalette,
    id: &str,
    value_key: &str,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    open: bool,
    selected_icon: UiIconId,
    label_key: &str,
    width: f32,
    compact: bool,
    popup_id: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let mut select = UiSelect::new(value_key, options, selected_index).with_popup_id(popup_id);
    select.open = open;
    let mut trigger = UiNode::select(id, select)
        .with_class(if open {
            "viewport-toolbar-button-active"
        } else {
            "viewport-toolbar-button"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: if compact {
                UiJustify::Center
            } else {
                UiJustify::Start
            },
            gap: 5.0,
            padding: UiSpacing::xy(6.0, 4.0),
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, width, 30.0))
        })
        .with_style(UiStyle {
            fill: if open {
                [116, 67, 24, 154]
            } else {
                [
                    tokens.surface_alt[0],
                    tokens.surface_alt[1],
                    tokens.surface_alt[2],
                    176,
                ]
            },
            border: if open { tokens.accent } else { tokens.border },
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_icon(
            UiIcon::new(selected_icon)
                .with_size(UiIconSize::Toolbar)
                .with_tint(if open {
                    [255, 247, 232, 255]
                } else {
                    tokens.text_muted
                }),
        )
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Combobox)
        .with_accessibility_expanded(open)
        .focusable()
        .with_text_overflow(UiTextOverflow::Ellipsis);
    if !compact {
        trigger = trigger.with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        );
    }
    trigger.with_child(
        UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
            .with_icon(
                UiIcon::new(if open {
                    UiIconId::ChevronDown
                } else {
                    UiIconId::ChevronRight
                })
                .with_size(UiIconSize::Small)
                .with_tint(tokens.text_muted),
            )
            .with_layout(UiLayout::fixed(16.0, 18.0)),
    )
}

fn select_popup_option(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: &str,
    value_key: &str,
    value: &str,
    index: usize,
    selected: bool,
    trigger_id: &str,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if selected {
            "dropdown-option-selected"
        } else {
            "dropdown-option"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Start,
            padding: UiSpacing::xy(8.0, 0.0),
            // Options deliberately sit one layer above their popup surface.
            // This keeps the label and selected fill visible in every
            // compositor backend, including nested/anchored menus.
            ..UiLayout::fixed(0.0, DROPDOWN_OPTION_HEIGHT)
                .with_width_mode(raf_ui::UiSizeMode::Fill)
                .with_z_index(1)
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(if selected {
            tokens.text
        } else {
            tokens.text_muted
        }))
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
        })
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_accessibility_role(UiAccessibilityRole::Option)
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelect {
                key: value_key.to_string(),
                value: value.to_string(),
                index,
            },
        })
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelectOpen {
                id: trigger_id.to_string(),
                open: false,
            },
        })
}

fn view_dropdown(palette: StudioUiPalette, state: ViewportToolbarState, width: f32) -> UiNode {
    let options = vec![
        UiSelectOption::new("2d", "app.viewport_2d_short"),
        UiSelectOption::new("3d", "app.viewport_3d_short"),
    ];
    let selected_index = match state.view_mode {
        ViewportViewMode::View2d => 0,
        ViewportViewMode::View3d => 1,
    };
    let (selected_icon, label_key) = match state.view_mode {
        ViewportViewMode::View2d => (UiIconId::View2d, "app.viewport_2d_short"),
        ViewportViewMode::View3d => (UiIconId::View3d, "app.viewport_3d_short"),
    };
    let trigger_id = "viewport.toolbar.view-mode.trigger";
    let popup_id = "viewport.toolbar.view-mode.menu";
    let trigger = toolbar_select_trigger(
        palette,
        trigger_id,
        "viewport.view_mode",
        options.clone(),
        selected_index,
        state.view_menu_open,
        selected_icon,
        label_key,
        width,
        state.compact,
        popup_id,
    );
    let mut group = UiNode::new("viewport.toolbar.view-mode", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_style(UiStyle::transparent())
        .with_child(trigger);
    if state.view_menu_open {
        group = group.with_child(toolbar_dropdown_menu(
            palette,
            popup_id,
            "viewport-toolbar-menu",
            UiRect::new(0.0, 32.0, 0.0, 0.0),
            options.as_slice(),
            "viewport.view_mode",
            selected_index,
            trigger_id,
            "viewport.toolbar.view-mode.option",
            None,
        ));
    }
    group
}

fn shading_dropdown(palette: StudioUiPalette, state: ViewportToolbarState, width: f32) -> UiNode {
    let options = vec![UiSelectOption::new("solid", "app.viewport_lit_short")];
    let selected_index = 0;
    let label_key = "app.viewport_lit_short";
    let icon = UiIconId::Shaded;
    let trigger_id = "viewport.toolbar.shading.trigger";
    let popup_id = "viewport.toolbar.shading.menu";
    let trigger = toolbar_select_trigger(
        palette,
        trigger_id,
        "viewport.render_style",
        options.clone(),
        selected_index,
        state.shading_menu_open,
        icon,
        label_key,
        width,
        state.compact,
        popup_id,
    );
    let mut group = UiNode::new("viewport.toolbar.shading", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_style(UiStyle::transparent())
        .with_child(trigger);
    if state.shading_menu_open {
        group = group.with_child(toolbar_dropdown_menu(
            palette,
            popup_id,
            "viewport-toolbar-menu",
            UiRect::new(0.0, 32.0, 0.0, 0.0),
            options.as_slice(),
            "viewport.render_style",
            selected_index,
            trigger_id,
            "viewport.toolbar.shading.option",
            None,
        ));
    }
    group
}

fn tool_rail(palette: StudioUiPalette, state: ViewportToolbarState) -> UiNode {
    let mut rail = UiNode::new("viewport.toolbar.rail", UiNodeKind::Toolbar)
        .with_class("viewport-toolbar-rail")
        .with_material(UiSurfaceMaterial::TranslucentChrome)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(4.0, 4.0),
            ..UiLayout::absolute(UiRect::new(0.0, RAIL_TOP, RAIL_WIDTH, RAIL_HEIGHT))
        })
        .with_style(UiStyle {
            fill: palette.tokens().surface_raised,
            border: palette.tokens().border,
            text: palette.tokens().text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
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
            UiIconId::Refresh,
            "viewport.reset-view",
            false,
            false,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(toolbar_button(
            palette,
            "viewport.toolbar.create-primitive",
            "app.create_primitive",
            UiIconId::Cube,
            "viewport.create-primitive.toggle",
            state.primitive_menu_open,
            false,
            RAIL_WIDTH - 8.0,
        ))
        .with_child(separator(
            palette,
            "viewport.toolbar.rail.separator-building",
            RAIL_WIDTH - 8.0,
        ))
        // Keep the mode control in the rail's final slot. Its popup is built
        // by the same owner group, so it follows this button if the rail is
        // resized or its spacing changes.
        .with_child(building_style_dropdown(palette, state));
    rail
}

fn primitive_menu(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    let mut menu = UiNode::new("viewport.toolbar.primitive-menu", UiNodeKind::Menu)
        .with_class("viewport-primitive-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(8.0),
            ..UiLayout::absolute(UiRect::new(
                RAIL_WIDTH + 8.0,
                RAIL_TOP + 112.0,
                188.0,
                178.0,
            ))
            .with_z_index(240)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 7.0,
            opacity: 1.0,
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "viewport.create-primitive.cancel",
        ))
        .with_child(
            UiNode::new("viewport.primitive-menu.title", UiNodeKind::Label)
                .with_text_key("app.create_primitive")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0).with_width_mode(raf_ui::UiSizeMode::Fill)),
        );
    for primitive in CREATEABLE_PRIMITIVES {
        menu = menu.with_child(viewport_primitive_option(palette, primitive));
    }
    menu
}

fn toolbar_dropdown_menu(
    palette: StudioUiPalette,
    popup_id: &str,
    class: &str,
    mut rect: UiRect,
    options: &[UiSelectOption],
    value_key: &str,
    selected_index: usize,
    trigger_id: &str,
    option_id_prefix: &str,
    title_key: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    let title_height = title_key.map_or(0.0, |_| 24.0);
    let option_height = options.len() as f32 * DROPDOWN_OPTION_HEIGHT;
    let child_count = options.len() + usize::from(title_key.is_some());
    let flow_gaps = child_count.saturating_sub(1) as f32 * DROPDOWN_GAP;
    let height = DROPDOWN_PADDING * 2.0 + title_height + option_height + flow_gaps;
    let width = rect.width.max(DROPDOWN_MIN_WIDTH);
    rect.width = width;
    rect.height = rect.height.max(height);
    let z_index = if title_key.is_some() { 240 } else { 140 };
    let mut menu = UiNode::new(popup_id, UiNodeKind::Menu)
        .with_class(class)
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: DROPDOWN_GAP,
            padding: UiSpacing::same(DROPDOWN_PADDING),
            ..UiLayout::absolute(rect).with_z_index(z_index)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Menu);
    if let Some(title_key) = title_key {
        menu = menu.with_child(
            UiNode::new(format!("{popup_id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(
                    UiLayout::fixed(0.0, title_height).with_width_mode(raf_ui::UiSizeMode::Fill),
                ),
        );
    }
    for (index, option) in options.iter().enumerate() {
        menu = menu.with_child(select_popup_option(
            palette,
            format!("{option_id_prefix}.{index}"),
            &option.label_key,
            value_key,
            &option.value,
            index,
            index == selected_index,
            trigger_id,
        ));
    }
    menu
}

/// Builds the building-style trigger and its popup as one retained subtree.
/// The menu offset is relative to this group, never to the viewport window.
fn building_style_dropdown(palette: StudioUiPalette, state: ViewportToolbarState) -> UiNode {
    let options = vec![
        UiSelectOption::new("free", "app.building_style_free"),
        UiSelectOption::new("professional", "app.building_style_professional"),
    ];
    let selected_index = usize::from(state.building_style == BuildingStyle::Organized);
    let trigger_id = "viewport.toolbar.building-style";
    let mut group = UiNode::new("viewport.toolbar.building-style.group", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(RAIL_WIDTH - 8.0, 30.0))
        .with_style(UiStyle::transparent())
        .with_child(toolbar_select_trigger(
            palette,
            trigger_id,
            "viewport.building_style",
            options.clone(),
            selected_index,
            state.building_menu_open,
            UiIconId::Grid,
            "app.building_style",
            RAIL_WIDTH - 8.0,
            true,
            "viewport.toolbar.building-menu",
        ));
    if state.building_menu_open {
        group = group.with_child(
            toolbar_dropdown_menu(
                palette,
                "viewport.toolbar.building-menu",
                "viewport-building-menu",
                UiRect::new(RAIL_WIDTH + 8.0, 0.0, 188.0, 0.0),
                options.as_slice(),
                "viewport.building_style",
                selected_index,
                trigger_id,
                "viewport.building-option",
                Some("app.building_style"),
            )
            .with_style(UiStyle {
                fill: palette.tokens().surface_raised,
                border: palette.tokens().border,
                text: palette.tokens().text,
                border_width: 1.0,
                radius: 7.0,
                opacity: 1.0,
            })
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "viewport.dropdown.building.cancel",
            )),
        );
    }
    group
}

fn viewport_primitive_option(palette: StudioUiPalette, primitive: Primitive) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("viewport.primitive-option.{}", primitive_slug(primitive)),
        UiNodeKind::Button,
    )
    .with_class("viewport-primitive-option")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 9.0,
        padding: UiSpacing::xy(9.0, 0.0),
        ..UiLayout::fixed(0.0, 29.0).with_width_mode(raf_ui::UiSizeMode::Fill)
    })
    .with_icon(
        UiIcon::new(match primitive {
            Primitive::Cube => UiIconId::Cube,
            Primitive::Sphere => UiIconId::Sphere,
            Primitive::Cylinder => UiIconId::Cylinder,
            Primitive::Plane => UiIconId::Plane,
            Primitive::Empty => UiIconId::Node,
        })
        .with_size(UiIconSize::Small)
        .with_tint(tokens.text_muted),
    )
    .with_text_key(primitive_key(primitive))
    .with_text_style(UiTextStyle::button(tokens.text))
    .with_accessibility_label_key(primitive_key(primitive))
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("viewport.create-primitive:{}", primitive_slug(primitive)),
    ))
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

fn viewport_toolbar_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-toolbar-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-toolbar-button-active".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-primitive-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-toolbar-menu".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
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
        "viewport.toggle-polygons" => Some(ViewportToolbarAction::TogglePolygons),
        "viewport.toggle-grid" => Some(ViewportToolbarAction::ToggleGrid),
        "viewport.toggle-labels" => Some(ViewportToolbarAction::ToggleLabels),
        "viewport.view-2d" => Some(ViewportToolbarAction::View2d),
        "viewport.view-3d" => Some(ViewportToolbarAction::View3d),
        "viewport.reset-view" => Some(ViewportToolbarAction::ResetView),
        "viewport.building.free" => {
            Some(ViewportToolbarAction::SetBuildingStyle(BuildingStyle::Free))
        }
        "viewport.building.professional" => Some(ViewportToolbarAction::SetBuildingStyle(
            BuildingStyle::Organized,
        )),
        _ if name.starts_with("viewport.create-primitive:") => name
            .strip_prefix("viewport.create-primitive:")
            .and_then(super::primitive_create::parse_primitive_slug)
            .map(ViewportToolbarAction::CreatePrimitive),
        _ => None,
    }
}

/// Parses the shared Select contract used by the compact viewport menus.
/// Command strings for the remaining viewport actions stay supported so
/// attached/native callers do not need to migrate in lockstep.
pub fn parse_viewport_toolbar_select_action(
    key: &str,
    value: &str,
) -> Option<ViewportToolbarAction> {
    match (key, value) {
        ("viewport.view_mode", "2d") => Some(ViewportToolbarAction::View2d),
        ("viewport.view_mode", "3d") => Some(ViewportToolbarAction::View3d),
        ("viewport.render_style", "solid") => Some(ViewportToolbarAction::Solid),
        ("viewport.building_style", "free") => {
            Some(ViewportToolbarAction::SetBuildingStyle(BuildingStyle::Free))
        }
        ("viewport.building_style", "professional") => Some(
            ViewportToolbarAction::SetBuildingStyle(BuildingStyle::Organized),
        ),
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

    fn child_index(node: &UiNode, id: &str) -> Option<usize> {
        node.children.iter().position(|child| child.id == id)
    }

    fn state(compact: bool) -> ViewportToolbarState {
        ViewportToolbarState {
            select_mode: true,
            tool: ViewportTool::Select,
            render_style: ViewportRenderStyle::Solid,
            polygons_visible: false,
            grid_visible: true,
            labels_visible: true,
            view_mode: ViewportViewMode::View3d,
            view_menu_open: false,
            shading_menu_open: false,
            primitive_menu_open: false,
            building_style: BuildingStyle::Free,
            building_menu_open: false,
            compact,
        }
    }

    #[test]
    fn toolbar_has_separate_top_bar_and_tool_rail() {
        let surface = build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, state(false));
        let top = find_node(&surface.root, "viewport.toolbar.top").expect("top bar");
        let rail = find_node(&surface.root, "viewport.toolbar.rail").expect("tool rail");

        assert_eq!(top.layout.rect, Some(UiRect::new(0.0, 0.0, 396.0, 46.0)));
        assert_eq!(rail.layout.rect, Some(UiRect::new(0.0, 54.0, 42.0, 308.0)));
        assert!(find_node(&surface.root, "viewport.toolbar.select").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.view-mode").is_some());
        assert_eq!(child_index(rail, "viewport.toolbar.select"), Some(0));
        assert_eq!(
            child_index(rail, "viewport.toolbar.building-style.group"),
            Some(rail.children.len() - 1),
            "building style belongs to the final rail slot"
        );
    }

    #[test]
    fn building_style_dropdown_lists_both_styles() {
        let mut toolbar_state = state(false);
        toolbar_state.building_menu_open = true;
        toolbar_state.building_style = BuildingStyle::Organized;
        let surface =
            build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, toolbar_state);

        assert!(find_node(&surface.root, "viewport.toolbar.building-menu").is_some());
        assert!(find_node(&surface.root, "viewport.building-option.0").is_some());
        assert!(find_node(&surface.root, "viewport.building-option.1").is_some());
        let menu =
            find_node(&surface.root, "viewport.toolbar.building-menu").expect("building menu");
        assert_eq!(menu.layout.position_mode, raf_ui::UiPositionMode::Absolute);
        assert_eq!(menu.layout.rect.map(|rect| rect.x), Some(RAIL_WIDTH + 8.0));
        assert!(
            find_node(&surface.root, "viewport.toolbar.building-style").is_some(),
            "trigger button stays in the rail"
        );
    }

    #[test]
    fn compact_toolbar_keeps_actions_available_as_icons() {
        let surface = build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, state(true));
        let perspective = find_node(&surface.root, "viewport.toolbar.view-mode.trigger")
            .expect("view mode trigger");
        let shading =
            find_node(&surface.root, "viewport.toolbar.shading.trigger").expect("shading dropdown");
        let show = find_node(&surface.root, "viewport.toolbar.labels").expect("show toggle");

        assert!(perspective.text_key.is_none());
        assert!(shading.text_key.is_none());
        assert!(show.text_key.is_none());
        assert!(perspective.tooltip_key.is_some());
        assert!(shading.tooltip_key.is_some());
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
        let menu = find_node(&surface.root, "viewport.toolbar.view-mode.menu").expect("view menu");
        assert_eq!(menu.layout.flow, UiFlow::Column);
        assert_eq!(menu.layout.rect.map(|rect| rect.height), Some(74.0));
    }

    #[test]
    fn shading_is_a_dropdown_and_polygon_edges_are_a_quick_toggle() {
        let mut toolbar_state = state(false);
        toolbar_state.shading_menu_open = true;
        let surface =
            build_viewport_toolbar_surface(StudioUiPalette::IndustrialDark, toolbar_state);

        assert!(find_node(&surface.root, "viewport.toolbar.shading.menu").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.polygons").is_some());
        assert!(find_node(&surface.root, "viewport.toolbar.solid").is_none());
        assert!(find_node(&surface.root, "viewport.toolbar.preview").is_none());
        let menu = find_node(&surface.root, "viewport.toolbar.shading.menu").expect("shading menu");
        assert_eq!(menu.layout.flow, UiFlow::Column);
        assert_eq!(menu.layout.rect.map(|rect| rect.height), Some(43.0));
        assert!(find_node(&surface.root, "viewport.toolbar.shading.option.0").is_some());
        assert_eq!(
            find_node(&surface.root, "viewport.toolbar.shading.option.0")
                .and_then(|node| node.text_key.as_deref()),
            Some("app.viewport_lit_short")
        );
        assert!(find_node(&surface.root, "viewport.toolbar.shading.option.1").is_none());
        assert!(find_node(&surface.root, "viewport.toolbar.shading.option.2").is_none());
        assert!(parse_viewport_toolbar_action(&UiAction::Command {
            name: "viewport.render.wireframe".to_string(),
        })
        .is_none());
        assert!(parse_viewport_toolbar_select_action("viewport.render_style", "preview").is_none());
    }
}
