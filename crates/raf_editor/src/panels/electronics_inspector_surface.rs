//! Retained RafUI inspectors for the Electronics workspace.
//!
//! The schematic and PCB view panels remain the document owners. These
//! inspectors expose their existing editable fields as typed retained actions.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::session::{ProjectSessionRegistry, SessionId};
use raf_electronics::{PcbLayer, Schematic};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiRange, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle, UiToggle,
};
use std::hash::{Hash, Hasher};
use uuid::Uuid;

use super::pcb_view::{PcbSelection, PcbViewPanel};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::schematic_view::{SchematicSelection, SchematicViewPanel};

#[derive(Debug, Clone, PartialEq)]
pub enum ElectronicsInspectorAction {
    Text { field: String, value: String },
    Range { field: String, value: f32 },
    Toggle { field: String, value: bool },
    Layer { field: String, value: PcbLayer },
    SwitchTab(ElectronicsInspectorTab),
    SessionCreate { name: String },
    SessionOpen(SessionId),
    SessionDuplicate { source: SessionId, name: String },
    SessionRemove(SessionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElectronicsInspectorTab {
    Properties,
    Sessions,
}

pub struct ElectronicsInspectorSurfaceHost {
    schematic_bridge: RafUiSurfaceBridge,
    pcb_bridge: RafUiSurfaceBridge,
    schematic_surface_key: Option<u64>,
    schematic_surface: Option<UiSurface>,
    schematic_surface_revision: u64,
    pcb_surface_key: Option<u64>,
    pcb_surface: Option<UiSurface>,
    pcb_surface_revision: u64,
    tab: ElectronicsInspectorTab,
}

impl Default for ElectronicsInspectorSurfaceHost {
    fn default() -> Self {
        Self {
            schematic_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_schematic_inspector"),
            pcb_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_pcb_inspector"),
            schematic_surface_key: None,
            schematic_surface: None,
            schematic_surface_revision: 0,
            pcb_surface_key: None,
            pcb_surface: None,
            pcb_surface_revision: 0,
            tab: ElectronicsInspectorTab::Properties,
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
        sessions: &ProjectSessionRegistry,
        lang: Language,
    ) -> Vec<ElectronicsInspectorAction> {
        let selected = view.selection();
        let key = inspector_schematic_key(palette, view, &selected, sessions, lang, self.tab);
        if self.schematic_surface_key != Some(key) {
            self.schematic_surface_revision =
                self.schematic_surface_revision.wrapping_add(1).max(1);
            self.schematic_surface = Some(build_schematic_surface(
                palette,
                &view.schematic,
                selected,
                lang,
                sessions,
                self.tab,
            ));
            self.schematic_surface_key = Some(key);
        }
        let Some(surface) = self.schematic_surface.as_ref() else {
            return Vec::new();
        };
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
        let dispatched = self.schematic_bridge.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            self.schematic_surface_revision,
            |controls| {
                controls.set_text("electronics.schematic.reference", reference.clone(), 48);
                controls.set_text("electronics.schematic.value", value.clone(), 128);
                controls.set_text("electronics.schematic.net", net.clone(), 128);
            },
            |key| t(key, lang),
        );
        let session_name = self
            .schematic_bridge
            .with_control_state_read(|controls| {
                controls.text("inspector.session.new_name").to_string()
            })
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        Self::collect(dispatched, sessions, session_name)
    }

    pub fn show_pcb(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &PcbViewPanel,
        sessions: &ProjectSessionRegistry,
        lang: Language,
    ) -> Vec<ElectronicsInspectorAction> {
        let key = inspector_pcb_key(palette, view, sessions, lang, self.tab);
        if self.pcb_surface_key != Some(key) {
            self.pcb_surface_revision = self.pcb_surface_revision.wrapping_add(1).max(1);
            self.pcb_surface = Some(build_pcb_surface(palette, view, lang, sessions, self.tab));
            self.pcb_surface_key = Some(key);
        }
        let Some(surface) = self.pcb_surface.as_ref() else {
            return Vec::new();
        };
        let (reference, value) = match view.selection() {
            PcbSelection::Component(index) => view
                .layout
                .components
                .get(index)
                .map(|component| (component.designator.clone(), component.value.clone()))
                .unwrap_or_default(),
            _ => (String::new(), String::new()),
        };
        let dispatched = self.pcb_bridge.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            self.pcb_surface_revision,
            |controls| {
                controls.set_text("electronics.pcb.reference", reference.clone(), 48);
                controls.set_text("electronics.pcb.value", value.clone(), 128);
            },
            |key| t(key, lang),
        );
        let session_name = self
            .pcb_bridge
            .with_control_state_read(|controls| {
                controls.text("inspector.session.new_name").to_string()
            })
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        Self::collect(dispatched, sessions, session_name)
    }

    fn collect(
        actions: Vec<raf_ui::UiDispatchedAction>,
        sessions: &ProjectSessionRegistry,
        session_name: Option<String>,
    ) -> Vec<ElectronicsInspectorAction> {
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
                UiAction::Command { name } => parse_layer_command(&name)
                    .or_else(|| parse_tab_command(&name))
                    .or_else(|| parse_session_command(&name, sessions, session_name.as_deref())),
                _ => None,
            })
            .collect()
    }

    pub fn set_tab(&mut self, tab: ElectronicsInspectorTab) {
        if self.tab != tab {
            self.tab = tab;
            self.schematic_surface_key = None;
            self.pcb_surface_key = None;
        }
    }
}

fn inspector_schematic_key(
    palette: StudioUiPalette,
    view: &SchematicViewPanel,
    selection: &SchematicSelection,
    sessions: &ProjectSessionRegistry,
    lang: Language,
    tab: ElectronicsInspectorTab,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    matches!(palette, StudioUiPalette::IndustrialDark).hash(&mut hasher);
    lang.locale_id().hash(&mut hasher);
    tab.hash(&mut hasher);
    session_registry_hash(sessions).hash(&mut hasher);
    format!("{selection:?}").hash(&mut hasher);
    view.surface_revision_hint().hash(&mut hasher);
    view.schematic.components.len().hash(&mut hasher);
    view.schematic.wires.len().hash(&mut hasher);
    if let SchematicSelection::Component(index) = selection {
        if let Some(component) = view.schematic.components.get(*index) {
            component.designator.hash(&mut hasher);
            component.value.hash(&mut hasher);
            component.position.x.to_bits().hash(&mut hasher);
            component.position.y.to_bits().hash(&mut hasher);
            component.rotation.to_bits().hash(&mut hasher);
            component.locked.hash(&mut hasher);
        }
    }
    if let SchematicSelection::Wire(index) = selection {
        if let Some(wire) = view.schematic.wires.get(*index) {
            wire.net.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn inspector_pcb_key(
    palette: StudioUiPalette,
    view: &PcbViewPanel,
    sessions: &ProjectSessionRegistry,
    lang: Language,
    tab: ElectronicsInspectorTab,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    matches!(palette, StudioUiPalette::IndustrialDark).hash(&mut hasher);
    lang.locale_id().hash(&mut hasher);
    tab.hash(&mut hasher);
    session_registry_hash(sessions).hash(&mut hasher);
    format!("{:?}", view.selection()).hash(&mut hasher);
    view.surface_revision_hint().hash(&mut hasher);
    view.layout.components.len().hash(&mut hasher);
    view.layout.traces.len().hash(&mut hasher);
    view.layout.airwires.len().hash(&mut hasher);
    if let Some(index) = view.selected_component_index() {
        if let Some(component) = view.layout.components.get(index) {
            component.designator.hash(&mut hasher);
            component.value.hash(&mut hasher);
            component.footprint.hash(&mut hasher);
            component.position.x.to_bits().hash(&mut hasher);
            component.position.y.to_bits().hash(&mut hasher);
            component.rotation.to_bits().hash(&mut hasher);
            component.locked.hash(&mut hasher);
            component.layer.display_name().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn build_schematic_surface(
    palette: StudioUiPalette,
    schematic: &Schematic,
    selection: SchematicSelection,
    lang: Language,
    sessions: &ProjectSessionRegistry,
    tab: ElectronicsInspectorTab,
) -> UiSurface {
    if tab == ElectronicsInspectorTab::Sessions {
        return build_sessions_surface(palette, "electronics-schematic-sessions", sessions);
    }
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
            content = content.with_child(summary_card(
                palette,
                "app.schematic_properties_root",
                format!(
                    "{} {} | {} {} | {} {}",
                    t("app.schematic_components", lang),
                    schematic.components.len(),
                    t("app.schematic_wires", lang),
                    schematic.wires.len(),
                    t("app.schematic_nets", lang),
                    schematic.netlist().nets.len()
                ),
                "app.electronics_inspector_select_hint",
            ));
        }
    }
    let root = root_with_header(palette, "app.electronics_inspector", content, tab);
    let mut surface = UiSurface::new("electronics-schematic-inspector", palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

fn build_pcb_surface(
    palette: StudioUiPalette,
    view: &PcbViewPanel,
    lang: Language,
    sessions: &ProjectSessionRegistry,
    tab: ElectronicsInspectorTab,
) -> UiSurface {
    if tab == ElectronicsInspectorTab::Sessions {
        return build_sessions_surface(palette, "electronics-pcb-sessions", sessions);
    }
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
                .with_child(summary_card(
                    palette,
                    "app.pcb_board_root",
                    format!(
                        "{} {} | {} {} | {} {}",
                        t("app.pcb_components", lang),
                        view.layout.components.len(),
                        t("app.pcb_traces", lang),
                        view.layout.traces.len(),
                        t("app.pcb_airwires", lang),
                        view.layout.airwires.len()
                    ),
                    "app.electronics_inspector_select_hint",
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
    let root = root_with_header(palette, "app.electronics_inspector", content, tab);
    let mut surface = UiSurface::new("electronics-pcb-inspector", palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

fn base_content(_palette: StudioUiPalette, id: &str) -> UiNode {
    UiNode::scroll_view(id, UiScrollAxis::Vertical)
        .with_class("electronics-inspector-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 5.0,
            padding: UiSpacing::xy(10.0, 8.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
}

fn build_sessions_surface(
    palette: StudioUiPalette,
    id: &str,
    sessions: &ProjectSessionRegistry,
) -> UiSurface {
    let content = super::inspector_surface::sessions_content(palette, sessions);
    let root = root_with_header(
        palette,
        "app.electronics_inspector",
        content,
        ElectronicsInspectorTab::Sessions,
    );
    let mut surface = UiSurface::new(id, palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

fn root_with_header(
    palette: StudioUiPalette,
    title_key: &str,
    content: UiNode,
    tab: ElectronicsInspectorTab,
) -> UiNode {
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
                    padding: UiSpacing::xy(10.0, 0.0),
                    ..UiLayout::fixed(0.0, 32.0)
                })
                .with_child(
                    UiNode::new("electronics.inspector.title", UiNodeKind::Label)
                        .with_text_key(title_key)
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(inspector_tab_button(
                    "electronics.inspector.properties-tab",
                    "app.properties",
                    "electronics.inspector.tab:properties",
                    tab == ElectronicsInspectorTab::Properties,
                    palette,
                ))
                .with_child(inspector_tab_button(
                    "electronics.inspector.sessions-tab",
                    "app.sessions",
                    "electronics.inspector.tab:sessions",
                    tab == ElectronicsInspectorTab::Sessions,
                    palette,
                )),
        )
        .with_child(content)
}

fn inspector_tab_button(
    id: &str,
    label_key: &str,
    command: &str,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    command_button(
        id.to_string(),
        label_key,
        command.to_string(),
        active,
        palette,
    )
    .with_layout(UiLayout {
        min_size: [74.0, 26.0],
        padding: UiSpacing::xy(6.0, 3.0),
        ..UiLayout::default()
    })
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(
        format!("electronics.inspector.section.{key}"),
        UiNodeKind::Label,
    )
    .with_text_key(key)
    .with_text_style(UiTextStyle::panel_title(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 20.0))
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

fn summary_card(
    palette: StudioUiPalette,
    title_key: &str,
    value: String,
    hint_key: &str,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("electronics.inspector.summary.{}", stable_id(title_key)),
        UiNodeKind::Panel,
    )
    .with_class("electronics-inspector-summary")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 1.0,
        padding: UiSpacing::xy(9.0, 5.0),
        ..UiLayout::fixed(0.0, 66.0)
    })
    .with_child(
        UiNode::new("electronics.inspector.summary.title", UiNodeKind::Label)
            .with_text_key(title_key)
            .with_text_style(UiTextStyle::panel_title(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 17.0)),
    )
    .with_child(
        UiNode::new("electronics.inspector.summary.value", UiNodeKind::Label)
            .with_text_key(value)
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 17.0)),
    )
    .with_child(
        UiNode::new("electronics.inspector.summary.hint", UiNodeKind::Label)
            .with_text_key(hint_key)
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 17.0)),
    )
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

fn parse_tab_command(name: &str) -> Option<ElectronicsInspectorAction> {
    match name {
        "electronics.inspector.tab:properties" => Some(ElectronicsInspectorAction::SwitchTab(
            ElectronicsInspectorTab::Properties,
        )),
        "electronics.inspector.tab:sessions" => Some(ElectronicsInspectorAction::SwitchTab(
            ElectronicsInspectorTab::Sessions,
        )),
        _ => None,
    }
}

fn parse_session_command(
    name: &str,
    sessions: &ProjectSessionRegistry,
    session_name: Option<&str>,
) -> Option<ElectronicsInspectorAction> {
    if name == "inspector.session.create" {
        let name = session_name
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| next_session_name(sessions));
        return Some(ElectronicsInspectorAction::SessionCreate { name });
    }
    if let Some(value) = name.strip_prefix("inspector.session.create:") {
        return (!value.trim().is_empty()).then(|| ElectronicsInspectorAction::SessionCreate {
            name: value.trim().to_string(),
        });
    }
    if let Some(id) = name
        .strip_prefix("inspector.session.open:")
        .and_then(parse_session_id)
    {
        return Some(ElectronicsInspectorAction::SessionOpen(id));
    }
    if let Some(id) = name
        .strip_prefix("inspector.session.duplicate:")
        .and_then(parse_session_id)
    {
        return Some(ElectronicsInspectorAction::SessionDuplicate {
            source: id,
            name: next_session_name(sessions),
        });
    }
    name.strip_prefix("inspector.session.remove:")
        .and_then(parse_session_id)
        .map(ElectronicsInspectorAction::SessionRemove)
}

fn parse_session_id(value: &str) -> Option<SessionId> {
    Uuid::parse_str(value).ok().map(SessionId)
}

fn next_session_name(registry: &ProjectSessionRegistry) -> String {
    let mut index = registry.sessions.len() + 1;
    loop {
        let candidate = format!("Session_{index}");
        if !registry
            .sessions
            .iter()
            .any(|session| session.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        index += 1;
    }
}

fn session_registry_hash(registry: &ProjectSessionRegistry) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    registry.active_session.hash(&mut hasher);
    for session in &registry.sessions {
        session.id.hash(&mut hasher);
        session.name.hash(&mut hasher);
        format!("{:?}", session.kind).hash(&mut hasher);
    }
    hasher.finish()
}

fn format_point(point: glam::Vec2) -> String {
    format!("({:.1}, {:.1})", point.x, point.y)
}

fn pin_direction_label(
    direction: raf_electronics::component::PinDirection,
    lang: Language,
) -> String {
    let key = match direction {
        raf_electronics::component::PinDirection::Input => "app.electronics_pin_input",
        raf_electronics::component::PinDirection::Output => "app.electronics_pin_output",
        raf_electronics::component::PinDirection::Bidirectional => {
            "app.electronics_pin_bidirectional"
        }
        raf_electronics::component::PinDirection::Power => "app.electronics_pin_power",
        raf_electronics::component::PinDirection::Ground => "app.electronics_pin_ground",
    };
    t(key, lang)
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
                    radius: Some(0.0),
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
                UiStyleSelector::Class("electronics-inspector-summary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-range".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-toggle".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-button-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    text: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-content".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}
