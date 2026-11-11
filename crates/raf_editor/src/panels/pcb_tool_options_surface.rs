//! Compact contextual PCB controls. Each control advertises its current value
//! and opens an explicit menu of physical values.
use crate::panels::electronics_surface::{
    electronics_class_rule, electronics_focus_rule, electronics_hover_rule,
};
use raf_electronics::PcbLayer;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiSpacing, UiTextOverflow, UiTextStyle,
};

/// Explicit resolution of parked routing. The controller retains undo history.
pub fn build_pcb_parked_traces_surface(
    palette: StudioUiPalette,
    count: usize,
    available_width: f32,
    language: raf_core::Language,
) -> UiSurface {
    let compact = available_width < 340.0;
    let t = |key| raf_core::i18n::t(key, language);
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let root = UiNode::new("electronics.pcb.parked", UiNodeKind::Toolbar)
        .with_class("pcb-parked-traces")
        .with_layout(UiLayout {
            flow: if compact { UiFlow::Column } else { UiFlow::Row },
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::same(4.0),
            ..UiLayout::fixed(
                if compact { 184.0 } else { 340.0 },
                if compact { 64.0 } else { 36.0 },
            )
        })
        .with_child(
            UiNode::new("electronics.pcb.parked.count", UiNodeKind::Label)
                .with_text_value(format!("{count} {}", t("electronics.pcb.parked_traces")))
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(if compact {
                    UiLayout::fixed(176.0, 24.0)
                } else {
                    UiLayout::fixed(0.0, 26.0).fill_width().grow(1.0)
                }),
        )
        .with_child(
            UiNode::new("electronics.pcb.parked.discard", UiNodeKind::Button)
                .with_class("pcb-parked-discard")
                .with_text_key("electronics.pcb.discard_parked")
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout::fixed(if compact { 176.0 } else { 96.0 }, 26.0))
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_label_key("electronics.pcb.discard_parked")
                .with_tooltip_key("electronics.pcb.discard_parked_help")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "electronics.pcb.discard_removed_traces",
                )),
        );
    let mut surface = UiSurface::new("electronics.pcb.parked", palette, root);
    surface.style_sheet.rules.push(electronics_class_rule(
        "pcb-parked-traces",
        tokens.surface,
        tokens.warning,
        tokens.text,
    ));
    surface.style_sheet.rules.extend([
        electronics_class_rule(
            "pcb-parked-discard",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_hover_rule("pcb-parked-discard", tokens),
        electronics_focus_rule("pcb-parked-discard", tokens),
    ]);
    surface
}

/// The same row reservation is shared by painting and CAD hit testing.
pub const PCB_TOOL_OPTIONS_HEIGHT: f32 = 80.0;
pub fn pcb_tool_options_height(width: f32) -> f32 {
    if width >= 760.0 {
        44.0
    } else {
        PCB_TOOL_OPTIONS_HEIGHT
    }
}

pub fn build_pcb_tool_options_surface(
    palette: StudioUiPalette,
    layer: PcbLayer,
    width: f32,
    grid: f32,
    available_width: f32,
    language: raf_core::Language,
) -> UiSurface {
    build_pcb_tool_options_surface_with_zoom(
        palette,
        layer,
        width,
        grid,
        100.0,
        available_width,
        language,
        None,
    )
}

pub fn build_pcb_tool_options_surface_with_zoom(
    palette: StudioUiPalette,
    layer: PcbLayer,
    width: f32,
    grid: f32,
    zoom: f32,
    available_width: f32,
    language: raf_core::Language,
    open: Option<&str>,
) -> UiSurface {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let tr = |key| raf_core::i18n::t(key, language);
    let choices = [
        (
            "layer",
            "electronics.desktop.active_layer",
            tr(if layer == PcbLayer::TopCopper {
                "electronics.pcb.copper_top"
            } else {
                "electronics.pcb.copper_bottom"
            }),
        ),
        (
            "width",
            "electronics.desktop.track_width",
            format!("{width:.2} mm"),
        ),
        ("grid", "electronics.desktop.grid", format!("{grid:.2} mm")),
        ("zoom", "electronics.desktop.zoom", format!("{zoom:.0}%")),
    ];
    let columns = if available_width >= 760.0 { 4 } else { 2 };
    let control_width =
        ((available_width - 16.0 - (columns - 1) as f32 * 6.0) / columns as f32).max(0.0);
    let mut root = UiNode::new("electronics.pcb.options", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fixed(available_width, pcb_tool_options_height(available_width))
        })
        .with_style(raf_ui::UiStyle::transparent().with_fill(t.surface));
    for (row_index, chunk) in choices.chunks(columns).enumerate() {
        let mut row = UiNode::new(
            format!("electronics.pcb.options.row.{row_index}"),
            UiNodeKind::Toolbar,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 32.0).fill_width()
        });
        for (id, label, value) in chunk {
            let mut button =
                UiNode::new(format!("electronics.pcb.options.{id}"), UiNodeKind::Button)
                    .with_class("pcb-tool-option")
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 4.0,
                        padding: UiSpacing::xy(6.0, 0.0),
                        ..UiLayout::fixed(control_width, 32.0)
                    })
                    .with_accessibility_role(UiAccessibilityRole::Button)
                    .with_accessibility_label_key(*label)
                    .with_accessibility_expanded(open == Some(*id))
                    .with_tooltip_key(*label)
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("electronics.pcb.menu:{id}"),
                    ));
            if *id == "layer" {
                button = button.with_child(
                    UiNode::new("electronics.pcb.options.layer.swatch", UiNodeKind::Panel)
                        .with_layout(UiLayout::fixed(12.0, 16.0))
                        .with_style(raf_ui::UiStyle::transparent().with_fill(
                            if layer == PcbLayer::TopCopper {
                                [246, 88, 69, 255]
                            } else {
                                [71, 144, 240, 255]
                            },
                        )),
                );
            }
            // A single text child fits readable localized captions without allocating a phantom icon inset.
            button = button
                .with_child(
                    UiNode::new(
                        format!("electronics.pcb.options.{id}.value"),
                        UiNodeKind::Label,
                    )
                    .with_text_value(if available_width >= 760.0 {
                        value.clone()
                    } else {
                        format!("{}: {}", tr(label), value)
                    })
                    .with_text_style(crate::panels::electronics_surface::electronics_body_style(
                        t.text,
                    ))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(
                        UiLayout::fixed(0.0, 24.0)
                            .fill_width()
                            .grow(1.0)
                            .with_text_safe_area(false),
                    ),
                )
                .with_child(
                    UiNode::new(
                        format!("electronics.pcb.options.{id}.arrow"),
                        UiNodeKind::Label,
                    )
                    .with_icon(raf_ui::UiIcon::new(raf_ui::UiIconId::ChevronDown))
                    .with_layout(UiLayout::fixed(12.0, 18.0)),
                );
            if available_width >= 760.0 {
                let label_width = match *id {
                    "layer" => 52.0,
                    "width" => 48.0,
                    "grid" => 80.0,
                    _ => 46.0,
                };
                button.layout.basis[0] = (control_width - label_width - 6.0).max(72.0);
                row = row.with_child(
                    UiNode::new(
                        format!("electronics.pcb.options.{id}.group"),
                        UiNodeKind::Panel,
                    )
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 6.0,
                        align_items: UiAlign::Center,
                        ..UiLayout::fixed(control_width, 32.0)
                    })
                    .with_child(
                        UiNode::new(
                            format!("electronics.pcb.options.{id}.label"),
                            UiNodeKind::Label,
                        )
                        .with_text_key(*label)
                        .with_text_style(
                            crate::panels::electronics_surface::electronics_body_style(
                                t.text_muted,
                            ),
                        )
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout::fixed(label_width, 24.0)),
                    )
                    .with_child(button),
                );
            } else {
                row = row.with_child(button);
            }
        }
        root = root.with_child(row);
    }
    let mut surface = UiSurface::new("electronics.pcb.options", palette, root);
    surface.style_sheet.rules.extend([
        electronics_class_rule("pcb-tool-option", t.surface_alt, t.border, t.text),
        electronics_hover_rule("pcb-tool-option", t),
        electronics_focus_rule("pcb-tool-option", t),
    ]);
    surface
}

pub fn pcb_menu_anchor(
    canvas: crate::editor_layout::EditorRect,
    menu: &str,
) -> crate::editor_layout::EditorRect {
    let x = canvas.x + crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH;
    let width =
        (canvas.width - crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH).max(0.0);
    let columns = if width >= 760.0 { 4 } else { 2 };
    let slot = match menu {
        "width" => 1,
        "grid" => 2,
        "zoom" => 3,
        _ => 0,
    };
    let step = (width - 16.0 - 6.0 * (columns - 1) as f32) / columns as f32 + 6.0;
    crate::editor_layout::EditorRect::new(
        x + 8.0 + (slot % columns) as f32 * step,
        canvas.y
            + crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT
            + 38.0
            + (slot / columns) as f32 * 36.0,
        step.max(140.0).min(280.0),
        200.0,
    )
}

/// Menu items apply concrete values. Selecting an item closes the menu in the host.
pub fn build_pcb_options_menu(
    palette: StudioUiPalette,
    menu: &str,
    language: raf_core::Language,
    layer: PcbLayer,
    width: f32,
    grid: f32,
    zoom: f32,
) -> UiSurface {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let options: Vec<(String, String, bool)> = match menu {
        "layer" => [
            ("top", "electronics.pcb.copper_top", PcbLayer::TopCopper),
            (
                "bottom",
                "electronics.pcb.copper_bottom",
                PcbLayer::BottomCopper,
            ),
        ]
        .into_iter()
        .map(|(id, key, v)| {
            (
                raf_core::i18n::t(key, language),
                format!("electronics.pcb.layer:{id}"),
                v == layer,
            )
        })
        .collect(),
        "width" => [0.15, 0.2, 0.25, 0.5, 0.8, 1.0]
            .into_iter()
            .map(|v| {
                (
                    format!("{v:.2} mm"),
                    format!("electronics.pcb.width:{v}"),
                    (v - width).abs() < 0.001,
                )
            })
            .collect(),
        "grid" => [0.1, 0.25, 0.5, 1.0, 2.54]
            .into_iter()
            .map(|v| {
                (
                    format!("{v:.2} mm"),
                    format!("electronics.pcb.grid:{v}"),
                    (v - grid).abs() < 0.001,
                )
            })
            .collect(),
        "zoom" => [50.0, 100.0, 200.0, 400.0, 800.0]
            .into_iter()
            .map(|v| {
                (
                    format!("{v:.0}%"),
                    format!("electronics.pcb.zoom:{v}"),
                    (v - zoom).abs() < 1.0,
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut root = UiNode::new("electronics.pcb.options.menu", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(4.0),
            ..UiLayout::fit_content().fill_width()
        })
        .with_style(raf_ui::UiStyle {
            fill: t.surface_raised,
            border: t.border,
            text: t.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        });
    for (index, (label, command, selected)) in options.into_iter().enumerate() {
        root = root.with_child(
            UiNode::new(
                format!("electronics.pcb.options.menu.{index}"),
                UiNodeKind::Button,
            )
            .with_layout(UiLayout {
                padding: UiSpacing::xy(8.0, 0.0),
                ..UiLayout::fixed(0.0, 30.0).fill_width()
            })
            .with_class("pcb-menu-item")
            .with_text_value(label)
            .with_text_style(crate::panels::electronics_surface::electronics_body_style(
                t.text,
            ))
            .with_style(raf_ui::UiStyle::transparent().with_fill(if selected {
                t.selection
            } else {
                [0, 0, 0, 0]
            }))
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_selected(selected)
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }
    let mut surface = UiSurface::new("electronics.pcb.options.menu", palette, root);
    surface.style_sheet.rules.extend([
        electronics_hover_rule("pcb-menu-item", t),
        electronics_focus_rule("pcb-menu-item", t),
    ]);
    surface
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcb_tool_options_open_explicit_menus_with_real_values() {
        for language in [raf_core::Language::Spanish, raf_core::Language::English] {
            for width in [340, 432, 720, 1080] {
                let s = build_pcb_tool_options_surface(
                    StudioUiPalette::IndustrialDark,
                    PcbLayer::TopCopper,
                    0.25,
                    0.25,
                    width as f32,
                    language,
                );
                crate::panels::electronics_surface::assert_electronics_layout_gate(
                    &s,
                    width,
                    pcb_tool_options_height(width as f32) as u32,
                );
                for id in ["layer", "width", "grid", "zoom"] {
                    let node = s
                        .root
                        .find(&format!("electronics.pcb.options.{id}"))
                        .unwrap();
                    assert!(node.event_handlers.iter().any(|e|matches!(&e.action,raf_ui::UiAction::Command{name:c} if c==&format!("electronics.pcb.menu:{id}"))));
                    let menu = build_pcb_options_menu(
                        StudioUiPalette::IndustrialDark,
                        id,
                        language,
                        PcbLayer::TopCopper,
                        0.25,
                        0.25,
                        100.0,
                    );
                    assert!(menu.root.children.len() >= 2);
                }
            }
        }
    }
}
