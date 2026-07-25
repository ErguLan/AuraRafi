//! Retained RafUI inspectors for the Electronics workspace.
//!
//! The schematic and PCB view panels remain the document owners. These
//! inspectors expose their existing editable fields as typed retained actions.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_electronics::{PcbLayer, Schematic};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiRange, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle, UiToggle,
};

use super::pcb_view::{PcbSelection, PcbViewPanel};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::schematic_view::{SchematicSelection, SchematicViewPanel};

#[derive(Debug, Clone, PartialEq)]
pub enum ElectronicsInspectorAction {
    Text { field: String, value: String },
    Range { field: String, value: f32 },
    Toggle { field: String, value: bool },
    Layer { field: String, value: PcbLayer },
}

pub struct ElectronicsInspectorSurfaceHost {
    schematic_bridge: RafUiSurfaceBridge,
    pcb_bridge: RafUiSurfaceBridge,
}

impl Default for ElectronicsInspectorSurfaceHost {
    fn default() -> Self {
        Self {
            schematic_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_schematic_inspector"),
            pcb_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_pcb_inspector"),
        }
    }
}

impl ElectronicsInspectorSurfaceHost {
    pub fn show_schematic(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &SchematicViewPanel,
        lang: Language,
    ) -> Vec<ElectronicsInspectorAction> {
        let selected = view.selection();
        let surface = build_schematic_surface(palette, &view.schematic, selected, lang);
        let (reference, value, net) = match view.selection() {
            SchematicSelection::Component(index) => view
                .schematic
                .components
                .get(index)
                .map(|component| {
                    (
                        component.designator.clone(),
                        component.value.clone(),
                        String::new(),
                    )
                })
                .unwrap_or_default(),
            SchematicSelection::Wire(index) => (
                String::new(),
                String::new(),
                view.schematic
                    .wires
                    .get(index)
                    .map(|wire| wire.net.clone())
                    .unwrap_or_default(),
            ),
            _ => (String::new(), String::new(), String::new()),
        };
        let dispatched = self.schematic_bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                controls.set_text("electronics.schematic.reference", reference.clone(), 48);
                controls.set_text("electronics.schematic.value", value.clone(), 128);
                controls.set_text("electronics.schematic.net", net.clone(), 128);
            },
            |key| t(key, lang),
        );
        Self::collect(dispatched)
    }

    pub fn show_pcb(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &PcbViewPanel,
        lang: Language,
    ) -> Vec<ElectronicsInspectorAction> {
        let surface = build_pcb_surface(palette, view, lang);
        let (reference, value) = match view.selection() {
            PcbSelection::Component(index) => view
                .layout
                .components
                .get(index)
                .map(|component| (component.designator.clone(), component.value.clone()))
                .unwrap_or_default(),
            _ => (String::new(), String::new()),
        };
        let dispatched = self.pcb_bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                controls.set_text("electronics.pcb.reference", reference.clone(), 48);
                controls.set_text("electronics.pcb.value", value.clone(), 128);
            },
            |key| t(key, lang),
        );
        Self::collect(dispatched)
    }

    fn collect(actions: Vec<raf_ui::UiDispatchedAction>) -> Vec<ElectronicsInspectorAction> {
        actions
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::SetText { key, value } => {
                    Some(ElectronicsInspectorAction::Text { field: key, value })
                }
                UiAction::SetRange { key, value } => {
                    Some(ElectronicsInspectorAction::Range { field: key, value })
                }
                UiAction::SetToggle { key, value } => {
                    Some(ElectronicsInspectorAction::Toggle { field: key, value })
                }
                UiAction::Command { name } => parse_layer_command(&name),
                _ => None,
            })
            .collect()
    }
}

fn build_schematic_surface(
    palette: StudioUiPalette,
    schematic: &Schematic,
    selection: SchematicSelection,
    lang: Language,
) -> UiSurface {
    let mut content = base_content(palette, "electronics.schematic.scroll");
    match selection {
        SchematicSelection::Component(index) => {
            if let Some(component) = schematic.components.get(index) {
                content = content
                    .with_child(section_label(palette, "app.schematic_component"))
                    .with_child(text_field(
                        palette,
                        "app.electronics_reference",
                        "electronics.schematic.reference",
                        "electronics.schematic.reference.input",
                    ))
                    .with_child(text_field(
                        palette,
                        "app.value",
                        "electronics.schematic.value",
                        "electronics.schematic.value.input",
                    ))
                    .with_child(info_line(
                        palette,
                        format!("{} | {}", component.kind_label(), component.footprint),
                    ))
                    .with_child(section_label(palette, "app.electronics_position"))
                    .with_child(range_row(
                        palette,
                        "app.position",
                        "electronics.schematic.position",
                        [component.position.x, component.position.y],
                        -1000.0,
                        1000.0,
                        0.1,
                    ))
                    .with_child(single_range(
                        palette,
                        "app.rotation",
                        "electronics.schematic.rotation",
                        component.rotation,
                        -360.0,
                        360.0,
                        1.0,
                    ))
                    .with_child(section_label(palette, "app.electronics_appearance"))
                    .with_child(toggle(
                        palette,
                        "app.visible",
                        "electronics.schematic.visible",
                        component.visible,
                    ))
                    .with_child(toggle(
                        palette,
                        "app.pcb_locked",
                        "electronics.schematic.locked",
                        component.locked,
                    ))
                    .with_child(section_label(palette, "app.electronics_electrical"));
                let netlist = schematic.netlist();
                for (pin_index, pin) in component.pins.iter().enumerate() {
                    let net = netlist
                        .net_for_pin(index, pin_index)
                        .map(|net| net.name.clone())
                        .unwrap_or_else(|| "-".to_string());
                    content = content.with_child(info_line(
                        palette,
                        format!(
                            "{} | {} | {net}",
                            pin.name,
                            pin_direction_label(pin.direction, lang)
                        ),
                    ));
                }
            }
        }
        SchematicSelection::MultipleComponents(indices) => {
            content = content
                .with_child(section_label(palette, "app.electronics_selection"))
                .with_child(info_line(
                    palette,
                    format!("{}: {}", t("app.schematic_components", lang), indices.len()),
                ));
        }
        SchematicSelection::Wire(index) => {
            if let Some(wire) = schematic.wires.get(index) {
                content = content
                    .with_child(section_label(palette, "app.schematic_wire"))
                    .with_child(text_field(
                        palette,
                        "app.schematic_net",
                        "electronics.schematic.net",
                        "electronics.schematic.net.input",
                    ))
                    .with_child(info_line(
                        palette,
                        format!(
                            "{}: {:.1} | {}: {}",
                            t("app.schematic_length", lang),
                            wire.start.distance(wire.end),
                            t("app.electronics_segments", lang),
                            1
                        ),
                    ));
            }
        }
        SchematicSelection::None => {
            content = content
                .with_child(section_label(palette, "app.schematic_properties_root"))
                .with_child(info_line(
                    palette,
                    format!(
                        "{} {} | {} {} | {} {}",
                        t("app.schematic_components", lang),
                        schematic.components.len(),
                        t("app.schematic_wires", lang),
                        schematic.wires.len(),
                        t("app.schematic_nets", lang),
                        schematic.netlist().nets.len()
                    ),
                ));
        }
    }
    let root = root_with_header(palette, "app.electronics_inspector", content);
    let mut surface = UiSurface::new("electronics-schematic-inspector", palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

fn build_pcb_surface(palette: StudioUiPalette, view: &PcbViewPanel, lang: Language) -> UiSurface {
    let mut content = base_content(palette, "electronics.pcb.scroll");
    match view.selection() {
        PcbSelection::Component(index) => {
            if let Some(component) = view.layout.components.get(index) {
                content = content
                    .with_child(section_label(palette, "app.pcb_component"))
                    .with_child(text_field(
                        palette,
                        "app.electronics_reference",
                        "electronics.pcb.reference",
                        "electronics.pcb.reference.input",
                    ))
                    .with_child(text_field(
                        palette,
                        "app.value",
                        "electronics.pcb.value",
                        "electronics.pcb.value.input",
                    ))
                    .with_child(info_line(
                        palette,
                        format!(
                            "{}: {}",
                            t("app.schematic_footprint", lang),
                            component.footprint
                        ),
                    ))
                    .with_child(section_label(palette, "app.electronics_position"))
                    .with_child(range_row(
                        palette,
                        "app.position",
                        "electronics.pcb.position",
                        [component.position.x, component.position.y],
                        -1000.0,
                        1000.0,
                        0.1,
                    ))
                    .with_child(single_range(
                        palette,
                        "app.rotation",
                        "electronics.pcb.rotation",
                        component.rotation,
                        -360.0,
                        360.0,
                        1.0,
                    ))
                    .with_child(section_label(palette, "app.electronics_appearance"))
                    .with_child(layer_row(palette, "electronics.pcb.layer", component.layer))
                    .with_child(toggle(
                        palette,
                        "app.pcb_locked",
                        "electronics.pcb.locked",
                        component.locked,
                    ))
                    .with_child(section_label(palette, "app.electronics_electrical"));
                for (pad_index, net) in component.pad_nets.iter().enumerate() {
                    content = content.with_child(info_line(
                        palette,
                        format!("{} {}: {}", t("app.pcb_pad", lang), pad_index + 1, net),
                    ));
                }
            }
        }
        PcbSelection::Trace(index) => {
            if let Some(trace) = view.layout.traces.get(index) {
                content = content
                    .with_child(section_label(palette, "app.pcb_trace"))
                    .with_child(info_line(
                        palette,
                        format!("{}: {}", t("app.schematic_net", lang), trace.net),
                    ))
                    .with_child(single_range(
                        palette,
                        "app.pcb_width",
                        "electronics.pcb.trace.width",
                        trace.width,
                        0.01,
                        100.0,
                        0.05,
                    ))
                    .with_child(layer_row(
                        palette,
                        "electronics.pcb.trace.layer",
                        trace.layer,
                    ))
                    .with_child(info_line(
                        palette,
                        format!(
                            "{}: {}",
                            t("app.pcb_segments", lang),
                            trace.points.len().saturating_sub(1)
                        ),
                    ));
            }
        }
        PcbSelection::Airwire(index) => {
            if let Some(airwire) = view.layout.airwires.get(index) {
                content = content
                    .with_child(section_label(palette, "app.pcb_airwire"))
                    .with_child(info_line(
                        palette,
                        format!("{}: {}", t("app.schematic_net", lang), airwire.net),
                    ))
                    .with_child(info_line(
                        palette,
                        format!(
                            "{} -> {}",
                            format_point(airwire.from),
                            format_point(airwire.to)
                        ),
                    ));
            }
        }
        PcbSelection::None => {
            let size = view.layout.board_size();
            content = content
                .with_child(section_label(palette, "app.pcb_board_root"))
                .with_child(info_line(
                    palette,
                    format!(
                        "{} {} | {} {} | {} {}",
                        t("app.pcb_components", lang),
                        view.layout.components.len(),
                        t("app.pcb_traces", lang),
                        view.layout.traces.len(),
                        t("app.pcb_airwires", lang),
                        view.layout.airwires.len()
                    ),
                ))
                .with_child(info_line(
                    palette,
                    format!(
                        "{}: {:.0} x {:.0}",
                        t("app.pcb_board_size", lang),
                        size.x,
                        size.y
                    ),
                ));
        }
    }
    let root = root_with_header(palette, "app.electronics_inspector", content);
    let mut surface = UiSurface::new("electronics-pcb-inspector", palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

fn base_content(palette: StudioUiPalette, id: &str) -> UiNode {
    UiNode::scroll_view(id, UiScrollAxis::Vertical)
        .with_class("electronics-inspector-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 7.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new(format!("{id}.hint"), UiNodeKind::Label)
                .with_text_key("app.electronics_inspector")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
}

fn root_with_header(palette: StudioUiPalette, title_key: &str, content: UiNode) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("electronics.inspector.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("electronics.inspector.header", UiNodeKind::Toolbar)
                .with_class("electronics-inspector-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(12.0, 0.0),
                    ..UiLayout::fixed(0.0, 38.0)
                })
                .with_child(
                    UiNode::new("electronics.inspector.title", UiNodeKind::Label)
                        .with_text_key(title_key)
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                ),
        )
        .with_child(content)
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(
        format!("electronics.inspector.section.{key}"),
        UiNodeKind::Label,
    )
    .with_text_key(key)
    .with_text_style(UiTextStyle::panel_title(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 22.0))
}

fn info_line(palette: StudioUiPalette, value: String) -> UiNode {
    UiNode::new(
        format!("electronics.inspector.info.{}", stable_id(&value)),
        UiNodeKind::Label,
    )
    .with_text_key(value)
    .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 20.0))
}

fn text_field(palette: StudioUiPalette, label_key: &str, key: &str, id: &str) -> UiNode {
    let mut input = UiTextInput::new(key);
    input.max_length = 160;
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fixed(0.0, 52.0)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        )
        .with_child(
            UiNode::text_input(id, input)
                .with_class("electronics-inspector-input")
                .with_layout(UiLayout::fixed(0.0, 30.0)),
        )
}

fn toggle(palette: StudioUiPalette, label_key: &str, key: &str, value: bool) -> UiNode {
    UiNode::toggle(format!("{key}.toggle"), UiToggle::new(key, value))
        .with_class("electronics-inspector-toggle")
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text))
        .with_layout(UiLayout::fixed(0.0, 30.0))
}

fn range_row(
    palette: StudioUiPalette,
    label_key: &str,
    prefix: &str,
    values: [f32; 2],
    min: f32,
    max: f32,
    step: f32,
) -> UiNode {
    let controls = UiNode::new(format!("{prefix}.row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fixed(0.0, 64.0)
        })
        .with_child(
            UiNode::new(format!("{prefix}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        );
    let mut axis_row =
        UiNode::new(format!("{prefix}.axes"), UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fill(UiFlow::Row)
        });
    for (index, axis) in ["x", "y"].into_iter().enumerate() {
        axis_row = axis_row.with_child(
            UiNode::new(format!("{prefix}.{axis}"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    gap: 2.0,
                    ..UiLayout::default()
                })
                .with_child(
                    UiNode::new(format!("{prefix}.{axis}.label"), UiNodeKind::Label)
                        .with_text_key(axis.to_uppercase())
                        .with_text_style(UiTextStyle::body(palette.tokens().text_muted)),
                )
                .with_child(
                    UiNode::range(
                        format!("{prefix}.{axis}.range"),
                        UiRange::new(format!("{prefix}.{axis}"), values[index], min, max, step),
                    )
                    .with_class("electronics-inspector-range")
                    .with_layout(UiLayout::fixed(0.0, 26.0)),
                ),
        );
    }
    controls.with_child(axis_row)
}

fn single_range(
    palette: StudioUiPalette,
    label_key: &str,
    key: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) -> UiNode {
    UiNode::new(format!("{key}.row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fixed(0.0, 48.0)
        })
        .with_child(
            UiNode::new(format!("{key}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted)),
        )
        .with_child(
            UiNode::range(
                format!("{key}.range"),
                UiRange::new(key, value, min, max, step),
            )
            .with_class("electronics-inspector-range")
            .with_layout(UiLayout::fixed(0.0, 26.0)),
        )
}

fn layer_row(palette: StudioUiPalette, prefix: &str, active: PcbLayer) -> UiNode {
    UiNode::new(format!("{prefix}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 5.0,
            ..UiLayout::fixed(0.0, 30.0)
        })
        .with_child(command_button(
            format!("{prefix}.top"),
            "app.pcb_layer_top",
            format!("electronics.layer:{prefix}:top"),
            active == PcbLayer::TopCopper,
            palette,
        ))
        .with_child(command_button(
            format!("{prefix}.bottom"),
            "app.pcb_layer_bottom",
            format!("electronics.layer:{prefix}:bottom"),
            active == PcbLayer::BottomCopper,
            palette,
        ))
}

fn command_button(
    id: String,
    label_key: &str,
    command: String,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "electronics-inspector-button-active"
        } else {
            "electronics-inspector-button"
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [48.0, 26.0],
            padding: UiSpacing::xy(7.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn parse_layer_command(name: &str) -> Option<ElectronicsInspectorAction> {
    let mut parts = name.split(':');
    if parts.next()? != "electronics.layer" {
        return None;
    }
    let field = parts.next()?.to_string();
    let value = match parts.next()? {
        "top" => PcbLayer::TopCopper,
        "bottom" => PcbLayer::BottomCopper,
        _ => return None,
    };
    Some(ElectronicsInspectorAction::Layer { field, value })
}

fn format_point(point: glam::Vec2) -> String {
    format!("({:.1}, {:.1})", point.x, point.y)
}

fn pin_direction_label(
    direction: raf_electronics::component::PinDirection,
    lang: Language,
) -> &'static str {
    match (lang, direction) {
        (Language::Spanish, raf_electronics::component::PinDirection::Input) => "Entrada",
        (Language::Spanish, raf_electronics::component::PinDirection::Output) => "Salida",
        (Language::Spanish, raf_electronics::component::PinDirection::Bidirectional) => {
            "Bidireccional"
        }
        (Language::Spanish, raf_electronics::component::PinDirection::Power) => "Energia",
        (Language::Spanish, raf_electronics::component::PinDirection::Ground) => "Tierra",
        (_, raf_electronics::component::PinDirection::Input) => "Input",
        (_, raf_electronics::component::PinDirection::Output) => "Output",
        (_, raf_electronics::component::PinDirection::Bidirectional) => "Bidirectional",
        (_, raf_electronics::component::PinDirection::Power) => "Power",
        (_, raf_electronics::component::PinDirection::Ground) => "Ground",
    }
}

fn stable_id(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(32)
        .collect()
}

fn inspector_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Root),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-scroll".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-range".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-toggle".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-button-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}
