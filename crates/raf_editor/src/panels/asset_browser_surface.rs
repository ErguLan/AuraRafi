//! Retained RafUI asset browser.
//!
//! Filesystem scans, imports, script creation, and IDE handoff remain owned by
//! `AssetBrowserPanel`; this host only renders the browser and emits commands.

use eframe::{egui, egui_wgpu};
use raf_assets::importer::AssetType;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::Primitive;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle,
};

use super::asset_browser::AssetEntry;
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetBrowserAction {
    SetSearch(String),
    SetFilter(Option<AssetType>),
    OpenFolder,
    Refresh,
    CreateScript(String),
    AddPrimitive(Primitive),
    OpenScript(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetDialogAction {
    OpenYoll,
    OpenVscode,
    Cancel,
}

pub struct AssetBrowserSurfaceHost {
    bridge: RafUiSurfaceBridge,
    dialog_bridge: RafUiSurfaceBridge,
}

impl Default for AssetBrowserSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_asset_browser"),
            dialog_bridge: RafUiSurfaceBridge::new("raf_ui_asset_ide_dialog"),
        }
    }
}

impl AssetBrowserSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        entries: &[AssetEntry],
        search: &str,
        filter: Option<AssetType>,
        scanning: bool,
        status: Option<&str>,
        lang: Language,
    ) -> Vec<AssetBrowserAction> {
        let surface = build_asset_surface(palette, entries, search, filter, scanning, status, lang);
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("assets.search", search.to_string(), 160),
            |key| t(key, lang),
        );
        actions
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::SetText { key, value } if key == "assets.search" => {
                    Some(AssetBrowserAction::SetSearch(value))
                }
                UiAction::Command { name } => parse_asset_command(&name),
                _ => None,
            })
            .collect()
    }

    pub fn show_ide_dialog(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        file: &str,
        lang: Language,
    ) -> Option<AssetDialogAction> {
        let surface = build_ide_dialog_surface(palette, file);
        self.dialog_bridge
            .show(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .find_map(|dispatched| match dispatched.action {
                UiAction::Command { name } if name == "assets.ide.yoll" => {
                    Some(AssetDialogAction::OpenYoll)
                }
                UiAction::Command { name } if name == "assets.ide.vscode" => {
                    Some(AssetDialogAction::OpenVscode)
                }
                UiAction::Command { name } if name == "assets.ide.cancel" => {
                    Some(AssetDialogAction::Cancel)
                }
                _ => None,
            })
    }
}

fn build_asset_surface(
    palette: StudioUiPalette,
    entries: &[AssetEntry],
    search: &str,
    filter: Option<AssetType>,
    scanning: bool,
    status: Option<&str>,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut search_input = UiTextInput::new("assets.search");
    search_input.placeholder_key = Some("app.search".to_string());
    search_input.max_length = 160;

    let filters = [
        ("app.all", None, "assets.filter.all"),
        ("app.images", Some(AssetType::Image), "assets.filter.images"),
        (
            "app.models",
            Some(AssetType::Model3D),
            "assets.filter.models",
        ),
        ("app.audio", Some(AssetType::Audio), "assets.filter.audio"),
        (
            "app.scripts_filter",
            Some(AssetType::Scene),
            "assets.filter.scripts",
        ),
    ];
    let mut filter_row = UiNode::new("assets.filters", UiNodeKind::Toolbar).with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 4.0,
        ..UiLayout::fixed(0.0, 30.0)
    });
    for (index, (label_key, value, command)) in filters.into_iter().enumerate() {
        filter_row = filter_row.with_child(command_button(
            format!("assets.filter.{index}"),
            label_key,
            command.to_string(),
            if filter == value {
                "assets-filter-active"
            } else {
                "assets-filter"
            },
            palette,
        ));
    }

    let mut list = UiNode::scroll_view("assets.list", UiScrollAxis::Vertical)
        .with_class("assets-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 6.0,
            padding: UiSpacing::same(10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    let query = search.trim().to_lowercase();
    let mut visible = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if filter.is_some_and(|active| active != entry.asset_type) {
            continue;
        }
        if !query.is_empty() && !entry.name.to_lowercase().contains(&query) {
            continue;
        }
        visible += 1;
        list = list.with_child(asset_card(palette, entry, index, lang));
    }
    if visible == 0 {
        list = list.with_child(
            UiNode::new("assets.empty", UiNodeKind::Panel)
                .with_class("assets-empty")
                .with_layout(UiLayout {
                    padding: UiSpacing::same(16.0),
                    ..UiLayout::fixed(0.0, 96.0)
                })
                .with_child(
                    UiNode::new("assets.empty.title", UiNodeKind::Label)
                        .with_text_key("app.no_assets")
                        .with_text_style(UiTextStyle::body(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 24.0)),
                )
                .with_child(
                    UiNode::new("assets.empty.hint", UiNodeKind::Label)
                        .with_text_key("app.drag_drop_hint")
                        .with_text_style(UiTextStyle::body(tokens.text_muted)),
                ),
        );
    }

    let mut actions = UiNode::new("assets.actions", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            ..UiLayout::fixed(0.0, 32.0)
        })
        .with_child(command_button(
            "assets.open-folder".to_string(),
            "app.open_folder",
            "assets.open-folder".to_string(),
            "assets-action",
            palette,
        ))
        .with_child(command_button(
            "assets.refresh".to_string(),
            "app.refresh_assets",
            "assets.refresh".to_string(),
            "assets-action",
            palette,
        ))
        .with_child(command_button(
            "assets.create-script".to_string(),
            "app.create_script",
            "assets.create-script".to_string(),
            "assets-action",
            palette,
        ));
    for (index, (label_key, kind)) in [
        ("app.script_language_rust", "rust"),
        ("app.script_language_cpp", "cpp"),
        ("app.script_language_rhai", "rhai"),
    ]
    .into_iter()
    .enumerate()
    {
        actions = actions.with_child(command_button(
            format!("assets.script-kind.{index}"),
            label_key,
            format!("assets.create-script:{kind}"),
            "assets-action-subtle",
            palette,
        ));
    }

    let mut primitives =
        UiNode::new("assets.primitives", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 30.0)
        });
    for (index, primitive) in [
        Primitive::Cube,
        Primitive::Sphere,
        Primitive::Plane,
        Primitive::Cylinder,
    ]
    .into_iter()
    .enumerate()
    {
        primitives = primitives.with_child(command_button(
            format!("assets.primitive.{index}"),
            primitive.label(),
            format!("assets.add-primitive:{}", primitive.label()),
            "assets-primitive",
            palette,
        ));
    }

    let mut status_row = UiNode::new("assets.status", UiNodeKind::Label)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout::fixed(0.0, 22.0));
    if scanning {
        status_row = status_row.with_text_key("app.scanning_assets");
    } else if let Some(status) = status {
        status_row = status_row.with_text_key(status.to_string());
    } else {
        status_row = status_row.with_text_key(format!("{}: {}", t("app.assets", lang), visible));
    }

    let root = UiNode::new("assets.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(8.0),
            gap: 5.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("assets.header", UiNodeKind::Toolbar)
                .with_class("assets-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 34.0)
                })
                .with_child(
                    UiNode::new("assets.title", UiNodeKind::Label)
                        .with_text_key("app.assets")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(96.0, 22.0)),
                )
                .with_child(
                    UiNode::text_input("assets.search.input", search_input)
                        .with_class("assets-search")
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(0.0, 30.0)
                        }),
                )
                .with_child(filter_row),
        )
        .with_child(actions)
        .with_child(primitives)
        .with_child(status_row)
        .with_child(list);

    let mut surface = UiSurface::new("asset-browser", palette, root);
    surface.style_sheet = asset_style_sheet(palette);
    surface
}

fn asset_card(
    palette: StudioUiPalette,
    entry: &AssetEntry,
    index: usize,
    lang: Language,
) -> UiNode {
    let tokens = palette.tokens();
    let mut card = UiNode::new(format!("assets.card.{index}"), UiNodeKind::Panel)
        .with_class("assets-card")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(10.0, 6.0),
            ..UiLayout::fixed(0.0, 46.0)
        })
        .with_child(
            UiNode::new(format!("assets.card.{index}.type"), UiNodeKind::Label)
                .with_text_key(asset_type_label(entry.asset_type, lang))
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fixed(56.0, 22.0)),
        )
        .with_child(
            UiNode::new(format!("assets.card.{index}.name"), UiNodeKind::Label)
                .with_text_key(entry.name.clone())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
        )
        .with_child(
            UiNode::new(format!("assets.card.{index}.size"), UiNodeKind::Label)
                .with_text_key(entry.size_display.clone())
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(72.0, 20.0)),
        );
    if is_script_name(&entry.name) {
        card = card.with_child(command_button(
            format!("assets.card.{index}.open"),
            "app.open_script",
            format!("assets.open-script:{index}"),
            "assets-open-script",
            palette,
        ));
    }
    card
}

fn asset_type_label(asset_type: AssetType, lang: Language) -> String {
    match asset_type {
        AssetType::Image => t("app.images", lang),
        AssetType::Model3D => t("app.models", lang),
        AssetType::Audio => t("app.audio", lang),
        AssetType::Scene => t("app.scripts_filter", lang),
        AssetType::Unknown => "FILE".to_string(),
    }
}

fn is_script_name(name: &str) -> bool {
    matches!(
        name.rsplit('.').next().map(str::to_lowercase).as_deref(),
        Some("rs" | "rhai" | "cpp" | "h" | "hpp" | "lua" | "py")
    )
}

fn build_ide_dialog_surface(palette: StudioUiPalette, file: &str) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("assets.ide.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(16.0),
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("assets.ide.title", UiNodeKind::Label)
                .with_text_key("app.ide_dialog_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("assets.ide.file", UiNodeKind::Label)
                .with_text_key(file.to_string())
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("assets.ide.message", UiNodeKind::Label)
                .with_text_key("app.ide_dialog_message")
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::Column)
                }),
        )
        .with_child(
            UiNode::new("assets.ide.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: raf_ui::UiJustify::End,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 32.0)
                })
                .with_child(command_button(
                    "assets.ide.cancel".to_string(),
                    "app.cancel",
                    "assets.ide.cancel".to_string(),
                    "assets-dialog-secondary",
                    palette,
                ))
                .with_child(command_button(
                    "assets.ide.yoll".to_string(),
                    "app.ide_open_yoll",
                    "assets.ide.yoll".to_string(),
                    "assets-dialog-secondary",
                    palette,
                ))
                .with_child(command_button(
                    "assets.ide.vscode".to_string(),
                    "app.ide_open_vscode",
                    "assets.ide.vscode".to_string(),
                    "assets-dialog-primary",
                    palette,
                )),
        );
    let mut surface = UiSurface::new("asset-ide-dialog", palette, root);
    surface.style_sheet = asset_style_sheet(palette);
    surface
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
            min_size: [48.0, 26.0],
            padding: UiSpacing::xy(7.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn asset_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
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
                UiStyleSelector::Class("assets-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-filter".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-filter-active".to_string()),
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
                UiStyleSelector::Class("assets-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-action-subtle".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-primitive".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-open-script".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-dialog-secondary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-dialog-primary".to_string()),
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
                UiStyleSelector::Class("assets-dialog-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}

fn parse_asset_command(name: &str) -> Option<AssetBrowserAction> {
    match name {
        "assets.filter.all" => Some(AssetBrowserAction::SetFilter(None)),
        "assets.filter.images" => Some(AssetBrowserAction::SetFilter(Some(AssetType::Image))),
        "assets.filter.models" => Some(AssetBrowserAction::SetFilter(Some(AssetType::Model3D))),
        "assets.filter.audio" => Some(AssetBrowserAction::SetFilter(Some(AssetType::Audio))),
        "assets.filter.scripts" => Some(AssetBrowserAction::SetFilter(Some(AssetType::Scene))),
        "assets.open-folder" => Some(AssetBrowserAction::OpenFolder),
        "assets.refresh" => Some(AssetBrowserAction::Refresh),
        "assets.create-script" => None,
        value if value.starts_with("assets.create-script:") => value
            .split_once(':')
            .map(|(_, kind)| AssetBrowserAction::CreateScript(kind.to_string())),
        value if value.starts_with("assets.open-script:") => value
            .split_once(':')
            .and_then(|(_, index)| index.parse().ok())
            .map(AssetBrowserAction::OpenScript),
        value if value.starts_with("assets.add-primitive:") => value
            .split_once(':')
            .and_then(|(_, primitive)| parse_primitive(primitive))
            .map(AssetBrowserAction::AddPrimitive),
        _ => None,
    }
}

fn parse_primitive(value: &str) -> Option<Primitive> {
    match value {
        "Cube" => Some(Primitive::Cube),
        "Sphere" => Some(Primitive::Sphere),
        "Plane" => Some(Primitive::Plane),
        "Cylinder" => Some(Primitive::Cylinder),
        _ => None,
    }
}
