//! Native retained context menu for the Electronics canvas.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize, UiLayout,
    UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiSurface, UiSurfaceMaterial,
};
use raf_ui::{UiAlign, UiFontWeight, UiTextRole, UiTextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsContextMenuAction {
    Select,
    Delete,
    Duplicate,
    Properties,
    Cancel,
}

pub fn build_electronics_context_menu_surface(
    palette: StudioUiPalette,
    has_selection: bool,
    can_duplicate: bool,
    can_route: bool,
) -> UiSurface {
    let mut root = UiNode::new("electronics.context-menu", UiNodeKind::Menu)
        .with_class("electronics-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fit_content()
        });
    let mut items = vec![("select", "Select", UiIconId::Select)];
    if can_duplicate {
        items.push(("duplicate", "Duplicate", UiIconId::Add));
    }
    if can_route {
        items.push(("route", "Route airwire", UiIconId::Move));
    }
    if has_selection {
        items.push(("properties", "Properties", UiIconId::Settings));
        items.push(("delete", "Delete", UiIconId::Close));
    }
    items.push(("cancel", "Cancel", UiIconId::Close));
    for (id, text, icon) in items {
        root = root.with_child(
            UiNode::new(format!("electronics.context.{id}"), UiNodeKind::Button)
                .with_class("electronics-context-item")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    padding: UiSpacing::xy(8.0, 5.0),
                    ..UiLayout::fixed(190.0, 30.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
                .with_child(
                    UiNode::new(format!("electronics.context.{id}.label"), UiNodeKind::Label)
                        .with_text_value(text.to_string())
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Button,
                            size_px: 12.0,
                            line_height_px: 16.0,
                            weight: UiFontWeight::Bold,
                            color: palette.tokens().text,
                            inherit_color: false,
                        }),
                )
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("electronics.context.{id}"),
                )),
        );
    }
    let mut surface = UiSurface::new("electronics.context-menu", palette, root);
    let tokens = palette.tokens();
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.focus),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    };
    surface
}
