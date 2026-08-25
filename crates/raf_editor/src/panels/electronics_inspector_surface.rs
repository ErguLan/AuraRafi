//! Native Electronics inspector surface.

use raf_core::session::ProjectSessionRegistry;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput,
    UiTextRole, UiTextStyle,
};

use crate::panels::inspector_surface::{inspector_style_sheet, sessions_content, InspectorTab};

#[derive(Debug, Clone)]
pub struct ElectronicsInspectorModel {
    pub title: String,
    pub kind: String,
    pub value: String,
    pub category: String,
    pub position: String,
    pub rotation: String,
    pub footprint: String,
    pub pins: Vec<(String, String)>,
    pub locked: bool,
    pub editable_value: bool,
    pub identity_label: String,
}

pub fn build_electronics_inspector_surface(
    palette: StudioUiPalette,
    title: &str,
    fields: &[(String, String)],
    pins: &[(String, String)],
    sessions: &ProjectSessionRegistry,
    tab: InspectorTab,
) -> UiSurface {
    let model = if fields.is_empty() {
        None
    } else {
        Some(ElectronicsInspectorModel {
            title: title.to_string(),
            kind: field_value(fields, "Category")
                .unwrap_or("Selected item")
                .to_string(),
            value: field_value(fields, "Value").unwrap_or("-").to_string(),
            category: field_value(fields, "Category").unwrap_or("-").to_string(),
            position: field_value(fields, "Position").unwrap_or("-").to_string(),
            rotation: field_value(fields, "Rotation").unwrap_or("-").to_string(),
            footprint: field_value(fields, "Footprint").unwrap_or("-").to_string(),
            pins: pins.to_vec(),
            locked: false,
            editable_value: field_value(fields, "Editable")
                .map(|value| value == "true")
                .unwrap_or(true),
            identity_label: field_value(fields, "Identity label")
                .unwrap_or("Value")
                .to_string(),
        })
    };
    let root = UiNode::new("electronics.inspector", UiNodeKind::Panel)
        .with_class("electronics-inspector")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 7.0,
            padding: UiSpacing::xy(10.0, 9.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(header(palette))
        .with_child(tabs(palette, tab))
        .with_child(match model.as_ref() {
            Some(model) if tab == InspectorTab::Properties => selected_panel(palette, model),
            None if tab == InspectorTab::Properties => empty_panel(palette),
            _ => sessions_content(palette, sessions),
        });
    let mut surface = UiSurface::new("electronics.inspector", palette, root);
    let mut styles = inspector_style_sheet(palette);
    styles.rules.extend(style_sheet(palette).rules);
    surface.style_sheet = styles;
    surface
}

fn tabs(palette: StudioUiPalette, active: InspectorTab) -> UiNode {
    UiNode::new("electronics.inspector.tabs", UiNodeKind::Toolbar)
        .with_class("inspector-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(tab_button(
            palette,
            "electronics.inspector.properties-tab",
            "PROPERTIES",
            "inspector.tab:properties",
            active == InspectorTab::Properties,
        ))
        .with_child(tab_button(
            palette,
            "electronics.inspector.sessions-tab",
            "SESSIONS",
            "inspector.tab:sessions",
            active == InspectorTab::Sessions,
        ))
}

fn tab_button(
    palette: StudioUiPalette,
    id: &str,
    label: &str,
    command: &str,
    active: bool,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "inspector-tab-active"
        } else {
            "inspector-tab"
        })
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [88.0, 28.0],
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_text_value(label.to_string())
        .with_text_style(UiTextStyle::button(if active {
            palette.tokens().text
        } else {
            palette.tokens().text_muted
        }))
        .with_tooltip_key(if active {
            "electronics.tooltip.inspector.active"
        } else if label == "PROPERTIES" {
            "electronics.tooltip.inspector.properties"
        } else {
            "electronics.tooltip.inspector.sessions"
        })
        .with_accessibility_label_key(label)
        .focusable()
        .with_event(raf_ui::UiEventBinding::command(
            raf_ui::UiEventKind::Click,
            command,
        ))
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(label, _)| label == key)
        .map(|(_, value)| value.as_str())
}

fn header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("electronics.inspector.header", UiNodeKind::Toolbar)
        .with_class("electronics-inspector-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fixed(0.0, 38.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.inspector.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Settings).with_size(UiIconSize::Small)),
        )
        .with_child(
            UiNode::new("electronics.inspector.header.title", UiNodeKind::Label)
                .with_text_value("INSPECTOR".to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 13.0,
                    line_height_px: 17.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
}

fn empty_panel(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("electronics.inspector.empty", UiNodeKind::Panel)
        .with_class("electronics-inspector-empty")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(18.0, 24.0),
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("electronics.inspector.empty.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Select).with_size(UiIconSize::Panel)),
        )
        .with_child(
            UiNode::new("electronics.inspector.empty.title", UiNodeKind::Label)
                .with_text_value("No selection".to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 14.0,
                    line_height_px: 18.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(
            UiNode::new("electronics.inspector.empty.body", UiNodeKind::Label)
                .with_text_value(
                    "Select a component, pin, wire, trace or pad on the canvas.".to_string(),
                )
                .with_text_style(body_style(tokens.text_muted)),
        )
}

fn selected_panel(palette: StudioUiPalette, model: &ElectronicsInspectorModel) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::scroll_view(
        "electronics.inspector.scroll",
        raf_ui::UiScrollAxis::Vertical,
    )
    .with_class("electronics-inspector-scroll")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 7.0,
        grow: 1.0,
        overflow: UiOverflow::ScrollY,
        ..UiLayout::fill(UiFlow::Column)
    })
    .with_child(
        UiNode::new("electronics.inspector.selected", UiNodeKind::Panel)
            .with_class("electronics-inspector-selected")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(9.0, 8.0),
                ..UiLayout::fixed(0.0, 54.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("electronics.inspector.selected.icon", UiNodeKind::Label).with_icon(
                    UiIcon::new(inspector_icon_for_kind(&model.kind)).with_size(UiIconSize::Panel),
                ),
            )
            .with_child(
                UiNode::new("electronics.inspector.selected.text", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        gap: 1.0,
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    })
                    .with_child(
                        UiNode::new("electronics.inspector.selected.title", UiNodeKind::Label)
                            .with_text_value(model.title.clone())
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::PanelTitle,
                                size_px: 13.0,
                                line_height_px: 16.0,
                                weight: UiFontWeight::Bold,
                                color: tokens.text,
                                inherit_color: false,
                            }),
                    )
                    .with_child(
                        UiNode::new("electronics.inspector.selected.kind", UiNodeKind::Label)
                            .with_text_value(model.kind.clone())
                            .with_text_style(body_style(tokens.text_muted)),
                    ),
            ),
    )
    .with_child(editable_identity(palette, model))
    .with_child(section(
        palette,
        "Placement",
        &[
            ("Position", model.position.as_str()),
            ("Rotation", model.rotation.as_str()),
            ("Footprint", model.footprint.as_str()),
            ("State", if model.locked { "Locked" } else { "Editable" }),
        ],
    ));

    let mut pins = UiNode::new("electronics.inspector.pins", UiNodeKind::Panel)
        .with_class("electronics-inspector-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(8.0, 7.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_title(palette, "PINS"));
    for (index, (name, net)) in model.pins.iter().enumerate() {
        pins = pins.with_child(
            UiNode::new(
                format!("electronics.inspector.pin.{index}"),
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                ..UiLayout::fixed(0.0, 23.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.pin.{index}.name"),
                    UiNodeKind::Label,
                )
                .with_text_value(name.clone())
                .with_text_style(body_style(tokens.text)),
            )
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.pin.{index}.net"),
                    UiNodeKind::Label,
                )
                .with_text_value(if net.is_empty() {
                    "Unconnected".to_string()
                } else {
                    net.clone()
                })
                .with_text_style(body_style(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
            ),
        );
    }
    panel = panel.with_child(pins);
    panel
}

fn inspector_icon_for_kind(kind: &str) -> UiIconId {
    let kind = kind.to_ascii_lowercase();
    if kind.contains("trace") || kind.contains("pcb") {
        UiIconId::Pcb
    } else if kind.contains("wire") || kind.contains("connection") {
        UiIconId::Move
    } else if kind.contains("diode") {
        UiIconId::Warning
    } else {
        UiIconId::Schematic
    }
}

fn editable_identity(palette: StudioUiPalette, model: &ElectronicsInspectorModel) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("electronics.inspector.identity", UiNodeKind::Panel)
        .with_class("electronics-inspector-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 7.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_title(palette, "IDENTITY"))
        .with_child(
            UiNode::new(
                "electronics.inspector.identity.value-label",
                UiNodeKind::Label,
            )
            .with_text_value(model.identity_label.clone())
            .with_text_style(body_style(tokens.text_muted)),
        )
        .with_child(if model.editable_value {
            UiNode::text_input(
                "electronics.inspector.value",
                UiTextInput {
                    value_key: "electronics.inspector.value".to_string(),
                    placeholder_key: Some("electronics.inspector.value.placeholder".to_string()),
                    max_length: 128,
                    multiline: false,
                    password: false,
                    submit_command: Some("electronics.inspector.value.commit".to_string()),
                },
            )
            .with_class("electronics-inspector-input")
            .with_layout(
                UiLayout::fixed(0.0, 28.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true),
            )
            .with_text_style(body_style(tokens.text))
        } else {
            UiNode::new("electronics.inspector.identity.value", UiNodeKind::Label)
                .with_text_value(model.value.clone())
                .with_text_style(body_style(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill))
        })
        .with_child(
            UiNode::new("electronics.inspector.identity.category", UiNodeKind::Label)
                .with_text_value(format!("Category  {}", model.category))
                .with_text_style(body_style(tokens.text_muted)),
        )
}

fn section(palette: StudioUiPalette, title: &str, fields: &[(&str, &str)]) -> UiNode {
    let mut section = UiNode::new(
        format!("electronics.inspector.section.{}", title.to_lowercase()),
        UiNodeKind::Panel,
    )
    .with_class("electronics-inspector-section")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 3.0,
        padding: UiSpacing::xy(8.0, 7.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(section_title(palette, title));
    for (index, (label, value)) in fields.iter().enumerate() {
        section = section.with_child(
            UiNode::new(
                format!(
                    "electronics.inspector.{}.field.{index}",
                    title.to_lowercase()
                ),
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!(
                        "electronics.inspector.{}.field.{index}.label",
                        title.to_lowercase()
                    ),
                    UiNodeKind::Label,
                )
                .with_text_value((*label).to_string())
                .with_text_style(body_style(palette.tokens().text_muted)),
            )
            .with_child(
                UiNode::new(
                    format!(
                        "electronics.inspector.{}.field.{index}.value",
                        title.to_lowercase()
                    ),
                    UiNodeKind::Label,
                )
                .with_text_value((*value).to_string())
                .with_text_style(body_style(palette.tokens().text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
            ),
        );
    }
    section
}

fn section_title(palette: StudioUiPalette, title: &str) -> UiNode {
    UiNode::new(
        format!("electronics.inspector.section-title.{title}"),
        UiNodeKind::Label,
    )
    .with_text_value(title.to_uppercase())
    .with_text_style(body_style(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill))
}

fn body_style(color: [u8; 4]) -> UiTextStyle {
    UiTextStyle {
        role: UiTextRole::Body,
        size_px: 11.0,
        line_height_px: 15.0,
        weight: UiFontWeight::Regular,
        color,
        inherit_color: false,
    }
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let classes = [
        (
            "electronics-inspector",
            tokens.background,
            tokens.border,
            tokens.text,
        ),
        (
            "electronics-inspector-header",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        (
            "electronics-inspector-empty",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        (
            "electronics-inspector-scroll",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        (
            "electronics-inspector-selected",
            tokens.surface_raised,
            tokens.accent,
            tokens.text,
        ),
        (
            "electronics-inspector-section",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        (
            "electronics-inspector-input",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
    ];
    let rules = classes
        .into_iter()
        .map(|(class, fill, border, text)| {
            UiStyleRule::new(
                UiStyleSelector::Class(class.to_string()),
                UiStylePatch {
                    fill: Some(fill),
                    border: Some(border),
                    text: Some(text),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always)
        })
        .collect();
    UiStyleSheet { rules }
}
