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
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput,
    UiTextStyle, UiTokens,
};
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

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

#[derive(Debug, Clone, Copy)]
enum SchematicNavigatorRow {
    Section(&'static str, bool),
    Library(usize),
    Component(usize),
    Wire(usize),
}

#[derive(Debug, Clone, Copy)]
enum PcbNavigatorRow {
    Section(&'static str),
    Component(usize),
    Trace(usize),
    Airwire(usize),
    BoardSize,
    Outline,
}

const NAVIGATOR_OVERSCAN_ROWS: usize = 8;
const SCHEMATIC_NAVIGATOR_GAP: f32 = 2.0;
const PCB_NAVIGATOR_GAP: f32 = 2.0;

pub struct ElectronicsNavigatorSurfaceHost {
    schematic_bridge: RafUiSurfaceBridge,
    pcb_bridge: RafUiSurfaceBridge,
    schematic_search: String,
    collapsed_sections: BTreeSet<String>,
    schematic_surface_key: Option<u64>,
    schematic_surface: Option<UiSurface>,
    schematic_surface_revision: u64,
    schematic_scroll_offset: f32,
    pcb_surface_key: Option<u64>,
    pcb_surface: Option<UiSurface>,
    pcb_surface_revision: u64,
    pcb_scroll_offset: f32,
}

impl Default for ElectronicsNavigatorSurfaceHost {
    fn default() -> Self {
        Self {
            schematic_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_schematic_navigator"),
            pcb_bridge: RafUiSurfaceBridge::new("raf_ui_electronics_pcb_navigator"),
            schematic_search: String::new(),
            collapsed_sections: [
                "app.electronics_passive",
                "app.electronics_diodes",
                "app.electronics_magnets",
                "app.electronics_power",
                "app.electronics_other",
                "app.schematic_components",
                "app.schematic_wires",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            schematic_surface_key: None,
            schematic_surface: None,
            schematic_surface_revision: 0,
            schematic_scroll_offset: 0.0,
            pcb_surface_key: None,
            pcb_surface: None,
            pcb_surface_revision: 0,
            pcb_scroll_offset: 0.0,
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
        let viewport_height = ui.available_height().max(1.0);
        let key = schematic_surface_key(
            palette,
            view,
            &self.schematic_search,
            &self.collapsed_sections,
            &selection,
            lang,
            self.schematic_scroll_offset,
            viewport_height,
        );
        if self.schematic_surface_key != Some(key) {
            self.schematic_surface_revision =
                self.schematic_surface_revision.wrapping_add(1).max(1);
            self.schematic_surface = Some(build_schematic_surface(
                palette,
                &view.schematic,
                &view.library,
                &self.schematic_search,
                &self.collapsed_sections,
                &selection,
                lang,
                self.schematic_scroll_offset,
                viewport_height,
            ));
            self.schematic_surface_key = Some(key);
        }
        let Some(surface) = self.schematic_surface.as_ref() else {
            return Vec::new();
        };
        let search = self.schematic_search.clone();
        let actions = self.schematic_bridge.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            self.schematic_surface_revision,
            |controls| controls.set_text("electronics.schematic.search", search.clone(), 128),
            |key| t(key, lang),
        );
        let mut output = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "electronics.schematic.search" => {
                    self.schematic_search = value;
                    self.schematic_surface_key = None;
                }
                UiAction::Command { name } => {
                    if let Some(section) = name.strip_prefix("electronics.navigator.section:") {
                        if !self.collapsed_sections.remove(section) {
                            self.collapsed_sections.insert(section.to_string());
                        }
                        self.schematic_surface_key = None;
                    } else if name == "electronics.schematic.root" {
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
        self.schematic_scroll_offset = self
            .schematic_bridge
            .with_control_state_read(|controls| {
                controls.scroll_offset("electronics.schematic.list")[1]
            })
            .unwrap_or(self.schematic_scroll_offset)
            .max(0.0);
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
        let viewport_height = ui.available_height().max(1.0);
        let key = pcb_surface_key(palette, view, lang, self.pcb_scroll_offset, viewport_height);
        if self.pcb_surface_key != Some(key) {
            self.pcb_surface_revision = self.pcb_surface_revision.wrapping_add(1).max(1);
            self.pcb_surface = Some(build_pcb_surface(
                palette,
                view,
                lang,
                self.pcb_scroll_offset,
                viewport_height,
            ));
            self.pcb_surface_key = Some(key);
        }
        let Some(surface) = self.pcb_surface.as_ref() else {
            return Vec::new();
        };
        let actions = self.pcb_bridge.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            self.pcb_surface_revision,
            |_| {},
            |key| t(key, lang),
        );
        self.pcb_scroll_offset = self
            .pcb_bridge
            .with_control_state_read(|controls| controls.scroll_offset("electronics.pcb.list")[1])
            .unwrap_or(self.pcb_scroll_offset)
            .max(0.0);
        actions
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

fn schematic_surface_key(
    palette: StudioUiPalette,
    view: &SchematicViewPanel,
    search: &str,
    collapsed_sections: &BTreeSet<String>,
    selection: &SchematicSelection,
    lang: Language,
    scroll_offset: f32,
    viewport_height: f32,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    matches!(palette, StudioUiPalette::IndustrialDark).hash(&mut hasher);
    lang.locale_id().hash(&mut hasher);
    search.hash(&mut hasher);
    collapsed_sections.hash(&mut hasher);
    format!("{selection:?}").hash(&mut hasher);
    scroll_offset.to_bits().hash(&mut hasher);
    viewport_height.round().to_bits().hash(&mut hasher);
    view.schematic.components.len().hash(&mut hasher);
    view.schematic.wires.len().hash(&mut hasher);
    view.surface_revision_hint().hash(&mut hasher);
    view.library.components.len().hash(&mut hasher);
    hasher.finish()
}

fn pcb_surface_key(
    palette: StudioUiPalette,
    view: &PcbViewPanel,
    lang: Language,
    scroll_offset: f32,
    viewport_height: f32,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    matches!(palette, StudioUiPalette::IndustrialDark).hash(&mut hasher);
    lang.locale_id().hash(&mut hasher);
    scroll_offset.to_bits().hash(&mut hasher);
    viewport_height.round().to_bits().hash(&mut hasher);
    format!("{:?}", view.selection()).hash(&mut hasher);
    view.layout.components.len().hash(&mut hasher);
    view.layout.traces.len().hash(&mut hasher);
    view.layout.airwires.len().hash(&mut hasher);
    view.surface_revision_hint().hash(&mut hasher);
    hasher.finish()
}

fn build_schematic_surface(
    palette: StudioUiPalette,
    schematic: &Schematic,
    library: &ComponentLibrary,
    search: &str,
    collapsed_sections: &BTreeSet<String>,
    selection: &SchematicSelection,
    lang: Language,
    scroll_offset: f32,
    viewport_height: f32,
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
            gap: SCHEMATIC_NAVIGATOR_GAP,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    let query = search.trim().to_lowercase();
    let matches_query = |values: &[&str]| {
        query.is_empty()
            || values
                .iter()
                .any(|value| value.to_lowercase().contains(&query))
    };

    let visible_library = |template: &raf_electronics::library::ComponentTemplate| {
        matches_query(&[
            template.name.as_str(),
            template.description.as_str(),
            template.category.as_str(),
        ]) || template
            .keywords
            .iter()
            .any(|keyword| keyword.to_lowercase().contains(&query))
    };

    let mut rows = Vec::new();
    let favorites_key = "app.electronics_favorites";
    let favorites_expanded = !collapsed_sections.contains(favorites_key) || !query.is_empty();
    rows.push(SchematicNavigatorRow::Section(
        favorites_key,
        favorites_expanded,
    ));
    if favorites_expanded {
        for (index, template) in library.components.iter().enumerate() {
            if template.favorite && visible_library(template) {
                rows.push(SchematicNavigatorRow::Library(index));
            }
        }
    }

    for (category, label_key) in [
        ("Passive", "app.electronics_passive"),
        ("Diode", "app.electronics_diodes"),
        ("Magnet", "app.electronics_magnets"),
        ("Power", "app.electronics_power"),
    ] {
        let mut category_has_items = false;
        for template in &library.components {
            if template.category == category && visible_library(template) {
                category_has_items = true;
                break;
            }
        }
        if !category_has_items {
            continue;
        }
        let expanded = !collapsed_sections.contains(label_key) || !query.is_empty();
        rows.push(SchematicNavigatorRow::Section(label_key, expanded));
        if expanded {
            for (index, template) in library.components.iter().enumerate() {
                if template.category == category && visible_library(template) {
                    rows.push(SchematicNavigatorRow::Library(index));
                }
            }
        }
    }

    let known_categories = ["Passive", "Diode", "Magnet", "Power"];
    let mut other_categories = Vec::new();
    for template in &library.components {
        if !known_categories.contains(&template.category.as_str())
            && visible_library(template)
            && !other_categories.contains(&template.category.as_str())
        {
            other_categories.push(template.category.as_str());
        }
    }
    if !other_categories.is_empty() {
        let other_key = "app.electronics_other";
        let expanded = !collapsed_sections.contains(other_key) || !query.is_empty();
        rows.push(SchematicNavigatorRow::Section(other_key, expanded));
        if expanded {
            for (index, template) in library.components.iter().enumerate() {
                if !known_categories.contains(&template.category.as_str())
                    && visible_library(template)
                {
                    rows.push(SchematicNavigatorRow::Library(index));
                }
            }
        }
    }

    let components_key = "app.schematic_components";
    let components_expanded = !collapsed_sections.contains(components_key) || !query.is_empty();
    rows.push(SchematicNavigatorRow::Section(
        components_key,
        components_expanded,
    ));
    if components_expanded {
        for (index, component) in schematic.components.iter().enumerate() {
            if !matches_query(&[component.designator.as_str(), component.value.as_str()]) {
                continue;
            }
            rows.push(SchematicNavigatorRow::Component(index));
        }
    }

    if !schematic.wires.is_empty() {
        let wires_key = "app.schematic_wires";
        let wires_expanded = !collapsed_sections.contains(wires_key) || !query.is_empty();
        rows.push(SchematicNavigatorRow::Section(wires_key, wires_expanded));
        if wires_expanded {
            for (index, wire) in schematic.wires.iter().enumerate() {
                if !matches_query(&[wire.net.as_str(), t("app.schematic_wire", lang).as_str()]) {
                    continue;
                }
                rows.push(SchematicNavigatorRow::Wire(index));
            }
        }
    }

    let (visible_start, visible_end) = navigator_visible_range(
        &rows,
        scroll_offset,
        viewport_height,
        NAVIGATOR_OVERSCAN_ROWS,
        SCHEMATIC_NAVIGATOR_GAP,
    );
    if visible_start > 0 {
        list = list.with_child(navigator_spacer(
            "electronics.schematic.list.top-spacer",
            navigator_rows_height(&rows[..visible_start], SCHEMATIC_NAVIGATOR_GAP),
        ));
    }
    for row in rows[visible_start..visible_end].iter().copied() {
        let node = match row {
            SchematicNavigatorRow::Section(label_key, expanded) => {
                section_button(palette, label_key, expanded)
            }
            SchematicNavigatorRow::Library(index) => {
                library_card(palette, &library.components[index], index, tokens)
            }
            SchematicNavigatorRow::Component(index) => {
                let component = &schematic.components[index];
                let selected = matches!(
                    selection,
                    SchematicSelection::Component(active) if *active == index
                ) || matches!(
                    selection,
                    SchematicSelection::MultipleComponents(active) if active.contains(&index)
                );
                navigator_button(
                    format!("electronics.schematic.component.{index}"),
                    format!("{}  {}", component.designator, component.value),
                    format!("electronics.schematic.component:{index}"),
                    selected,
                    palette,
                )
            }
            SchematicNavigatorRow::Wire(index) => {
                let wire = &schematic.wires[index];
                let selected =
                    matches!(selection, SchematicSelection::Wire(active) if *active == index);
                let label = if wire.net.trim().is_empty() {
                    format!("{} #{index}", t("app.schematic_wire", lang))
                } else {
                    format!("{}  {}", t("app.schematic_wire", lang), wire.net)
                };
                navigator_button(
                    format!("electronics.schematic.wire.{index}"),
                    label,
                    format!("electronics.schematic.wire:{index}"),
                    selected,
                    palette,
                )
            }
        };
        list = list.with_child(node);
    }
    if visible_end < rows.len() {
        list = list.with_child(navigator_spacer(
            "electronics.schematic.list.bottom-spacer",
            navigator_rows_height(&rows[visible_end..], SCHEMATIC_NAVIGATOR_GAP),
        ));
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
            .with_layout(UiLayout::fixed(0.0, 28.0)),
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
                .with_layout(UiLayout::fixed(0.0, 28.0)),
        )
        .with_child(list);

    let mut surface = UiSurface::new("electronics-schematic-navigator", palette, root);
    surface.style_sheet = navigator_style_sheet(palette);
    surface
}

fn schematic_navigator_row_height(row: SchematicNavigatorRow) -> f32 {
    match row {
        SchematicNavigatorRow::Section(_, _) => 22.0,
        SchematicNavigatorRow::Library(_) => 38.0,
        SchematicNavigatorRow::Component(_) | SchematicNavigatorRow::Wire(_) => 28.0,
    }
}

fn navigator_rows_height(rows: &[SchematicNavigatorRow], gap: f32) -> f32 {
    let row_height = rows
        .iter()
        .copied()
        .map(schematic_navigator_row_height)
        .sum::<f32>();
    row_height + gap * rows.len().saturating_sub(1) as f32
}

fn navigator_visible_range(
    rows: &[SchematicNavigatorRow],
    scroll_offset: f32,
    viewport_height: f32,
    overscan_rows: usize,
    gap: f32,
) -> (usize, usize) {
    if rows.is_empty() {
        return (0, 0);
    }

    let target_start = scroll_offset.max(0.0);
    let target_end = target_start + viewport_height.max(1.0);
    let mut y = 0.0;
    let mut first = 0;
    while first < rows.len() {
        let next_y = y + schematic_navigator_row_height(rows[first]);
        if next_y > target_start {
            break;
        }
        y = next_y + gap;
        first += 1;
    }

    let start = first.saturating_sub(overscan_rows);
    let mut end = first;
    while end < rows.len() && y < target_end {
        y += schematic_navigator_row_height(rows[end]) + gap;
        end += 1;
    }
    (start, (end + overscan_rows).min(rows.len()))
}

fn navigator_spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

fn pcb_navigator_row_height(row: PcbNavigatorRow) -> f32 {
    match row {
        PcbNavigatorRow::Section(_) => 20.0,
        PcbNavigatorRow::Component(_) | PcbNavigatorRow::Trace(_) | PcbNavigatorRow::Airwire(_) => {
            28.0
        }
        PcbNavigatorRow::BoardSize | PcbNavigatorRow::Outline => 20.0,
    }
}

fn pcb_rows_height(rows: &[PcbNavigatorRow], gap: f32) -> f32 {
    let row_height = rows
        .iter()
        .copied()
        .map(pcb_navigator_row_height)
        .sum::<f32>();
    row_height + gap * rows.len().saturating_sub(1) as f32
}

fn pcb_visible_range(
    rows: &[PcbNavigatorRow],
    scroll_offset: f32,
    viewport_height: f32,
    overscan_rows: usize,
    gap: f32,
) -> (usize, usize) {
    if rows.is_empty() {
        return (0, 0);
    }
    let target_start = scroll_offset.max(0.0);
    let target_end = target_start + viewport_height.max(1.0);
    let mut y = 0.0;
    let mut first = 0;
    while first < rows.len() {
        let next_y = y + pcb_navigator_row_height(rows[first]);
        if next_y > target_start {
            break;
        }
        y = next_y + gap;
        first += 1;
    }
    let start = first.saturating_sub(overscan_rows);
    let mut end = first;
    while end < rows.len() && y < target_end {
        y += pcb_navigator_row_height(rows[end]) + gap;
        end += 1;
    }
    (start, (end + overscan_rows).min(rows.len()))
}

fn build_pcb_surface(
    palette: StudioUiPalette,
    view: &PcbViewPanel,
    lang: Language,
    scroll_offset: f32,
    viewport_height: f32,
) -> UiSurface {
    let mut list = UiNode::scroll_view("electronics.pcb.list", UiScrollAxis::Vertical)
        .with_class("electronics-navigator-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: PCB_NAVIGATOR_GAP,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    let mut rows = vec![PcbNavigatorRow::Section("app.pcb_components")];
    for index in 0..view.layout.components.len() {
        rows.push(PcbNavigatorRow::Component(index));
    }
    if !view.layout.traces.is_empty() {
        rows.push(PcbNavigatorRow::Section("app.pcb_traces"));
        rows.extend((0..view.layout.traces.len()).map(PcbNavigatorRow::Trace));
    }
    if !view.layout.airwires.is_empty() {
        rows.push(PcbNavigatorRow::Section("app.pcb_airwires"));
        rows.extend((0..view.layout.airwires.len()).map(PcbNavigatorRow::Airwire));
    }
    rows.extend([
        PcbNavigatorRow::Section("app.electronics_board"),
        PcbNavigatorRow::BoardSize,
        PcbNavigatorRow::Outline,
    ]);
    let (visible_start, visible_end) = pcb_visible_range(
        &rows,
        scroll_offset,
        viewport_height,
        NAVIGATOR_OVERSCAN_ROWS,
        PCB_NAVIGATOR_GAP,
    );
    if visible_start > 0 {
        list = list.with_child(navigator_spacer(
            "electronics.pcb.list.top-spacer",
            pcb_rows_height(&rows[..visible_start], PCB_NAVIGATOR_GAP),
        ));
    }
    for row in rows[visible_start..visible_end].iter().copied() {
        let node = match row {
            PcbNavigatorRow::Section(key) => section_label(palette, key),
            PcbNavigatorRow::Component(index) => {
                let component = &view.layout.components[index];
                navigator_button(
                    format!("electronics.pcb.component.{index}"),
                    format!("{}  {}", component.designator, component.value),
                    format!("electronics.pcb.component:{index}"),
                    matches!(view.selection(), PcbSelection::Component(active) if active == index),
                    palette,
                )
            }
            PcbNavigatorRow::Trace(index) => {
                let trace = &view.layout.traces[index];
                navigator_button(
                    format!("electronics.pcb.trace.{index}"),
                    format!("{}  {}", t("app.pcb_trace", lang), trace.net),
                    format!("electronics.pcb.trace:{index}"),
                    matches!(view.selection(), PcbSelection::Trace(active) if active == index),
                    palette,
                )
            }
            PcbNavigatorRow::Airwire(index) => {
                let airwire = &view.layout.airwires[index];
                navigator_button(
                    format!("electronics.pcb.airwire.{index}"),
                    format!("{}  {}", t("app.pcb_airwire", lang), airwire.net),
                    format!("electronics.pcb.airwire:{index}"),
                    matches!(view.selection(), PcbSelection::Airwire(active) if active == index),
                    palette,
                )
            }
            PcbNavigatorRow::BoardSize => detail_line(
                palette,
                "app.pcb_board_size",
                format!(
                    "{:.0} x {:.0}",
                    view.layout.board_size().x,
                    view.layout.board_size().y
                ),
                lang,
            ),
            PcbNavigatorRow::Outline => detail_line(
                palette,
                "app.pcb_outline_status",
                if view.layout.outline_is_closed() {
                    t("app.pcb_outline_closed", lang)
                } else {
                    t("app.pcb_outline_open", lang)
                },
                lang,
            ),
        };
        list = list.with_child(node);
    }
    if visible_end < rows.len() {
        list = list.with_child(navigator_spacer(
            "electronics.pcb.list.bottom-spacer",
            pcb_rows_height(&rows[visible_end..], PCB_NAVIGATOR_GAP),
        ));
    }

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
            .with_layout(UiLayout::fixed(0.0, 28.0)),
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
            padding: UiSpacing::xy(10.0, 0.0),
            ..UiLayout::fixed(0.0, 32.0)
        })
        .with_child(
            UiNode::new("electronics.navigator.header.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
}

fn metrics(palette: StudioUiPalette, values: &[String]) -> UiNode {
    UiNode::new("electronics.navigator.metrics", UiNodeKind::Panel)
        .with_class("electronics-navigator-metrics")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(10.0, 3.0),
            ..UiLayout::fixed(0.0, 26.0)
        })
        .with_child(
            UiNode::new("electronics.navigator.metric.summary", UiNodeKind::Label)
                .with_text_value(values.join("  |  "))
                .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted)),
        )
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(
        format!("electronics.navigator.section.{key}"),
        UiNodeKind::Label,
    )
    .with_text_key(key)
    .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
    .with_layout(UiLayout {
        padding: UiSpacing::xy(10.0, 2.0),
        ..UiLayout::fixed(0.0, 20.0)
    })
}

fn section_button(palette: StudioUiPalette, key: &str, expanded: bool) -> UiNode {
    UiNode::new(
        format!("electronics.navigator.section-toggle.{key}"),
        UiNodeKind::Button,
    )
    .with_class("electronics-navigator-section")
    .with_text_key(key)
    .with_icon(
        UiIcon::new(if expanded {
            UiIconId::ChevronDown
        } else {
            UiIconId::ChevronRight
        })
        .with_size(UiIconSize::Small),
    )
    .with_text_style(UiTextStyle::button(palette.tokens().text_muted))
    .with_layout(UiLayout {
        padding: UiSpacing::xy(8.0, 2.0),
        ..UiLayout::fixed(0.0, 22.0)
    })
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("electronics.navigator.section:{key}"),
    ))
}

fn detail_line(palette: StudioUiPalette, label_key: &str, value: String, lang: Language) -> UiNode {
    UiNode::new(
        format!("electronics.navigator.detail.{label_key}"),
        UiNodeKind::Label,
    )
    .with_text_key(format!("{}: {value}", t(label_key, lang)))
    .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 20.0))
}

fn library_card(
    palette: StudioUiPalette,
    template: &raf_electronics::library::ComponentTemplate,
    index: usize,
    tokens: UiTokens,
) -> UiNode {
    UiNode::new(
        format!("electronics.library.item.{index}"),
        UiNodeKind::Panel,
    )
    .with_class("electronics-library-card")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        padding: UiSpacing::xy(8.0, 2.0),
        ..UiLayout::fixed(0.0, 38.0)
    })
    .with_child(
        UiNode::new(
            format!("electronics.library.item.{index}.text"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 0.0,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(
                format!("electronics.library.item.{index}.name"),
                UiNodeKind::Label,
            )
            .with_text_key(template.name.clone())
            .with_icon(
                UiIcon::new(library_icon(template.category.as_str())).with_size(UiIconSize::Small),
            )
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 17.0)),
        )
        .with_child(
            UiNode::new(
                format!("electronics.library.item.{index}.description"),
                UiNodeKind::Label,
            )
            .with_text_key(template.description.clone())
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 14.0)),
        ),
    )
    .with_child(command_button(
        format!("electronics.library.item.{index}.place"),
        format!("electronics.library.place:{index}"),
        "electronics-library-place",
        palette,
    ))
}

fn library_icon(category: &str) -> UiIconId {
    match category {
        "Passive" => UiIconId::Schematic,
        "Diode" => UiIconId::Schematic,
        "Magnet" => UiIconId::Node,
        "Power" => UiIconId::Success,
        _ => UiIconId::Node,
    }
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
        .with_icon(UiIcon::new(navigator_icon(&command)).with_size(UiIconSize::Small))
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [0.0, 28.0],
            padding: UiSpacing::xy(9.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn command_button(id: String, command: String, class: &str, palette: StudioUiPalette) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [28.0, 26.0],
            padding: UiSpacing::xy(5.0, 3.0),
            ..UiLayout::default()
        })
        .with_tooltip_key("app.add")
        .with_accessibility_label_key("app.add")
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn navigator_icon(command: &str) -> UiIconId {
    match command {
        "electronics.schematic.root" | "electronics.pcb.root" => UiIconId::Folder,
        command if command.starts_with("electronics.schematic.component") => UiIconId::Node,
        command if command.starts_with("electronics.schematic.wire") => UiIconId::Node,
        command if command.starts_with("electronics.pcb.component") => UiIconId::Pcb,
        command if command.starts_with("electronics.pcb.trace") => UiIconId::Node,
        command if command.starts_with("electronics.pcb.airwire") => UiIconId::Grid,
        _ => UiIconId::Folder,
    }
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
                UiStyleSelector::Class("electronics-navigator-section".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    border: Some(tokens.border),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-row-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-library-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-library-place".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-navigator-section".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-library-place".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    text: Some(tokens.accent_hot),
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
