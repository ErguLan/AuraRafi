//! Retained RafUI navigation surfaces for Electronics.
//!
//! Selection and placement remain owned by the existing CAD view panels. This
//! surface only presents the navigator/library and emits stable commands.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::Schematic;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle,
};

use super::pcb_view::{PcbSelection, PcbViewPanel};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::schematic_view::{SchematicSelection, SchematicViewPanel};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectronicsNavigatorAction {
    SchematicRoot,
    SchematicComponent(usize),
    SchematicWire(usize),
    PlaceComponent(usize),
    PcbRoot,
    PcbComponent(usize),
    PcbTrace(usize),
    PcbAirwire(usize),
}

pub struct ElectronicsNavigatorSurfaceHost {
    schematic_bridge: RafUiSurfaceBridge,
    pcb_bridge: RafUiSurfaceBridge,
    schematic_search: String,
}

impl Default for ElectronicsNavigatorSurfaceHost {
    fn default() -> Self {
        Self {
            schematic_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_schematic_navigator"),
            pcb_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_pcb_navigator"),
            schematic_search: String::new(),
        }
    }
}

impl ElectronicsNavigatorSurfaceHost {
    pub fn show_schematic(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &SchematicViewPanel,
        lang: Language,
    ) -> Vec<ElectronicsNavigatorAction> {
        let selection = view.selection();
        let surface = build_schematic_surface(
            palette,
            &view.schematic,
            &view.library,
            &self.schematic_search,
            &selection,
            lang,
        );
        let search = self.schematic_search.clone();
        let actions = self.schematic_bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("electronics.schematic.search", search.clone(), 128),
            |key| t(key, lang),
        );
        let mut output = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "electronics.schematic.search" => {
                    self.schematic_search = value;
                }
                UiAction::Command { name } => {
                    if name == "electronics.schematic.root" {
                        output.push(ElectronicsNavigatorAction::SchematicRoot);
                    } else if let Some(index) =
                        command_index(&name, "electronics.schematic.component")
                    {
                        output.push(ElectronicsNavigatorAction::SchematicComponent(index));
                    } else if let Some(index) = command_index(&name, "electronics.schematic.wire") {
                        output.push(ElectronicsNavigatorAction::SchematicWire(index));
                    } else if let Some(index) = command_index(&name, "electronics.library.place") {
                        output.push(ElectronicsNavigatorAction::PlaceComponent(index));
                    }
                }
                _ => {}
            }
        }
        output
    }

    pub fn show_pcb(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &PcbViewPanel,
        lang: Language,
    ) -> Vec<ElectronicsNavigatorAction> {
        let surface = build_pcb_surface(palette, view, lang);
        self.pcb_bridge
            .show(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::Command { name } if name == "electronics.pcb.root" => {
                    Some(ElectronicsNavigatorAction::PcbRoot)
                }
                UiAction::Command { name } => command_index(&name, "electronics.pcb.component")
                    .map(ElectronicsNavigatorAction::PcbComponent)
                    .or_else(|| {
                        command_index(&name, "electronics.pcb.trace")
                            .map(ElectronicsNavigatorAction::PcbTrace)
                    })
                    .or_else(|| {
                        command_index(&name, "electronics.pcb.airwire")
                            .map(ElectronicsNavigatorAction::PcbAirwire)
                    }),
                _ => None,
            })
            .collect()
    }
}

fn build_schematic_surface(
    palette: StudioUiPalette,
    schematic: &Schematic,
    library: &ComponentLibrary,
    search: &str,
    selection: &SchematicSelection,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut search_input = UiTextInput::new("electronics.schematic.search");
    search_input.placeholder_key = Some("app.electronics_search_components".to_string());
    search_input.max_length = 128;

    let mut list = UiNode::scroll_view("electronics.schematic.list", UiScrollAxis::Vertical)
        .with_class("electronics-navigator-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 4.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    list = list.with_child(section_label(palette, "app.schematic_components"));
    for (index, component) in schematic.components.iter().enumerate() {
        let selected = matches!(
            selection,
            SchematicSelection::Component(active) if *active == index
        ) || matches!(
            selection,
            SchematicSelection::MultipleComponents(active) if active.contains(&index)
        );
        list = list.with_child(navigator_button(
            format!("electronics.schematic.component.{index}"),
            format!("{}  {}", component.designator, component.value),
            format!("electronics.schematic.component:{index}"),
            selected,
            palette,
        ));
    }

    if !schematic.wires.is_empty() {
        list = list.with_child(section_label(palette, "app.schematic_wires"));
        for (index, wire) in schematic.wires.iter().enumerate() {
            let selected =
                matches!(selection, SchematicSelection::Wire(active) if *active == index);
            let label = if wire.net.trim().is_empty() {
                format!("{} #{index}", t("app.schematic_wire", lang))
            } else {
                format!("{}  {}", t("app.schematic_wire", lang), wire.net)
            };
            list = list.with_child(navigator_button(
                format!("electronics.schematic.wire.{index}"),
                label,
                format!("electronics.schematic.wire:{index}"),
                selected,
                palette,
            ));
        }
    }

    let query = search.trim().to_lowercase();
    list = list.with_child(section_label(palette, "app.electronics_library"));
    for (index, template) in library.components.iter().enumerate() {
        let matches_query = query.is_empty()
            || template.name.to_lowercase().contains(&query)
            || template.category.to_lowercase().contains(&query)
            || template
                .keywords
                .iter()
                .any(|keyword| keyword.to_lowercase().contains(&query));
        if !matches_query {
            continue;
        }
        list = list.with_child(
            UiNode::new(
                format!("electronics.library.item.{index}"),
                UiNodeKind::Panel,
            )
            .with_class("electronics-library-card")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(8.0, 5.0),
                ..UiLayout::fixed(0.0, 42.0)
            })
            .with_child(
                UiNode::new(
                    format!("electronics.library.item.{index}.text"),
                    UiNodeKind::Label,
                )
                .with_text_key(format!("{} | {}", template.name, template.description))
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
            )
            .with_child(command_button(
                format!("electronics.library.item.{index}.place"),
                "app.add",
                format!("electronics.library.place:{index}"),
                "electronics-library-place",
                palette,
            )),
        );
    }

    let root = UiNode::new("electronics.schematic.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(header(palette, "app.electronics_project"))
        .with_child(
            navigator_button(
                "electronics.schematic.root.row".to_string(),
                t("app.electronics_main_schematic", lang),
                "electronics.schematic.root".to_string(),
                matches!(selection, SchematicSelection::None),
                palette,
            )
            .with_layout(UiLayout::fixed(0.0, 30.0)),
        )
        .with_child(metrics(
            palette,
            &[
                format!(
                    "{}  {}",
                    t("app.schematic_components", lang),
                    schematic.components.len()
                ),
                format!(
                    "{}  {}",
                    t("app.schematic_wires", lang),
                    schematic.wires.len()
                ),
                format!(
                    "{}  {}",
                    t("app.schematic_nets", lang),
                    schematic.netlist().nets.len()
                ),
            ],
        ))
        .with_child(
            UiNode::text_input("electronics.schematic.search.input", search_input)
                .with_class("electronics-navigator-search")
                .with_layout(UiLayout::fixed(0.0, 30.0)),
        )
        .with_child(list);

    let mut surface = UiSurface::new("electronics-schematic-navigator", palette, root);
    surface.style_sheet = navigator_style_sheet(palette);
    surface
}

fn build_pcb_surface(palette: StudioUiPalette, view: &PcbViewPanel, lang: Language) -> UiSurface {
    let mut list = UiNode::scroll_view("electronics.pcb.list", UiScrollAxis::Vertical)
        .with_class("electronics-navigator-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 4.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    list = list.with_child(section_label(palette, "app.pcb_components"));
    for (index, component) in view.layout.components.iter().enumerate() {
        let selected =
            matches!(view.selection(), PcbSelection::Component(active) if active == index);
        list = list.with_child(navigator_button(
            format!("electronics.pcb.component.{index}"),
            format!("{}  {}", component.designator, component.value),
            format!("electronics.pcb.component:{index}"),
            selected,
            palette,
        ));
    }
    if !view.layout.traces.is_empty() {
        list = list.with_child(section_label(palette, "app.pcb_traces"));
        for (index, trace) in view.layout.traces.iter().enumerate() {
            let selected =
                matches!(view.selection(), PcbSelection::Trace(active) if active == index);
            list = list.with_child(navigator_button(
                format!("electronics.pcb.trace.{index}"),
                format!("{}  {}", t("app.pcb_trace", lang), trace.net),
                format!("electronics.pcb.trace:{index}"),
                selected,
                palette,
            ));
        }
    }
    if !view.layout.airwires.is_empty() {
        list = list.with_child(section_label(palette, "app.pcb_airwires"));
        for (index, airwire) in view.layout.airwires.iter().enumerate() {
            let selected =
                matches!(view.selection(), PcbSelection::Airwire(active) if active == index);
            list = list.with_child(navigator_button(
                format!("electronics.pcb.airwire.{index}"),
                format!("{}  {}", t("app.pcb_airwire", lang), airwire.net),
                format!("electronics.pcb.airwire:{index}"),
                selected,
                palette,
            ));
        }
    }
    list = list
        .with_child(section_label(palette, "app.electronics_board"))
        .with_child(detail_line(
            palette,
            "app.pcb_board_size",
            format!(
                "{:.0} x {:.0}",
                view.layout.board_size().x,
                view.layout.board_size().y
            ),
            lang,
        ))
        .with_child(detail_line(
            palette,
            "app.pcb_outline_status",
            if view.layout.outline_is_closed() {
                t("app.pcb_outline_closed", lang)
            } else {
                t("app.pcb_outline_open", lang)
            },
            lang,
        ));

    let root = UiNode::new("electronics.pcb.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(header(palette, "app.electronics_project"))
        .with_child(
            navigator_button(
                "electronics.pcb.root.row".to_string(),
                t("app.electronics_main_pcb", lang),
                "electronics.pcb.root".to_string(),
                matches!(view.selection(), PcbSelection::None),
                palette,
            )
            .with_layout(UiLayout::fixed(0.0, 30.0)),
        )
        .with_child(metrics(
            palette,
            &[
                format!(
                    "{}  {}",
                    t("app.pcb_components", lang),
                    view.layout.components.len()
                ),
                format!(
                    "{}  {}",
                    t("app.pcb_traces", lang),
                    view.layout.traces.len()
                ),
                format!(
                    "{}  {}",
                    t("app.pcb_airwires", lang),
                    view.layout.airwires.len()
                ),
            ],
        ))
        .with_child(list);

    let mut surface = UiSurface::new("electronics-pcb-navigator", palette, root);
    surface.style_sheet = navigator_style_sheet(palette);
    surface
}

fn header(palette: StudioUiPalette, title_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("electronics.navigator.header", UiNodeKind::Toolbar)
        .with_class("electronics-navigator-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(12.0, 0.0),
            ..UiLayout::fixed(0.0, 38.0)
        })
        .with_child(
            UiNode::new("electronics.navigator.header.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
}

fn metrics(palette: StudioUiPalette, values: &[String]) -> UiNode {
    let mut node = UiNode::new("electronics.navigator.metrics", UiNodeKind::Panel)
        .with_class("electronics-navigator-metrics")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::xy(12.0, 6.0),
            ..UiLayout::fixed(0.0, values.len() as f32 * 18.0 + 12.0)
        });
    for (index, value) in values.iter().enumerate() {
        node = node.with_child(
            UiNode::new(
                format!("electronics.navigator.metric.{index}"),
                UiNodeKind::Label,
            )
            .with_text_key(value.clone())
            .with_text_style(UiTextStyle::body(palette.tokens().text_muted)),
        );
    }
    node
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(
        format!("electronics.navigator.section.{key}"),
        UiNodeKind::Label,
    )
    .with_text_key(key)
    .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
    .with_layout(UiLayout {
        padding: UiSpacing::xy(12.0, 5.0),
        ..UiLayout::fixed(0.0, 24.0)
    })
}

fn detail_line(palette: StudioUiPalette, label_key: &str, value: String, lang: Language) -> UiNode {
    UiNode::new(
        format!("electronics.navigator.detail.{label_key}"),
        UiNodeKind::Label,
    )
    .with_text_key(format!("{}: {value}", t(label_key, lang)))
    .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 22.0))
}

fn navigator_button(
    id: String,
    label: String,
    command: String,
    selected: bool,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if selected {
            "electronics-navigator-row-selected"
        } else {
            "electronics-navigator-row"
        })
        .with_text_key(label)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [0.0, 28.0],
            padding: UiSpacing::xy(10.0, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn command_button(
    id: String,
    label_key: &str,
    command: String,
    class: &str,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [46.0, 26.0],
            padding: UiSpacing::xy(6.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn navigator_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
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
                UiStyleSelector::Class("electronics-navigator-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-metrics".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-row-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-library-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-library-place".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-library-place".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}

fn command_index(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix)
        .and_then(|value| value.strip_prefix(':'))
        .and_then(|value| value.parse::<usize>().ok())
}
