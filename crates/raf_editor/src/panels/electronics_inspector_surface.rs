//! Native Electronics inspector surface.
//!
//! The backend projects the selected object as `(field name, value)` pairs.
//! Field names are backend data, so they are only used as lookups; every label
//! the user reads is an i18n key resolved by this surface, and a renamed
//! backend field can no longer leak raw English into the panel.

use raf_core::session::ProjectSessionRegistry;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiFlow, UiFontWeight, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiSizeMode, UiSpacing, UiStyleSheet, UiTextInput, UiTextOverflow, UiTextRole,
    UiTextStyle,
};

use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_focus_rule, electronics_hover_rule, electronics_state_rule, with_alpha,
    ELECTRONICS_BODY_LINE_HEIGHT, ELECTRONICS_CONTROL_HEIGHT, ELECTRONICS_ITEM_HEIGHT,
    ELECTRONICS_ROW_GLYPH_TRACK, ELECTRONICS_ROW_HEIGHT, PANEL_PADDING_X, PANEL_PADDING_Y,
};
use crate::panels::inspector_surface::{inspector_style_sheet, sessions_content, InspectorTab};

/// Height of one property row, in logical points.
const FIELD_ROW_HEIGHT: f32 = ELECTRONICS_ROW_HEIGHT;
/// Height of the editable value field, in logical points.
const VALUE_INPUT_HEIGHT: f32 = ELECTRONICS_CONTROL_HEIGHT;
/// Height of a read-only value line, in logical points.
const VALUE_LABEL_HEIGHT: f32 = ELECTRONICS_ROW_HEIGHT;
/// Height of a section title, in logical points. It matches the body line box so
/// the title never paints its descenders into the first field.
const SECTION_TITLE_HEIGHT: f32 = ELECTRONICS_BODY_LINE_HEIGHT;
/// Height of the selected-object card, in logical points.
const SELECTED_CARD_HEIGHT: f32 = 54.0;

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
        let editable_value = field_value(fields, "Editable")
            .map(|value| value == "true")
            // Without the backend telling us the field is writable, present it read
            // only. A missing `Editable` entry must never render an input that
            // silently discards what the user types.
            .unwrap_or(false);
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
            locked: !editable_value,
            editable_value,
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
            padding: UiSpacing::xy(PANEL_PADDING_X + 2.0, PANEL_PADDING_Y),
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
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::xy(2.0, 0.0),
            ..UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(tab_button(
            palette,
            "electronics.inspector.properties-tab",
            "app.properties",
            UiIconId::Settings,
            "inspector.tab:properties",
            active == InspectorTab::Properties,
        ))
        .with_child(tab_button(
            palette,
            "electronics.inspector.sessions-tab",
            "app.sessions",
            UiIconId::Scene,
            "inspector.tab:sessions",
            active == InspectorTab::Sessions,
        ))
}

fn tab_button(
    palette: StudioUiPalette,
    id: &str,
    label: &str,
    icon: UiIconId,
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
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 5.0,
            grow: 1.0,
            min_size: [72.0, ELECTRONICS_CONTROL_HEIGHT],
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(label)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::button(if active {
            palette.tokens().text
        } else {
            palette.tokens().text_muted
        }))
        .with_tooltip_key(if active {
            "electronics.tooltip.inspector.active"
        } else if label == "app.properties" {
            "electronics.tooltip.inspector.properties"
        } else {
            "electronics.tooltip.inspector.sessions"
        })
        .with_accessibility_label_key(label)
        // A tab role is what puts the strip into the roving arrow-key order of
        // the shared RafUI keyboard traversal.
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_accessibility_selected(active)
        .with_accessibility_expanded(active)
        .focusable()
        .with_event(raf_ui::UiEventBinding::command(
            raf_ui::UiEventKind::Click,
            command,
        ))
}

/// Backend field name lookup. Only the backend projection uses these strings.
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
            ..UiLayout::fixed(0.0, ELECTRONICS_ITEM_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.inspector.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Settings).with_size(UiIconSize::Small))
                // A row child with no authored track resolves to zero width and
                // its glyph is never painted, so the leading icon owns one.
                .with_layout(UiLayout::fixed(
                    ELECTRONICS_ROW_GLYPH_TRACK,
                    ELECTRONICS_ITEM_HEIGHT - 12.0,
                )),
        )
        .with_child(
            UiNode::new("electronics.inspector.header.title", UiNodeKind::Label)
                .with_text_key("app.electronics_inspector")
                .with_text_overflow(UiTextOverflow::Ellipsis)
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
                .with_text_key("app.electronics_no_selection")
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
                .with_text_key("app.electronics_inspector_select_hint")
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
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
                ..UiLayout::fixed(0.0, SELECTED_CARD_HEIGHT).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("electronics.inspector.selected.icon", UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(inspector_icon_for_kind(&model.kind))
                            .with_size(UiIconSize::Panel),
                    )
                    .with_layout(UiLayout::fixed(20.0, 20.0)),
            )
            .with_child(
                UiNode::new("electronics.inspector.selected.text", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        gap: 1.0,
                        grow: 1.0,
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        UiNode::new("electronics.inspector.selected.title", UiNodeKind::Label)
                            .with_text_value(model.title.clone())
                            .with_text_overflow(UiTextOverflow::Ellipsis)
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::PanelTitle,
                                size_px: 13.0,
                                line_height_px: 16.0,
                                weight: UiFontWeight::Bold,
                                color: tokens.text,
                                inherit_color: false,
                            })
                            .with_layout(
                                UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill),
                            ),
                    )
                    .with_child(
                        UiNode::new("electronics.inspector.selected.kind", UiNodeKind::Label)
                            .with_text_value(model.kind.clone())
                            .with_text_overflow(UiTextOverflow::Ellipsis)
                            .with_text_style(electronics_body_style(tokens.text_muted))
                            .with_layout(
                                UiLayout::fixed(0.0, 15.0).with_width_mode(UiSizeMode::Fill),
                            ),
                    ),
            ),
    )
    .with_child(editable_identity(palette, model))
    .with_child(section(
        palette,
        "placement",
        "app.electronics_placement",
        &[
            ("app.position", FieldValue::text(&model.position)),
            ("app.rotation", FieldValue::text(&model.rotation)),
            (
                "app.schematic_footprint",
                FieldValue::text(&model.footprint),
            ),
            (
                "app.electronics_state",
                FieldValue::Key(if model.locked {
                    "app.electronics_locked"
                } else {
                    "app.electronics_editable"
                }),
            ),
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
        .with_child(section_title(palette, "pins", "app.electronics_pins"));
    for (index, (name, net)) in model.pins.iter().enumerate() {
        let mut net_node = UiNode::new(
            format!("electronics.inspector.pin.{index}.net"),
            UiNodeKind::Label,
        )
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(electronics_body_style(tokens.text_muted))
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill));
        net_node = if net.is_empty() {
            net_node.with_text_key("app.electronics_unconnected")
        } else {
            net_node.with_text_value(net.clone())
        };
        pins = pins.with_child(
            UiNode::new(
                format!("electronics.inspector.pin.{index}"),
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.pin.{index}.name"),
                    UiNodeKind::Label,
                )
                .with_text_value(name.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text)),
            )
            .with_child(net_node),
        );
    }
    panel = panel.with_child(pins);
    panel
}

/// Icon for a backend category value.
///
/// The value is runtime data, so this stays a tolerant icon heuristic and never
/// becomes user-visible text.
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
        .with_child(section_title(
            palette,
            "identity",
            "app.electronics_identity",
        ))
        .with_child(identity_label(palette, &model.identity_label))
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
            .with_accessibility_role(UiAccessibilityRole::Textbox)
            .with_accessibility_label_key(identity_field_key(&model.identity_label))
            // RafUI has no focus-lost event, so the field cannot commit on blur
            // without host support. The description states the one gesture that
            // applies the value instead of leaving the user to guess it.
            .with_accessibility_description_key("electronics.tooltip.inspector.value_commit")
            .with_tooltip_key("electronics.tooltip.inspector.value_commit")
            .with_layout(
                UiLayout::fixed(0.0, VALUE_INPUT_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true),
            )
            .with_text_style(electronics_body_style(tokens.text))
        } else {
            UiNode::new("electronics.inspector.identity.value", UiNodeKind::Label)
                .with_text_value(model.value.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text))
                .with_layout(
                    UiLayout::fixed(0.0, VALUE_LABEL_HEIGHT).with_width_mode(UiSizeMode::Fill),
                )
        })
        .with_child(
            UiNode::new(
                "electronics.inspector.identity.category",
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 5.0,
                ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    "electronics.inspector.identity.category.label",
                    UiNodeKind::Label,
                )
                .with_text_key("app.electronics_category")
                .with_text_style(electronics_body_style(tokens.text_muted)),
            )
            .with_child(
                UiNode::new(
                    "electronics.inspector.identity.category.value",
                    UiNodeKind::Label,
                )
                .with_text_value(model.category.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            ),
        )
}

/// One grouped property block.
fn section(
    palette: StudioUiPalette,
    id: &str,
    title_key: &'static str,
    fields: &[(&'static str, FieldValue)],
) -> UiNode {
    let mut section = UiNode::new(
        format!("electronics.inspector.section.{id}"),
        UiNodeKind::Panel,
    )
    .with_class("electronics-inspector-section")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 3.0,
        padding: UiSpacing::xy(8.0, 7.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(section_title(palette, id, title_key));
    for (index, (label_key, value)) in fields.iter().enumerate() {
        section = section.with_child(
            UiNode::new(
                format!("electronics.inspector.{id}.field.{index}"),
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                ..UiLayout::fixed(0.0, FIELD_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.{id}.field.{index}.label"),
                    UiNodeKind::Label,
                )
                .with_text_key(*label_key)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(palette.tokens().text_muted)),
            )
            .with_child(field_value_node(palette, id, index, value)),
        );
    }
    section
}

/// How one property value reaches the document.
///
/// The distinction is explicit because a formatted runtime value ("12.0, 4.0",
/// a net name) must never be guessed into a localization key, and a catalog
/// key must never be shown raw.
#[derive(Debug, Clone)]
enum FieldValue {
    /// Localized catalog key.
    Key(&'static str),
    /// Runtime value coming from the document.
    Text(String),
}

impl FieldValue {
    fn text(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

/// Value cell of a property row.
fn field_value_node(
    palette: StudioUiPalette,
    section_id: &str,
    index: usize,
    value: &FieldValue,
) -> UiNode {
    let node = UiNode::new(
        format!("electronics.inspector.{section_id}.field.{index}.value"),
        UiNodeKind::Label,
    )
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(palette.tokens().text))
    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill));
    match value {
        FieldValue::Key(key) => node.with_text_key(*key),
        FieldValue::Text(value) => node.with_text_value(value.clone()),
    }
}

fn section_title(palette: StudioUiPalette, id: &str, title_key: &'static str) -> UiNode {
    UiNode::new(
        format!("electronics.inspector.section-title.{id}"),
        UiNodeKind::Label,
    )
    .with_text_key(title_key)
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, SECTION_TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill))
}

/// Label of the identity value. The backend sends a value, not a key, so it is
/// mapped to the closest real key instead of being shown raw.
fn identity_label(palette: StudioUiPalette, label: &str) -> UiNode {
    UiNode::new(
        "electronics.inspector.identity.value-label",
        UiNodeKind::Label,
    )
    .with_text_key(identity_field_key(label))
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(palette.tokens().text_muted))
}

fn identity_field_key(backend_label: &str) -> &'static str {
    match backend_label.to_ascii_lowercase().as_str() {
        "net" | "net name" => "app.schematic_net",
        "footprint" => "app.schematic_footprint",
        _ => "app.value",
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
    let mut rules: Vec<raf_ui::UiStyleRule> = classes
        .into_iter()
        .map(|(class, fill, border, text)| electronics_class_rule(class, fill, border, text))
        .collect();
    // The shared inspector sheet already owns the tab focus ring; the value
    // field is the only Electronics-specific control that needs its own.
    rules.push(electronics_hover_rule(
        "electronics-inspector-input",
        tokens,
    ));
    rules.push(electronics_focus_rule(
        "electronics-inspector-input",
        tokens,
    ));
    rules.push(electronics_active_rule(
        "electronics-inspector-input",
        tokens,
    ));
    rules.push(electronics_state_rule(
        raf_ui::UiStyleRuleState::Always,
        "electronics-inspector-selected",
        raf_ui::UiStylePatch {
            fill: Some(with_alpha(tokens.accent, 24)),
            border: Some(tokens.accent),
            ..Default::default()
        },
    ));
    UiStyleSheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;
    use raf_ui::UiStyleSelector;
    use raf_ui::UiTokens;

    fn fields() -> Vec<(String, String)> {
        vec![
            ("Value".to_string(), "10k".to_string()),
            ("Category".to_string(), "Passive".to_string()),
            ("Position".to_string(), "12.0, 4.0".to_string()),
            ("Rotation".to_string(), "90 deg".to_string()),
            ("Footprint".to_string(), "R_0805".to_string()),
            ("Identity label".to_string(), "Net".to_string()),
            ("Editable".to_string(), "true".to_string()),
        ]
    }

    fn surface(tab: InspectorTab) -> UiSurface {
        build_electronics_inspector_surface(
            StudioUiPalette::IndustrialDark,
            "R1  Resistor",
            &fields(),
            &[("1".to_string(), "N001".to_string())],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            tab,
        )
    }

    #[test]
    fn every_property_label_is_a_key_and_never_a_backend_field_name() {
        let surface = surface(InspectorTab::Properties);
        for id in [
            "electronics.inspector.placement.field.0.label",
            "electronics.inspector.placement.field.1.label",
            "electronics.inspector.placement.field.2.label",
            "electronics.inspector.placement.field.3.label",
            "electronics.inspector.identity.value-label",
        ] {
            let node = surface.root.find(id).expect("label");
            assert!(node.text_key.is_some(), "{id} must use an i18n key");
            assert!(node.text_value.is_none(), "{id} must not show a field name");
        }
    }

    #[test]
    fn the_identity_label_of_a_wire_resolves_to_the_net_key() {
        assert_eq!(identity_field_key("Net"), "app.schematic_net");
        assert_eq!(identity_field_key("Value"), "app.value");
    }

    #[test]
    fn tabs_declare_the_tab_role_for_the_shared_arrow_key_order() {
        let surface = surface(InspectorTab::Sessions);
        let properties = surface
            .root
            .find("electronics.inspector.properties-tab")
            .expect("properties tab");
        assert_eq!(properties.accessibility_role, UiAccessibilityRole::Tab);
        assert_eq!(properties.accessibility_selected, Some(false));
        let sessions = surface
            .root
            .find("electronics.inspector.sessions-tab")
            .expect("sessions tab");
        assert_eq!(sessions.accessibility_selected, Some(true));
    }

    #[test]
    fn the_value_field_exposes_a_focus_ring_and_a_textbox_role() {
        let surface = surface(InspectorTab::Properties);
        let input = surface
            .root
            .find("electronics.inspector.value")
            .expect("value input");
        assert_eq!(input.accessibility_role, UiAccessibilityRole::Textbox);
        assert!(surface.style_sheet.rules.iter().any(|rule| {
            rule.selector == UiStyleSelector::Class("electronics-inspector-input".to_string())
                && rule.state == raf_ui::UiStyleRuleState::Focused
        }));
    }

    #[test]
    fn the_value_field_states_the_only_gesture_that_applies_it() {
        let surface = surface(InspectorTab::Properties);
        let input = surface
            .root
            .find("electronics.inspector.value")
            .expect("value input");
        assert_eq!(
            input.accessibility_description_key.as_deref(),
            Some("electronics.tooltip.inspector.value_commit")
        );
        assert_eq!(
            input.tooltip_key.as_deref(),
            Some("electronics.tooltip.inspector.value_commit")
        );
        let control = input.control.text_input().expect("text input control");
        assert_eq!(
            control.submit_command.as_deref(),
            Some("electronics.inspector.value.commit")
        );
    }

    #[test]
    fn the_inspector_passes_the_retained_layout_gate() {
        // The Sessions tab is composed by the shared Game inspector and is not
        // gated here; this pass owns the Electronics property body.
        assert_electronics_layout_gate(&surface(InspectorTab::Properties), 300, 760);
    }

    #[test]
    fn every_inspector_row_shares_the_family_density() {
        let surface = surface(InspectorTab::Properties);
        for id in [
            "electronics.inspector.placement.field.0",
            "electronics.inspector.identity.category",
            "electronics.inspector.pin.0",
        ] {
            let node = surface.root.find(id).expect("property row");
            assert_eq!(node.layout.basis[1], ELECTRONICS_ROW_HEIGHT, "{id}");
        }
        for id in [
            "electronics.inspector.properties-tab",
            "electronics.inspector.sessions-tab",
            "electronics.inspector.value",
        ] {
            let node = surface.root.find(id).expect("control");
            assert_eq!(node.layout.basis[1], ELECTRONICS_CONTROL_HEIGHT, "{id}");
        }
        assert_eq!(FIELD_ROW_HEIGHT, ELECTRONICS_ROW_HEIGHT);
        assert_eq!(VALUE_INPUT_HEIGHT, ELECTRONICS_CONTROL_HEIGHT);
        assert_eq!(SECTION_TITLE_HEIGHT, ELECTRONICS_BODY_LINE_HEIGHT);
    }

    #[test]
    fn a_locked_selection_replaces_the_field_with_a_read_only_value() {
        let mut fields = fields();
        fields.retain(|(name, _)| name != "Editable");
        let surface = build_electronics_inspector_surface(
            StudioUiPalette::IndustrialDark,
            "R1",
            &fields,
            &[],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            InspectorTab::Properties,
        );
        assert!(surface.root.find("electronics.inspector.value").is_none());
        assert!(surface
            .root
            .find("electronics.inspector.identity.value")
            .is_some());
    }

    #[test]
    fn a_property_value_never_has_to_be_guessed_as_a_key() {
        let surface = surface(InspectorTab::Properties);
        let state = surface
            .root
            .find("electronics.inspector.placement.field.3.value")
            .expect("state value");
        assert_eq!(state.text_key.as_deref(), Some("app.electronics_editable"));
        assert!(state.text_value.is_none());

        let position = surface
            .root
            .find("electronics.inspector.placement.field.0.value")
            .expect("position value");
        assert_eq!(position.text_value.as_deref(), Some("12.0, 4.0"));
        assert!(position.text_key.is_none());
    }

    #[test]
    fn both_themes_keep_the_same_section_hierarchy() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let tokens: UiTokens = palette.tokens();
            let surface = build_electronics_inspector_surface(
                palette,
                "R1",
                &fields(),
                &[],
                &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
                InspectorTab::Properties,
            );
            // The class is the shared section recipe; the id is what the
            // document actually carries.
            let section = surface
                .root
                .find("electronics.inspector.pins")
                .expect("section");
            assert_eq!(section.classes, vec!["electronics-inspector-section"]);
            let rule = surface
                .style_sheet
                .rules
                .iter()
                .find(|rule| {
                    rule.selector == UiStyleSelector::Class("electronics-inspector-section".into())
                })
                .expect("section rule");
            // The fill must come from the active palette, not from a literal
            // baked into the surface, so both themes stay coherent.
            let fill = rule.patch.fill.expect("section fill");
            assert!(
                fill == tokens.surface || fill == tokens.surface_alt,
                "section fill {fill:?} is not a palette surface token"
            );
        }
    }
}
