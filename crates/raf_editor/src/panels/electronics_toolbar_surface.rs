//! Retained RafUI toolbars for the Electronics CAD canvases.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiImage, UiImageFit, UiImageSource, UiLayout, UiNode, UiNodeKind, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextStyle,
};
use std::hash::{Hash, Hasher};

use super::pcb_view::PcbViewPanel;
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;
use super::schematic_view::SchematicViewPanel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsToolbarAction {
    SwitchToSchematic,
    SwitchToPcb,
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
    schematic_surface_key: Option<u64>,
    schematic_surface: Option<UiSurface>,
    schematic_surface_revision: u64,
    pcb_surface_key: Option<u64>,
    pcb_surface: Option<UiSurface>,
    pcb_surface_revision: u64,
}

const TOOLBAR_ASSET_PREFIX: &str = "electronics://toolbar/";
const TOOLBAR_ICON_TINT: [u8; 4] = [243, 245, 247, 226];

const TOOLBAR_ASSETS: &[(&str, &[u8])] = &[
    (
        "select.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/select.png"),
    ),
    (
        "wire.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/wire.png"),
    ),
    (
        "rotate.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/rotate.png"),
    ),
    (
        "fit.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/fit.png"),
    ),
    (
        "library.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/library.png"),
    ),
    (
        "play.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/play.png"),
    ),
    (
        "delete.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/delete.png"),
    ),
    (
        "zoom-in.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/zoom-in.png"),
    ),
    (
        "zoom-out.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/zoom-out.png"),
    ),
    (
        "outline.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/outline.png"),
    ),
    (
        "airwire.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/airwire.png"),
    ),
    (
        "layers.png",
        include_bytes!("../../../../editor/assets/electronics/toolbar/layers.png"),
    ),
];

impl Default for ElectronicsToolbarSurfaceHost {
    fn default() -> Self {
        let mut host = Self {
            schematic_bridge: RafUiSurfaceBridge::new("raf_ui_schematic_toolbar"),
            pcb_bridge: RafUiSurfaceBridge::new("raf_ui_pcb_toolbar"),
            schematic_surface_key: None,
            schematic_surface: None,
            schematic_surface_revision: 0,
            pcb_surface_key: None,
            pcb_surface: None,
            pcb_surface_revision: 0,
        };
        register_toolbar_assets(&mut host.schematic_bridge);
        register_toolbar_assets(&mut host.pcb_bridge);
        host
    }
}

fn register_toolbar_assets(bridge: &mut RafUiSurfaceBridge) {
    for (name, bytes) in TOOLBAR_ASSETS {
        let key = format!("{TOOLBAR_ASSET_PREFIX}{name}");
        if let Err(error) = bridge.register_embedded_png(&key, bytes) {
            tracing::warn!(%error, asset = name, "unable to register Electronics toolbar asset");
        }
    }
}

fn toolbar_asset_key(name: &str) -> String {
    format!("{TOOLBAR_ASSET_PREFIX}{name}")
}

impl ElectronicsToolbarSurfaceHost {
    pub fn show_schematic(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        view: &SchematicViewPanel,
        lang: Language,
        compact: bool,
    ) -> Vec<ElectronicsToolbarAction> {
        let key = schematic_toolbar_key(palette, view, lang, compact);
        if self.schematic_surface_key != Some(key) {
            self.schematic_surface_revision =
                self.schematic_surface_revision.wrapping_add(1).max(1);
            self.schematic_surface = Some(build_schematic_toolbar(palette, view, lang, compact));
            self.schematic_surface_key = Some(key);
        }
        let Some(surface) = self.schematic_surface.as_ref() else {
            return Vec::new();
        };
        collect(
            self.schematic_bridge.show_transparent_ref_revision(
                ui,
                render_state,
                palette,
                surface,
                self.schematic_surface_revision,
                |key| t(key, lang),
            ),
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
        compact: bool,
    ) -> Vec<ElectronicsToolbarAction> {
        let key = pcb_toolbar_key(palette, view, lang, compact);
        if self.pcb_surface_key != Some(key) {
            self.pcb_surface_revision = self.pcb_surface_revision.wrapping_add(1).max(1);
            self.pcb_surface = Some(build_pcb_toolbar(palette, view, lang, compact));
            self.pcb_surface_key = Some(key);
        }
        let Some(surface) = self.pcb_surface.as_ref() else {
            return Vec::new();
        };
        collect(
            self.pcb_bridge.show_transparent_ref_revision(
                ui,
                render_state,
                palette,
                surface,
                self.pcb_surface_revision,
                |key| t(key, lang),
            ),
            false,
        )
    }
}

fn schematic_toolbar_key(
    palette: StudioUiPalette,
    view: &SchematicViewPanel,
    lang: Language,
    compact: bool,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    matches!(palette, StudioUiPalette::IndustrialDark).hash(&mut hasher);
    lang.locale_id().hash(&mut hasher);
    view.select_tool_active().hash(&mut hasher);
    view.wire_tool_active().hash(&mut hasher);
    view.library_is_visible().hash(&mut hasher);
    view.schematic.components.len().hash(&mut hasher);
    view.schematic.wires.len().hash(&mut hasher);
    compact.hash(&mut hasher);
    hasher.finish()
}

fn pcb_toolbar_key(
    palette: StudioUiPalette,
    view: &PcbViewPanel,
    lang: Language,
    compact: bool,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    matches!(palette, StudioUiPalette::IndustrialDark).hash(&mut hasher);
    lang.locale_id().hash(&mut hasher);
    view.route_tool_active().hash(&mut hasher);
    view.outline_tool_active().hash(&mut hasher);
    view.airwires_visible().hash(&mut hasher);
    view.selected_airwire_index().hash(&mut hasher);
    view.layout.components.len().hash(&mut hasher);
    view.layout.traces.len().hash(&mut hasher);
    compact.hash(&mut hasher);
    hasher.finish()
}

fn build_schematic_toolbar(
    palette: StudioUiPalette,
    view: &SchematicViewPanel,
    _lang: Language,
    compact: bool,
) -> UiSurface {
    let root = UiNode::new("electronics.schematic.toolbar.root", UiNodeKind::Root)
        .with_layout(toolbar_layout())
        .with_style(palette.toolbar_style())
        .with_child(mode_button(
            "electronics.schematic.mode",
            "app.electronics_schematic_tab",
            "electronics.mode.schematic",
            true,
            compact,
            palette,
        ))
        .with_child(mode_button(
            "electronics.pcb.mode",
            "app.electronics_pcb_tab",
            "electronics.mode.pcb",
            false,
            compact,
            palette,
        ))
        .with_child(separator(palette))
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
            UiNode::new("electronics.schematic.toolbar.spacer", UiNodeKind::Panel)
                .with_style(UiStyle::transparent())
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
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

fn build_pcb_toolbar(
    palette: StudioUiPalette,
    view: &PcbViewPanel,
    _lang: Language,
    compact: bool,
) -> UiSurface {
    let root = UiNode::new("electronics.pcb.toolbar.root", UiNodeKind::Root)
        .with_layout(toolbar_layout())
        .with_style(palette.toolbar_style())
        .with_child(mode_button(
            "electronics.schematic.mode",
            "app.electronics_schematic_tab",
            "electronics.mode.schematic",
            false,
            compact,
            palette,
        ))
        .with_child(mode_button(
            "electronics.pcb.mode",
            "app.electronics_pcb_tab",
            "electronics.mode.pcb",
            true,
            compact,
            palette,
        ))
        .with_child(separator(palette))
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
            UiNode::new("electronics.pcb.toolbar.spacer", UiNodeKind::Panel)
                .with_style(UiStyle::transparent())
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
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
        gap: 4.0,
        padding: raf_ui::UiSpacing::xy(6.0, 3.0),
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
    let asset = toolbar_asset_for_command(command);
    let mut node = if let Some(asset) = asset {
        UiNode::image(
            id,
            UiImage {
                source: UiImageSource::new(toolbar_asset_key(asset)),
                fit: UiImageFit::Contain,
                tint: Some(TOOLBAR_ICON_TINT),
            },
        )
    } else {
        UiNode::new(id, UiNodeKind::Button)
            .with_icon(UiIcon::new(toolbar_icon(command)).with_size(UiIconSize::Toolbar))
    };
    node = node
        .with_class(if active {
            "electronics-toolbar-active"
        } else if command.ends_with("test") {
            "electronics-toolbar-primary"
        } else {
            "electronics-toolbar-button"
        })
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [32.0, 28.0],
            padding: raf_ui::UiSpacing::xy(4.0, 4.0),
            ..UiLayout::default()
        })
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
    node
}

fn toolbar_asset_for_command(command: &str) -> Option<&'static str> {
    match command {
        command if command.ends_with("select") => Some("select.png"),
        command if command.ends_with("wire") || command.ends_with("route") => Some("wire.png"),
        command if command.ends_with("rotate") => Some("rotate.png"),
        command if command.ends_with("fit") => Some("fit.png"),
        command if command.ends_with("library") => Some("library.png"),
        command if command.ends_with("test") => Some("play.png"),
        command if command.ends_with("delete") => Some("delete.png"),
        command if command.ends_with("zoom-in") => Some("zoom-in.png"),
        command if command.ends_with("zoom-out") => Some("zoom-out.png"),
        command if command.ends_with("outline") || command.ends_with("new-outline") => {
            Some("outline.png")
        }
        command if command.ends_with("airwires") => Some("airwire.png"),
        command if command.ends_with("route-selected") => Some("layers.png"),
        _ => None,
    }
}

fn mode_button(
    id: &str,
    label_key: &str,
    command: &str,
    active: bool,
    compact: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let mut node = UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "electronics-mode-active"
        } else {
            "electronics-mode-button"
        })
        .with_icon(UiIcon::new(if command.ends_with("schematic") {
            UiIconId::Schematic
        } else {
            UiIconId::Pcb
        }))
        .with_accessibility_label_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: if compact { [32.0, 28.0] } else { [92.0, 28.0] },
            padding: raf_ui::UiSpacing::xy(if compact { 5.0 } else { 8.0 }, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
    if !compact {
        node = node.with_text_key(label_key);
    }
    node
}

fn toolbar_icon(command: &str) -> UiIconId {
    match command {
        command if command.ends_with("select") => UiIconId::Select,
        command if command.ends_with("wire") => UiIconId::Node,
        command if command.ends_with("rotate") => UiIconId::Rotate,
        command if command.ends_with("fit") => UiIconId::Focus,
        command if command.ends_with("library") => UiIconId::Assets,
        command if command.ends_with("test") => UiIconId::Play,
        command if command.ends_with("delete") => UiIconId::Close,
        command if command.ends_with("zoom-in") => UiIconId::Add,
        command if command.ends_with("zoom-out") => UiIconId::Search,
        command if command.ends_with("route") => UiIconId::Node,
        command if command.ends_with("outline") => UiIconId::View2d,
        command if command.ends_with("airwires") => UiIconId::Grid,
        command if command.ends_with("new-outline") => UiIconId::Add,
        command if command.ends_with("route-selected") => UiIconId::Move,
        _ => UiIconId::Menu,
    }
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
        "electronics.mode.schematic" => Some(ElectronicsToolbarAction::SwitchToSchematic),
        "electronics.mode.pcb" => Some(ElectronicsToolbarAction::SwitchToPcb),
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

#[cfg(test)]
mod tests {
    use super::toolbar_asset_for_command;

    #[test]
    fn electronics_toolbar_uses_dedicated_assets_for_primary_actions() {
        assert_eq!(
            toolbar_asset_for_command("electronics.schematic.wire"),
            Some("wire.png")
        );
        assert_eq!(
            toolbar_asset_for_command("electronics.schematic.zoom-in"),
            Some("zoom-in.png")
        );
        assert_eq!(
            toolbar_asset_for_command("electronics.pcb.route-selected"),
            Some("layers.png")
        );
    }

    #[test]
    fn electronics_toolbar_does_not_fall_back_to_generic_node_icons() {
        assert_ne!(toolbar_asset_for_command("electronics.pcb.route"), None);
        assert_ne!(toolbar_asset_for_command("electronics.pcb.airwires"), None);
    }
}

fn toolbar_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let translucent = |mut color: [u8; 4], alpha: u8| {
        color[3] = alpha;
        color
    };
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Root),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(10.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.surface_raised, 132)),
                    border: Some(translucent(tokens.border, 128)),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-toolbar-active".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.selection, 150)),
                    border: Some(translucent(tokens.accent, 210)),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    text: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-toolbar-primary".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.accent, 168)),
                    border: Some(translucent(tokens.accent_hot, 210)),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.surface, 165)),
                    border: Some(translucent(tokens.border, 165)),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-mode-button".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.surface_raised, 132)),
                    border: Some(translucent(tokens.border, 128)),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-mode-active".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.surface_alt, 150)),
                    border: Some(translucent(tokens.accent, 210)),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    text: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-mode-button".to_string()),
                UiStylePatch {
                    fill: Some(translucent(tokens.surface, 165)),
                    border: Some(translucent(tokens.border, 165)),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}
