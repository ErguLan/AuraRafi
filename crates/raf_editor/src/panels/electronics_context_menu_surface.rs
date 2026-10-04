//! Native retained context menu for the Electronics canvas.
//!
//! The menu is an elevated window-level overlay: the host resolves the anchor
//! and clamps it to the window, this module only describes the rows. Every
//! visible label is an i18n key, and every row is a real `MenuItem` for
//! assistive technology instead of a button that only looks like one.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiStylePatch,
    UiStyleRuleState, UiStyleSheet, UiSurface, UiSurfaceMaterial, UiTextOverflow,
};
use raf_ui::{UiAlign, UiTokens};

use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_focus_rule, electronics_hover_rule, electronics_state_rule, with_alpha,
    DANGER_FILL_ALPHA, DANGER_HOVER_ALPHA, ELECTRONICS_CONTROL_HEIGHT, ELECTRONICS_ROW_GLYPH_TRACK,
};

/// Presented menu width, in logical points.
///
/// The host must use this constant for the overlay rectangle so the rows and
/// the surface can never disagree about the panel width.
pub const ELECTRONICS_CONTEXT_MENU_WIDTH: f32 = 212.0;
/// Presented menu height, in logical points. Enough for the widest menu
/// (select, duplicate, route, delete, cancel) plus its padding.
pub const ELECTRONICS_CONTEXT_MENU_HEIGHT: f32 = 194.0;
/// Inner padding of the menu panel, in logical points.
const MENU_PADDING: f32 = 6.0;
/// Row height, in logical points. A menu row is a control, so it resolves to
/// the same track as the toolbar buttons and the dock actions.
const MENU_ROW_HEIGHT: f32 = ELECTRONICS_CONTROL_HEIGHT;
/// Vertical padding of a menu row, in logical points.
const MENU_ROW_PADDING_Y: f32 = 5.0;
/// Content track of a menu row: its height minus its vertical padding.
const MENU_ROW_CONTENT_HEIGHT: f32 = MENU_ROW_HEIGHT - MENU_ROW_PADDING_Y * 2.0;
/// Horizontal gap between the row icon and its label, in logical points.
const MENU_ROW_GAP: f32 = 8.0;

/// One row of the canvas context menu.
struct MenuRow {
    /// Row identity inside the retained document.
    id: &'static str,
    /// Command the host already routes for this row.
    command: &'static str,
    label_key: &'static str,
    tooltip_key: &'static str,
    icon: UiIconId,
    danger: bool,
}

/// Builds the retained Electronics canvas context menu.
///
/// `has_selection`, `can_duplicate` and `can_route` describe the real target, so
/// the menu never offers an action the controller would reject.
pub fn build_electronics_context_menu_surface(
    palette: StudioUiPalette,
    has_selection: bool,
    can_duplicate: bool,
    can_route: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut rows: Vec<MenuRow> = Vec::new();
    if has_selection {
        rows.push(MenuRow {
            id: "select",
            command: "electronics.context.select",
            label_key: "electronics.context.select",
            tooltip_key: "electronics.tooltip.context.select",
            icon: UiIconId::Select,
            danger: false,
        });
    }
    if can_duplicate {
        rows.push(MenuRow {
            id: "duplicate",
            command: "electronics.context.duplicate",
            label_key: "electronics.context.duplicate",
            tooltip_key: "electronics.tooltip.context.duplicate",
            icon: UiIconId::Copy,
            danger: false,
        });
    }
    if can_route {
        rows.push(MenuRow {
            id: "route",
            command: "electronics.context.route",
            label_key: "electronics.context.route",
            tooltip_key: "electronics.tooltip.context.route",
            icon: UiIconId::Route,
            danger: false,
        });
    }
    if has_selection {
        rows.push(MenuRow {
            id: "delete",
            command: "electronics.context.delete",
            label_key: "electronics.context.delete",
            tooltip_key: "electronics.tooltip.delete",
            icon: UiIconId::Trash,
            danger: true,
        });
    }
    // Right-clicking empty canvas has no target to act on. Offering a single
    // decorative "Cancel" was the whole menu, so the one view command that is
    // meaningful without a selection takes its place.
    if rows.is_empty() {
        rows.push(MenuRow {
            id: "fit",
            command: "electronics.fit",
            label_key: "electronics.context.fit",
            tooltip_key: "electronics.tooltip.fit",
            icon: UiIconId::Focus,
            danger: false,
        });
    }
    rows.push(MenuRow {
        id: "cancel",
        command: "electronics.context.cancel",
        label_key: "electronics.context.cancel",
        tooltip_key: "app.cancel",
        icon: UiIconId::Close,
        danger: false,
    });

    let mut root = UiNode::new("electronics.context-menu", UiNodeKind::Menu)
        .with_class("electronics-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_accessibility_role(UiAccessibilityRole::Menu)
        .with_accessibility_label_key("electronics.context.title")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(MENU_PADDING),
            ..UiLayout::fixed(
                ELECTRONICS_CONTEXT_MENU_WIDTH,
                ELECTRONICS_CONTEXT_MENU_HEIGHT,
            )
        });
    for row in &rows {
        root = root.with_child(menu_row(&tokens, row));
    }

    let mut surface = UiSurface::new("electronics.context-menu", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn menu_row(tokens: &UiTokens, row: &MenuRow) -> UiNode {
    let id = format!("electronics.context.{}", row.id);
    let class = if row.danger {
        "electronics-context-item electronics-context-item-danger"
    } else {
        "electronics-context-item"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        // The row fills the panel minus its padding, so a row track and the menu
        // panel can never disagree about their width.
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: MENU_ROW_GAP,
            padding: UiSpacing::xy(8.0, MENU_ROW_PADDING_Y),
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, MENU_ROW_HEIGHT],
            ..UiLayout::default()
        })
        .with_accessibility_role(UiAccessibilityRole::MenuItem)
        .with_accessibility_label_key(row.label_key)
        .with_tooltip_key(row.tooltip_key)
        // A node that owns an icon but no text of its own centers that icon in
        // its whole content box, which floated every glyph in the middle of the
        // row. The glyph therefore owns an explicit leading track, the same
        // contract the analysis rows and panel headers use.
        .with_child(
            UiNode::new(
                format!("electronics.context.{}.icon", row.id),
                UiNodeKind::Label,
            )
            .with_icon(UiIcon::new(row.icon).with_size(UiIconSize::Small))
            .with_layout(UiLayout::fixed(
                ELECTRONICS_ROW_GLYPH_TRACK,
                MENU_ROW_CONTENT_HEIGHT,
            )),
        )
        .with_child(
            UiNode::new(
                format!("electronics.context.{}.label", row.id),
                UiNodeKind::Label,
            )
            .with_text_key(row.label_key)
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text))
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, row.command))
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let mut rules = vec![
        // Base layers first, then one ordered pass per interaction state so a
        // destructive row keeps its tint while still showing the shared rings.
        electronics_class_rule(
            "electronics-context-menu",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-context-item",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-context-item-danger",
            with_alpha(tokens.danger, DANGER_FILL_ALPHA),
            tokens.danger,
            tokens.text,
        ),
    ];
    for class in [
        "electronics-context-item",
        "electronics-context-item-danger",
    ] {
        rules.push(electronics_hover_rule(class, tokens));
        rules.push(electronics_focus_rule(class, tokens));
        rules.push(electronics_active_rule(class, tokens));
    }
    // The destructive row keeps the danger fill while still lifting on hover,
    // so it uses the same alpha ramp as the delete confirmation buttons.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-context-item-danger",
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_HOVER_ALPHA)),
            border: Some(tokens.danger),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    UiStyleSheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;
    use raf_ui::UiAction;
    use raf_ui::UiStyleSelector;

    fn build(has_selection: bool, can_duplicate: bool, can_route: bool) -> UiSurface {
        build_electronics_context_menu_surface(
            StudioUiPalette::IndustrialDark,
            has_selection,
            can_duplicate,
            can_route,
        )
    }

    fn command_of(surface: &UiSurface, id: &str) -> Option<String> {
        let node = surface.root.find(id)?;
        node.event_handlers
            .iter()
            .find_map(|event| match &event.action {
                UiAction::Command { name } => Some(name.clone()),
                _ => None,
            })
    }

    #[test]
    fn no_visible_label_is_a_raw_english_literal() {
        let surface = build(true, true, true);
        for row in surface.root.children.iter() {
            let label = row
                .children
                .iter()
                .find(|child| child.id.ends_with(".label"))
                .expect("row label");
            assert!(
                label.text_key.is_some(),
                "{} must use an i18n key",
                label.id
            );
            assert!(label.text_value.is_none());
        }
    }

    #[test]
    fn every_row_is_a_focusable_menu_item_with_its_own_role() {
        let surface = build(true, true, true);
        let rows: Vec<&UiNode> = surface
            .root
            .children
            .iter()
            .filter(|node| node.id.starts_with("electronics.context."))
            .collect();
        assert!(!rows.is_empty());
        for row in rows {
            assert_eq!(row.accessibility_role, UiAccessibilityRole::MenuItem);
            assert!(row.focusable);
            assert!(row.accessibility_label_key.is_some(), "{}", row.id);
        }
    }

    fn row_icon(surface: &UiSurface, id: &str) -> Option<UiIconId> {
        surface
            .root
            .find(&format!("electronics.context.{id}.icon"))
            .and_then(|node| node.icon.map(|icon| icon.id))
    }

    #[test]
    fn delete_and_cancel_do_not_share_an_icon() {
        let surface = build(true, false, false);
        assert_eq!(row_icon(&surface, "delete"), Some(UiIconId::Trash));
        assert_eq!(row_icon(&surface, "cancel"), Some(UiIconId::Close));
    }

    #[test]
    fn routing_uses_the_route_icon_instead_of_the_pan_icon() {
        let surface = build(false, false, true);
        assert_eq!(row_icon(&surface, "route"), Some(UiIconId::Route));
    }

    #[test]
    fn no_two_context_actions_share_one_glyph() {
        let surface = build(true, true, true);
        let ids = ["select", "duplicate", "route", "delete", "cancel"];
        for (index, id) in ids.iter().enumerate() {
            let icon = row_icon(&surface, id).expect("row icon");
            for other in &ids[index + 1..] {
                assert_ne!(
                    row_icon(&surface, other),
                    Some(icon),
                    "{id} and {other} share one glyph"
                );
            }
        }
    }

    #[test]
    fn every_row_glyph_owns_a_leading_track_instead_of_floating() {
        let surface = build(true, true, true);
        for row in surface
            .root
            .children
            .iter()
            .filter(|node| node.id.starts_with("electronics.context."))
        {
            let icon = row
                .children
                .iter()
                .find(|child| child.id.ends_with(".icon"))
                .unwrap_or_else(|| panic!("{} needs a leading glyph track", row.id));
            assert_eq!(icon.layout.basis[0], ELECTRONICS_ROW_GLYPH_TRACK);
            let label = row
                .children
                .iter()
                .find(|child| child.id.ends_with(".label"))
                .expect("row label");
            assert_eq!(label.layout.width_mode, UiSizeMode::Fill);
        }
    }

    #[test]
    fn a_decorative_properties_entry_is_gone() {
        let surface = build(true, true, true);
        assert!(surface
            .root
            .find("electronics.context.properties")
            .is_none());
    }

    #[test]
    fn an_empty_target_offers_a_routed_command_instead_of_only_cancel() {
        let surface = build(false, false, false);
        assert!(surface.root.find("electronics.context.fit").is_some());
        assert_eq!(
            command_of(&surface, "electronics.context.fit").as_deref(),
            Some("electronics.fit")
        );
    }

    #[test]
    fn rows_expose_focus_and_pressed_states() {
        let surface = build(true, false, false);
        let states: Vec<UiStyleRuleState> = surface
            .style_sheet
            .rules
            .iter()
            .filter(|rule| {
                rule.selector == UiStyleSelector::Class("electronics-context-item".to_string())
            })
            .map(|rule| rule.state)
            .collect();
        assert!(states.contains(&UiStyleRuleState::Focused));
        assert!(states.contains(&UiStyleRuleState::Active));
        assert!(states.contains(&UiStyleRuleState::Hovered));
    }

    #[test]
    fn the_row_track_fits_the_menu_panel() {
        assert!(
            ELECTRONICS_CONTEXT_MENU_WIDTH - MENU_PADDING * 2.0 > MENU_ROW_HEIGHT,
            "the row track must fit the panel minus its padding"
        );
        assert!(
            ELECTRONICS_CONTEXT_MENU_HEIGHT > MENU_ROW_HEIGHT * 5.0 + MENU_PADDING * 2.0,
            "the widest menu must fit its five rows"
        );
        assert_eq!(MENU_ROW_HEIGHT, ELECTRONICS_CONTROL_HEIGHT);
    }

    #[test]
    fn the_resolved_row_measure_exactly_the_panel_minus_its_padding() {
        let surface = build(true, true, true);
        let frame = surface.build_frame(
            ELECTRONICS_CONTEXT_MENU_WIDTH as u32,
            ELECTRONICS_CONTEXT_MENU_HEIGHT as u32,
            [0, 0, 0, 255],
        );
        let menu = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "electronics.context-menu")
            .expect("menu box");
        for row in frame
            .layout_boxes
            .iter()
            .filter(|layout| layout.id.starts_with("electronics.context."))
            .filter(|layout| layout.id != "electronics.context-menu")
            .filter(|layout| !layout.id.ends_with(".icon") && !layout.id.ends_with(".label"))
        {
            assert!(
                (row.rect.width - (menu.rect.width - MENU_PADDING * 2.0)).abs() < 0.01,
                "{} is {} wide inside a {} panel",
                row.id,
                row.rect.width,
                menu.rect.width
            );
            assert_eq!(row.rect.height, MENU_ROW_HEIGHT, "{}", row.id);
        }
    }

    #[test]
    fn the_context_menu_passes_the_retained_layout_gate() {
        for (selection, duplicate, route) in [
            (true, true, true),
            (false, false, false),
            (true, false, false),
        ] {
            let surface = build(selection, duplicate, route);
            assert_electronics_layout_gate(
                &surface,
                ELECTRONICS_CONTEXT_MENU_WIDTH as u32,
                ELECTRONICS_CONTEXT_MENU_HEIGHT as u32,
            );
        }
    }
}
