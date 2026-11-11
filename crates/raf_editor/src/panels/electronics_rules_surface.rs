//! Board-wide and selected-net rules, authored in a compact inspector drawer.
use super::electronics_surface::{electronics_body_style, electronics_tokens};
use raf_electronics::pcb::PcbLayout;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAccessibilityRole, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind,
    UiScrollAxis, UiSpacing, UiTextInput, UiTextOverflow,
};

pub fn build_rules_surface(
    palette: StudioUiPalette,
    layout: &PcbLayout,
    net: Option<&str>,
) -> UiSurface {
    let t = electronics_tokens(palette);
    let rules = &layout.rules;
    let label = |id: &str, key: &str| {
        UiNode::new(id, UiNodeKind::Label)
            .with_text_key(key)
            .with_text_style(electronics_body_style(t.text_muted))
            .with_layout(UiLayout::fixed(0.0, 26.0).fill_width())
    };
    let mut body = UiNode::scroll_view("electronics.rules.scroll", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(label(
            "electronics.rules.default.title",
            "electronics.pcb.rules_default",
        ));
    for (field, key, value) in [
        (
            "width",
            "electronics.pcb.rules_width",
            rules.default.min_width,
        ),
        (
            "clearance",
            "electronics.pcb.rules_clearance",
            rules.default.clearance,
        ),
        ("edge", "electronics.pcb.rules_edge", rules.edge_clearance),
        ("drill", "electronics.pcb.rules_drill", rules.min_drill),
        ("ring", "electronics.pcb.rules_ring", rules.min_annular_ring),
        (
            "via-diameter",
            "electronics.pcb.rules_via_diameter",
            rules.via_diameter,
        ),
        (
            "via-drill",
            "electronics.pcb.rules_via_drill",
            rules.via_drill,
        ),
    ] {
        body = body.with_child(rule_field(palette, field, "", key, value));
    }
    body = body.with_child(label(
        "electronics.rules.net.title",
        "electronics.pcb.rules_net",
    ));
    if let Some(net) = net.filter(|net| !net.is_empty()) {
        body = body.with_child(
            UiNode::new("electronics.rules.net.name", UiNodeKind::Label)
                .with_text_value(net)
                .with_text_style(electronics_body_style(t.accent))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 24.0).fill_width()),
        );
        let entry = rules.for_net(net);
        for (field, key, value) in [
            ("width", "electronics.pcb.rules_width", entry.min_width),
            (
                "clearance",
                "electronics.pcb.rules_clearance",
                entry.clearance,
            ),
        ] {
            body = body.with_child(rule_field(palette, field, net, key, value));
        }
    } else {
        body = body.with_child(label(
            "electronics.rules.net.empty",
            "electronics.pcb.rules_select_net",
        ));
    }
    for (index, name) in layout.net_pins.keys().enumerate() {
        body = body.with_child(
            UiNode::new(format!("electronics.rules.net.{index}"), UiNodeKind::Button)
                .with_text_value(name)
                .with_text_style(electronics_body_style(if net == Some(name.as_str()) {
                    t.accent
                } else {
                    t.text_muted
                }))
                .with_layout(UiLayout::fixed(0.0, 28.0).fill_width())
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("electronics.net.highlight:{name}"),
                )),
        );
    }
    let root = UiNode::new("electronics.rules", UiNodeKind::Panel)
        .with_layout(UiLayout {
            padding: UiSpacing::same(12.0),
            gap: 10.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("electronics.rules.close", UiNodeKind::Button)
                .with_text_key("electronics.pcb.rules_close")
                .with_text_style(electronics_body_style(t.text))
                .with_layout(UiLayout::fixed(0.0, 34.0).fill_width())
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "electronics.pcb.menu.close",
                )),
        )
        .with_child(body);
    let mut surface = UiSurface::new("electronics.rules", palette, root);
    surface.style_sheet = super::inspector_surface::inspector_style_sheet(palette);
    surface
        .style_sheet
        .rules
        .extend(super::electronics_inspector_surface::style_sheet(palette).rules);
    surface
}

fn rule_field(palette: StudioUiPalette, field: &str, net: &str, key: &str, value: f32) -> UiNode {
    let t = electronics_tokens(palette);
    let id = format!("electronics.pcb.rule.{field}:{net}");
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(key)
                .with_text_style(electronics_body_style(t.text_muted))
                .with_layout(UiLayout::fixed(0.0, 22.0).fill_width()),
        )
        .with_child(
            UiNode::text_input(
                id.clone(),
                UiTextInput {
                    value_key: id,
                    placeholder_key: None,
                    max_length: 16,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("electronics.pcb.rule.commit:{field}:{net}")),
                },
            )
            .with_text_value(format!("{value:.3}"))
            .with_text_style(electronics_body_style(t.text))
            .with_class("electronics-inspector-input")
            .with_layout(
                UiLayout::fixed(0.0, 30.0)
                    .fill_width()
                    .with_text_safe_area(true),
            )
            .with_tooltip_key("electronics.pcb.rules_edit_help")
            .with_accessibility_label_key(key)
            .focusable(),
        )
}
