//! Context menus of the retained Hierarchy and their viewport-bounded geometry.

use raf_core::scene::Primitive;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiIcon, UiIconId, UiIconSize};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiSizeMode, UiSpacing, UiStyle, UiSurfaceMaterial, UiTextStyle,
};

use super::super::primitive_create::{primitive_key, primitive_slug, CREATEABLE_PRIMITIVES};
use super::HIERARCHY_ROOT_PADDING;

pub(super) fn empty_context_menu(
    palette: StudioUiPalette,
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
    can_paste: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let menu_rect = empty_context_menu_rect(position, can_paste, surface_size);
    let authored_rect = raf_ui::UiRect::new(
        menu_rect.x - HIERARCHY_ROOT_PADDING,
        menu_rect.y - HIERARCHY_ROOT_PADDING,
        menu_rect.width,
        menu_rect.height,
    );
    let mut menu = UiNode::new("hierarchy.empty-context-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(authored_rect).with_z_index(220)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_child(menu_title(
            palette,
            "hierarchy.empty-context-menu.title",
            "app.scene_actions",
            UiIconId::Scene,
        ))
        .with_child(menu_item(
            palette,
            "hierarchy.empty-context-menu.create-primitive",
            "app.create_primitive",
            UiIconId::Cube,
            "hierarchy.empty.create-primitive-menu",
            true,
        ));
    menu = menu.with_child(menu_item(
        palette,
        "hierarchy.empty-context-menu.camera",
        "camera.create",
        UiIconId::Camera,
        "hierarchy.create-camera:root",
        false,
    ));
    menu = menu.with_child(menu_item(
        palette,
        "hierarchy.empty-context-menu.entity",
        "app.create_entity",
        UiIconId::Node,
        "hierarchy.create-entity:root",
        false,
    ));
    menu = menu.with_child(menu_item(
        palette,
        "hierarchy.empty-context-menu.folder",
        "app.add_folder",
        UiIconId::Folder,
        "hierarchy.create-folder:root",
        false,
    ));
    if can_paste {
        menu = menu.with_child(menu_separator("hierarchy.empty-context-menu.sep", palette));
        menu = menu.with_child(menu_item(
            palette,
            "hierarchy.empty-context-menu.paste",
            "app.paste_menu",
            UiIconId::Add,
            "hierarchy.paste:root",
            false,
        ));
    }
    menu
}

pub(super) fn empty_primitive_menu(
    palette: StudioUiPalette,
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
) -> UiNode {
    let tokens = palette.tokens();
    let rect = empty_primitive_menu_rect(position, surface_size);
    let authored_rect = raf_ui::UiRect::new(
        rect.x - HIERARCHY_ROOT_PADDING,
        rect.y - HIERARCHY_ROOT_PADDING,
        rect.width,
        rect.height,
    );
    let mut menu = UiNode::new("hierarchy.empty-primitive-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu hierarchy-primitive-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(authored_rect).with_z_index(230)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_child(menu_title(
            palette,
            "hierarchy.empty-primitive-menu.title",
            "app.create_primitive",
            UiIconId::Cube,
        ));
    for primitive in CREATEABLE_PRIMITIVES {
        menu = menu.with_child(primitive_menu_item(
            palette,
            primitive,
            format!(
                "hierarchy.create-primitive:root:{}",
                primitive_slug(primitive)
            ),
        ));
    }
    menu
}

fn menu_separator(id: &str, palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::xy(4.0, 2.0),
            ..UiLayout::fixed(0.0, 5.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.line"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill))
                .with_style(UiStyle {
                    fill: [tokens.border[0], tokens.border[1], tokens.border[2], 120],
                    border: [0, 0, 0, 0],
                    text: [0, 0, 0, 0],
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 0.7,
                }),
        )
}

fn menu_title(palette: StudioUiPalette, id: &str, text_key: &str, icon: UiIconId) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, CONTEXT_MENU_TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(tokens.accent),
        )
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::panel_title(tokens.text))
}

fn menu_item(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    icon: UiIconId,
    command: impl Into<String>,
    arrow: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let mut item = UiNode::new(id, UiNodeKind::Button)
        .with_class("hierarchy-menu-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, CONTEXT_MENU_BUTTON_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(text_key)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_accessibility_label_key(text_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
    if arrow {
        item = item.with_child(
            UiNode::new(format!("{id}.arrow"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::ChevronRight)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    }
    item
}

fn primitive_menu_item(
    palette: StudioUiPalette,
    primitive: Primitive,
    command: impl Into<String>,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("hierarchy.primitive-option.{}", primitive_slug(primitive)),
        UiNodeKind::Button,
    )
    .with_class("hierarchy-menu-button hierarchy-primitive-option")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 8.0,
        padding: UiSpacing::xy(8.0, 0.0),
        ..UiLayout::fixed(0.0, CONTEXT_MENU_BUTTON_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!(
                "hierarchy.primitive-option.{}.icon",
                primitive_slug(primitive)
            ),
            UiNodeKind::Label,
        )
        .with_icon(
            UiIcon::new(match primitive {
                Primitive::Cube | Primitive::Wedge => UiIconId::Cube,
                Primitive::Sphere | Primitive::Sphere2D | Primitive::Torus => UiIconId::Sphere,
                Primitive::Cylinder | Primitive::Prism => UiIconId::Cylinder,
                Primitive::Capsule => UiIconId::Capsule,
                Primitive::Plane => UiIconId::Plane,
                Primitive::Empty => UiIconId::Node,
            })
            .with_size(UiIconSize::Small)
            .with_tint(tokens.accent),
        )
        .with_layout(UiLayout::fixed(16.0, 16.0)),
    )
    .with_child(
        UiNode::new(
            format!(
                "hierarchy.primitive-option.{}.label",
                primitive_slug(primitive)
            ),
            UiNodeKind::Label,
        )
        .with_text_key(primitive_key(primitive))
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content().with_text_safe_area(true)
        }),
    )
    .with_accessibility_label_key(primitive_key(primitive))
    .focusable()
    .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn context_menu(
    palette: StudioUiPalette,
    target: raf_core::scene::SceneNodeId,
    is_folder: bool,
    label: Option<&str>,
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
    can_paste: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let id = target.0;
    let menu_rect = context_menu_rect(position, is_folder, surface_size);
    let authored_menu_rect = raf_ui::UiRect::new(
        menu_rect.x - HIERARCHY_ROOT_PADDING,
        menu_rect.y - HIERARCHY_ROOT_PADDING,
        menu_rect.width,
        menu_rect.height,
    );
    let title_icon = if is_folder {
        UiIconId::Folder
    } else {
        UiIconId::Node
    };
    let mut title_node = UiNode::new("hierarchy.context-menu.title", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(6.0, 0.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, CONTEXT_MENU_TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(
            UiIcon::new(title_icon)
                .with_size(UiIconSize::Small)
                .with_tint(tokens.accent),
        );
    title_node = match label {
        Some(label) => title_node.with_text_value(label.to_string()),
        None => title_node.with_text_key("app.hierarchy"),
    };
    title_node = title_node.with_text_style(UiTextStyle::panel_title(tokens.text));

    let mut menu = UiNode::new("hierarchy.context-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(authored_menu_rect).with_z_index(220)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_accessibility_label_key("app.hierarchy")
        .with_child(title_node);

    // Group 1: Edit actions
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.rename",
        "app.rename",
        Some(UiIconId::Select),
        format!("hierarchy.rename.begin:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.duplicate",
        "app.duplicate_menu",
        Some(UiIconId::Add),
        format!("hierarchy.duplicate:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.copy",
        "app.copy_menu",
        None,
        format!("hierarchy.copy:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.paste",
        "app.paste_menu",
        None,
        format!("hierarchy.paste:{id}"),
        false,
        !can_paste,
    ));

    // Separator 1
    menu = menu.with_child(menu_separator("hierarchy.context-menu.sep1", palette));

    // Group 2: View and Navigation
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.focus",
        "app.focus_entity",
        Some(UiIconId::Focus),
        format!("hierarchy.focus:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.expand",
        "app.expand_menu",
        Some(UiIconId::ChevronDown),
        format!("hierarchy.expand-recursive:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.collapse",
        "app.collapse_menu",
        Some(UiIconId::ChevronRight),
        format!("hierarchy.collapse-recursive:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.select-children",
        "app.select_children_menu",
        Some(UiIconId::Select),
        format!("hierarchy.select-children:{id}"),
        false,
        false,
    ));

    // Separator 2
    menu = menu.with_child(menu_separator("hierarchy.context-menu.sep2", palette));

    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.insertion",
        "hierarchy.set_insertion_parent",
        Some(UiIconId::Add),
        format!("hierarchy.insertion-parent:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.snap-floor",
        "hierarchy.snap_floor",
        Some(UiIconId::Magnet),
        format!("hierarchy.snap-floor:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.snap-selected",
        "hierarchy.snap_selected",
        Some(UiIconId::Magnet),
        format!("hierarchy.snap-selected:{id}"),
        false,
        false,
    ));
    // Group 3: Structure and Hierarchy
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.camera",
        "camera.create",
        Some(UiIconId::Camera),
        format!("hierarchy.create-camera:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.entity",
        "app.create_entity",
        Some(UiIconId::Entity),
        format!("hierarchy.create-entity:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.folder",
        "app.add_folder",
        Some(UiIconId::Folder),
        format!("hierarchy.create-folder:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.root",
        "app.move_to_root",
        Some(UiIconId::Scene),
        format!("hierarchy.reparent-root:{id}"),
        false,
        false,
    ));
    if is_folder {
        menu = menu.with_child(context_menu_item(
            palette,
            "hierarchy.context-menu.ungroup",
            "app.ungroup",
            Some(UiIconId::Folder),
            format!("hierarchy.ungroup:{id}"),
            false,
            false,
        ));
    }

    // Separator 3
    menu = menu.with_child(menu_separator("hierarchy.context-menu.sep3", palette));

    // Group 4: Destructive
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.delete",
        "app.delete_menu",
        Some(UiIconId::Trash),
        format!("hierarchy.delete:{id}"),
        true,
        false,
    ));

    menu
}

const CONTEXT_MENU_WIDTH: f32 = 236.0;
const EMPTY_CONTEXT_MENU_WIDTH: f32 = 226.0;
const EMPTY_PRIMITIVE_MENU_WIDTH: f32 = 220.0;
const CONTEXT_MENU_MARGIN: f32 = 8.0;
const CONTEXT_MENU_PADDING: f32 = 6.0;
const CONTEXT_MENU_GAP: f32 = 2.0;
const CONTEXT_MENU_TITLE_HEIGHT: f32 = 26.0;
const CONTEXT_MENU_BUTTON_HEIGHT: f32 = 26.0;
const CONTEXT_MENU_BASE_BUTTON_COUNT: usize = 16;
const CONTEXT_MENU_SEPARATOR_COUNT: usize = 3;
const CONTEXT_MENU_SEPARATOR_HEIGHT: f32 = 5.0;

pub(crate) fn empty_context_menu_rect(
    position: Option<[f32; 2]>,
    can_paste: bool,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let button_count = 4 + usize::from(can_paste);
    let separator_height = if can_paste {
        CONTEXT_MENU_SEPARATOR_HEIGHT + CONTEXT_MENU_GAP
    } else {
        0.0
    };
    let natural_height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + button_count as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP)
        + separator_height;
    let height = bounded_menu_height(natural_height, surface_size[1]);
    let point = position.unwrap_or([CONTEXT_MENU_MARGIN, 130.0]);
    let x = clamp_menu_axis(point[0], surface_size[0].max(0.0), EMPTY_CONTEXT_MENU_WIDTH);
    let y = clamp_menu_axis(point[1], surface_size[1].max(0.0), height);
    raf_ui::UiRect::new(x, y, EMPTY_CONTEXT_MENU_WIDTH, height)
}

pub(crate) fn empty_primitive_menu_rect(
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let main = empty_context_menu_rect(position, false, surface_size);
    let right_x = main.right() + 6.0;
    let x = if right_x + EMPTY_PRIMITIVE_MENU_WIDTH + CONTEXT_MENU_MARGIN <= surface_size[0] {
        right_x
    } else {
        (main.x - EMPTY_PRIMITIVE_MENU_WIDTH - 6.0).max(CONTEXT_MENU_MARGIN)
    };
    let natural_height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + CREATEABLE_PRIMITIVES.len() as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP);
    let height = bounded_menu_height(natural_height, surface_size[1]);
    let y = clamp_menu_axis(main.y, surface_size[1].max(0.0), height);
    raf_ui::UiRect::new(x, y, EMPTY_PRIMITIVE_MENU_WIDTH, height)
}

pub(crate) fn context_menu_rect(
    position: Option<[f32; 2]>,
    is_folder: bool,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let button_count = CONTEXT_MENU_BASE_BUTTON_COUNT + if is_folder { 1 } else { 0 };
    let natural_height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + button_count as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP)
        + CONTEXT_MENU_SEPARATOR_COUNT as f32 * (CONTEXT_MENU_SEPARATOR_HEIGHT + CONTEXT_MENU_GAP);
    let viewport_width = surface_size[0].max(0.0);
    let viewport_height = surface_size[1].max(0.0);
    let height = bounded_menu_height(natural_height, viewport_height);
    let point = position.unwrap_or([CONTEXT_MENU_MARGIN, 130.0]);
    let x = clamp_menu_axis(point[0], viewport_width, CONTEXT_MENU_WIDTH);
    let y = clamp_menu_axis(point[1], viewport_height, height);
    raf_ui::UiRect::new(x, y, CONTEXT_MENU_WIDTH, height)
}

fn bounded_menu_height(natural_height: f32, viewport_height: f32) -> f32 {
    let available = (viewport_height.max(0.0) - CONTEXT_MENU_MARGIN * 2.0).max(1.0);
    natural_height.min(available).max(1.0)
}

fn clamp_menu_axis(value: f32, viewport: f32, extent: f32) -> f32 {
    let max = (viewport - extent - CONTEXT_MENU_MARGIN).max(0.0);
    if max >= CONTEXT_MENU_MARGIN {
        value.clamp(CONTEXT_MENU_MARGIN, max)
    } else {
        value.clamp(0.0, max)
    }
}

fn context_menu_item(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    icon: Option<UiIconId>,
    command: impl Into<String>,
    danger: bool,
    disabled: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let class = if danger {
        "hierarchy-menu-button hierarchy-menu-delete"
    } else {
        "hierarchy-menu-button"
    };
    let text_color = if disabled {
        tokens.text_muted
    } else if danger {
        [240, 160, 160, 255]
    } else {
        [222, 226, 232, 255]
    };
    let mut item = UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, CONTEXT_MENU_BUTTON_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_label_key(text_key)
        .focusable()
        .disabled(disabled)
        .with_event(UiEventBinding::command(UiEventKind::Click, command));

    if let Some(icon_id) = icon {
        let icon_tint = if disabled {
            tokens.text_muted
        } else if danger {
            [220, 110, 110, 255]
        } else {
            tokens.text_muted
        };
        item = item.with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon_id)
                        .with_size(UiIconSize::Small)
                        .with_tint(icon_tint),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    } else {
        item = item.with_child(
            UiNode::new(format!("{id}.icon-spacer"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    }

    item = item.with_child(
        UiNode::new(format!("{id}.label"), UiNodeKind::Label)
            .with_text_key(text_key)
            .with_text_style(UiTextStyle::button(text_color))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content().with_text_safe_area(true)
            }),
    );

    item
}
