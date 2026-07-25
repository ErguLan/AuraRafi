//! Retained RafUI toolbars for the Electronics CAD canvases.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiTextStyle,
};

use super::pcb_view::PcbViewPanel;
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::schematic_view::SchematicViewPanel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsToolbarAction {
    SchematicSelect,
    SchematicWire,
    SchematicRotate,
    SchematicFit,
    SchematicLibrary,
    SchematicTest,
    SchematicDelete,
    SchematicZoomIn,
    SchematicZoomOut,
    PcbSelect,
    PcbRoute,
    PcbOutline,
    PcbAirwires,
    PcbFit,
    PcbNewOutline,
    PcbRouteSelected,
    PcbZoomIn,
    PcbZoomOut,
}

pub struct ElectronicsToolbarSurfaceHost {
    schematic_bridge: RafUiSurfaceBridge,
    pcb_bridge: RafUiSurfaceBridge,
}

impl Default for ElectronicsToolbarSurfaceHost {
    fn default() -> Self {
        Self {
            schematic_bridge: RafUiSurfaceBridge::new("raf_ui_schematic_toolbar"),
            pcb_bridge: RafUiSurfaceBridge::new("raf_ui_pcb_toolbar"),
        }
    }
}

impl ElectronicsToolbarSurfaceHost {
    pub fn show_schematic(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &SchematicViewPanel,
        lang: Language,
    ) -> Vec<ElectronicsToolbarAction> {
        let surface = build_schematic_toolbar(palette, view, lang);
        collect(
            self.schematic_bridge
                .show(ui, render_state, palette, surface, |key| t(key, lang)),
            true,
        )
    }

    pub fn show_pcb(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &PcbViewPanel,
        lang: Language,
    ) -> Vec<ElectronicsToolbarAction> {
        let surface = build_pcb_toolbar(palette, view, lang);
        collect(
            self.pcb_bridge
                .show(ui, render_state, palette, surface, |key| t(key, lang)),
            false,
        )
    }
}

fn build_schematic_toolbar(
    palette: StudioUiPalette,
    view: &SchematicViewPanel,
    _lang: Language,
) -> UiSurface {
    let root = UiNode::new("electronics.schematic.toolbar.root", UiNodeKind::Root)
        .with_layout(toolbar_layout())
        .with_style(palette.toolbar_style())
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.select",
            "app.electronics_tool_select",
            "electronics.schematic.select",
            view.select_tool_active(),
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.wire",
            "app.wire_mode",
            "electronics.schematic.wire",
            view.wire_tool_active(),
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.rotate",
            "app.rotate_r",
            "electronics.schematic.rotate",
            false,
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.fit",
            "app.electronics_fit",
            "electronics.schematic.fit",
            false,
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.library",
            "app.electronics_toggle_library",
            "electronics.schematic.library",
            view.library_is_visible(),
            palette,
        ))
        .with_child(separator(palette))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.test",
            "app.electronics_play_test",
            "electronics.schematic.test",
            false,
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.delete",
            "app.delete_del",
            "electronics.schematic.delete",
            false,
            palette,
        ))
        .with_child(
            UiNode::new("electronics.schematic.toolbar.spacer", UiNodeKind::Panel).with_layout(
                UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                },
            ),
        )
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.zoom-out",
            "app.electronics_zoom_out",
            "electronics.schematic.zoom-out",
            false,
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.schematic.toolbar.zoom-in",
            "app.electronics_zoom_in",
            "electronics.schematic.zoom-in",
            false,
            palette,
        ));
    let mut surface = UiSurface::new("electronics-schematic-toolbar", palette, root);
    surface.style_sheet = toolbar_style_sheet(palette);
    surface
}

fn build_pcb_toolbar(palette: StudioUiPalette, view: &PcbViewPanel, _lang: Language) -> UiSurface {
    let root = UiNode::new("electronics.pcb.toolbar.root", UiNodeKind::Root)
        .with_layout(toolbar_layout())
        .with_style(palette.toolbar_style())
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.select",
            "app.pcb_tool_select",
            "electronics.pcb.select",
            !view.route_tool_active() && !view.outline_tool_active(),
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.route",
            "app.pcb_tool_route",
            "electronics.pcb.route",
            view.route_tool_active(),
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.outline",
            "app.pcb_tool_outline",
            "electronics.pcb.outline",
            view.outline_tool_active(),
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.airwires",
            "app.pcb_airwires",
            "electronics.pcb.airwires",
            view.airwires_visible(),
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.fit",
            "app.electronics_fit",
            "electronics.pcb.fit",
            false,
            palette,
        ))
        .with_child(separator(palette))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.new-outline",
            "app.pcb_new_outline",
            "electronics.pcb.new-outline",
            false,
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.route-selected",
            "app.pcb_route_selected",
            "electronics.pcb.route-selected",
            view.selected_airwire_index().is_some(),
            palette,
        ))
        .with_child(
            UiNode::new("electronics.pcb.toolbar.spacer", UiNodeKind::Panel).with_layout(
                UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                },
            ),
        )
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.zoom-out",
            "app.electronics_zoom_out",
            "electronics.pcb.zoom-out",
            false,
            palette,
        ))
        .with_child(toolbar_button(
            "electronics.pcb.toolbar.zoom-in",
            "app.electronics_zoom_in",
            "electronics.pcb.zoom-in",
            false,
            palette,
        ));
    let mut surface = UiSurface::new("electronics-pcb-toolbar", palette, root);
    surface.style_sheet = toolbar_style_sheet(palette);
    surface
}

fn toolbar_layout() -> UiLayout {
    UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 5.0,
        padding: raf_ui::UiSpacing::xy(8.0, 4.0),
        ..UiLayout::fill(UiFlow::Row)
    }
}

fn toolbar_button(
    id: &str,
    label_key: &str,
    command: &str,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "electronics-toolbar-active"
        } else {
            "electronics-toolbar-button"
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [38.0, 28.0],
            padding: raf_ui::UiSpacing::xy(7.0, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn separator(palette: StudioUiPalette) -> UiNode {
    UiNode::new("electronics.toolbar.separator", UiNodeKind::Separator)
        .with_layout(UiLayout::fixed(1.0, 22.0))
        .with_style(raf_ui::UiStyle {
            fill: palette.tokens().border,
            border: palette.tokens().border,
            text: palette.tokens().border,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn collect(
    actions: Vec<raf_ui::UiDispatchedAction>,
    schematic: bool,
) -> Vec<ElectronicsToolbarAction> {
    actions
        .into_iter()
        .filter_map(|dispatched| match dispatched.action {
            UiAction::Command { name } => parse_command(&name, schematic),
            _ => None,
        })
        .collect()
}

fn parse_command(name: &str, schematic: bool) -> Option<ElectronicsToolbarAction> {
    match name {
        "electronics.schematic.select" if schematic => {
            Some(ElectronicsToolbarAction::SchematicSelect)
        }
        "electronics.schematic.wire" if schematic => Some(ElectronicsToolbarAction::SchematicWire),
        "electronics.schematic.rotate" if schematic => {
            Some(ElectronicsToolbarAction::SchematicRotate)
        }
        "electronics.schematic.fit" if schematic => Some(ElectronicsToolbarAction::SchematicFit),
        "electronics.schematic.library" if schematic => {
            Some(ElectronicsToolbarAction::SchematicLibrary)
        }
        "electronics.schematic.test" if schematic => Some(ElectronicsToolbarAction::SchematicTest),
        "electronics.schematic.delete" if schematic => {
            Some(ElectronicsToolbarAction::SchematicDelete)
        }
        "electronics.schematic.zoom-in" if schematic => {
            Some(ElectronicsToolbarAction::SchematicZoomIn)
        }
        "electronics.schematic.zoom-out" if schematic => {
            Some(ElectronicsToolbarAction::SchematicZoomOut)
        }
        "electronics.pcb.select" if !schematic => Some(ElectronicsToolbarAction::PcbSelect),
        "electronics.pcb.route" if !schematic => Some(ElectronicsToolbarAction::PcbRoute),
        "electronics.pcb.outline" if !schematic => Some(ElectronicsToolbarAction::PcbOutline),
        "electronics.pcb.airwires" if !schematic => Some(ElectronicsToolbarAction::PcbAirwires),
        "electronics.pcb.fit" if !schematic => Some(ElectronicsToolbarAction::PcbFit),
        "electronics.pcb.new-outline" if !schematic => {
            Some(ElectronicsToolbarAction::PcbNewOutline)
        }
        "electronics.pcb.route-selected" if !schematic => {
            Some(ElectronicsToolbarAction::PcbRouteSelected)
        }
        "electronics.pcb.zoom-in" if !schematic => Some(ElectronicsToolbarAction::PcbZoomIn),
        "electronics.pcb.zoom-out" if !schematic => Some(ElectronicsToolbarAction::PcbZoomOut),
        _ => None,
    }
}

fn toolbar_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Root),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-toolbar-active".to_string()),
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
                UiStyleSelector::Class("electronics-toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}
