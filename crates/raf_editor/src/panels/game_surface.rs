//! Retained RafUI surfaces for the Game hierarchy and inspector.
//!
//! The scene graph remains authoritative in `AuraRafiApp`. These surfaces only
//! describe the workbench controls and emit stable commands back to the app.

use std::collections::HashSet;

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::{Primitive, SceneGraph, SceneNodeId};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiImage, UiImageFit,
    UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRange, UiScrollAxis,
    UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiTextInput, UiTextStyle, UiToggle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, PartialEq)]
pub enum GameHierarchyAction {
    Select(SceneNodeId),
    ToggleVisibility(SceneNodeId),
    Delete(SceneNodeId),
    Duplicate(SceneNodeId),
    AddFolder(Option<SceneNodeId>),
    Ungroup(SceneNodeId),
}

pub struct GameHierarchySurfaceHost {
    bridge: RafUiSurfaceBridge,
    search_query: String,
    collapsed: HashSet<SceneNodeId>,
    icons_registered: bool,
}

impl Default for GameHierarchySurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_game_hierarchy"),
            search_query: String::new(),
            collapsed: HashSet::new(),
            icons_registered: false,
        }
    }
}

impl GameHierarchySurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        lang: Language,
    ) -> Vec<GameHierarchyAction> {
        self.register_icons();
        let surface = build_game_hierarchy_surface(
            palette,
            scene,
            selected,
            &self.search_query,
            &self.collapsed,
            lang,
        );
        let search = self.search_query.clone();
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("game.hierarchy.search", search.clone(), 128),
            |key| t(key, lang),
        );
        let mut output = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "game.hierarchy.search" => {
                    self.search_query = value;
                }
                UiAction::Command { name } => {
                    if let Some(id) = parse_node_command(&name, "game.hierarchy.select") {
                        output.push(GameHierarchyAction::Select(id));
                    } else if let Some(id) =
                        parse_node_command(&name, "game.hierarchy.toggle-visibility")
                    {
                        output.push(GameHierarchyAction::ToggleVisibility(id));
                    } else if let Some(id) = parse_node_command(&name, "game.hierarchy.delete") {
                        output.push(GameHierarchyAction::Delete(id));
                    } else if let Some(id) = parse_node_command(&name, "game.hierarchy.duplicate") {
                        output.push(GameHierarchyAction::Duplicate(id));
                    } else if let Some(id) = parse_node_command(&name, "game.hierarchy.ungroup") {
                        output.push(GameHierarchyAction::Ungroup(id));
                    } else if let Some(id) = parse_node_command(&name, "game.hierarchy.toggle-open")
                    {
                        if !self.collapsed.insert(id) {
                            self.collapsed.remove(&id);
                        }
                    } else if name == "game.hierarchy.add-root-folder" {
                        output.push(GameHierarchyAction::AddFolder(None));
                    } else if let Some(id) =
                        parse_node_command(&name, "game.hierarchy.add-child-folder")
                    {
                        output.push(GameHierarchyAction::AddFolder(Some(id)));
                    }
                }
                _ => {}
            }
        }
        output
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        for (key, bytes) in [
            (
                "game.hierarchy.icon.cube",
                include_bytes!("../../../../editor/assets/ui_icons/cube.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.cylinder",
                include_bytes!("../../../../editor/assets/ui_icons/cylinder.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.sphere",
                include_bytes!("../../../../editor/assets/ui_icons/sphere.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.plane",
                include_bytes!("../../../../editor/assets/ui_icons/plane.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.empty",
                include_bytes!("../../../../editor/assets/ui_icons/empty.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.folder",
                include_bytes!("../../../../editor/assets/ui_icons/folder.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.visible",
                include_bytes!("../../../../editor/assets/ui_icons/visible.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.hidden",
                include_bytes!("../../../../editor/assets/ui_icons/hidden.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.more",
                include_bytes!("../../../../editor/assets/ui_icons/3-dots vertical.png").as_slice(),
            ),
            (
                "game.hierarchy.icon.folder",
                include_bytes!("../../../../editor/assets/ui_icons/group.png").as_slice(),
            ),
            (
                "game.properties.icon.mesh",
                include_bytes!("../../../../editor/assets/ui_icons/mesh.png").as_slice(),
            ),
            (
                "game.properties.icon.collider",
                include_bytes!("../../../../editor/assets/ui_icons/shape.png").as_slice(),
            ),
        ] {
            let _ = self.bridge.register_embedded_png(key, bytes);
        }
        self.icons_registered = true;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GamePropertiesAction {
    SetName(String),
    SetVisible(bool),
    SetRange { key: String, value: f32 },
    SetPrimitive(Primitive),
    ResetTransform,
    ResetAll,
}

pub struct GamePropertiesSurfaceHost {
    bridge: RafUiSurfaceBridge,
    icons_registered: bool,
}

impl Default for GamePropertiesSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_game_properties"),
            icons_registered: false,
        }
    }
}

impl GamePropertiesSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        lang: Language,
    ) -> Vec<GamePropertiesAction> {
        self.register_icons();
        let surface = build_game_properties_surface(palette, scene, selected, lang);
        let name = selected
            .and_then(|id| scene.get(id))
            .map(|node| node.name.clone())
            .unwrap_or_default();
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("game.properties.name", name.clone(), 160),
            |key| t(key, lang),
        );
        actions
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::SetText { key, value } if key == "game.properties.name" => {
                    Some(GamePropertiesAction::SetName(value))
                }
                UiAction::SetToggle { key, value } if key == "game.properties.visible" => {
                    Some(GamePropertiesAction::SetVisible(value))
                }
                UiAction::SetRange { key, value } => {
                    Some(GamePropertiesAction::SetRange { key, value })
                }
                UiAction::Command { name } => match name.as_str() {
                    "game.properties.reset-transform" => Some(GamePropertiesAction::ResetTransform),
                    "game.properties.reset-all" => Some(GamePropertiesAction::ResetAll),
                    value => value
                        .strip_prefix("game.properties.primitive:")
                        .and_then(parse_primitive)
                        .map(GamePropertiesAction::SetPrimitive),
                },
                _ => None,
            })
            .collect()
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        for (key, bytes) in [
            (
                "game.properties.icon.scene",
                include_bytes!("../../../../editor/assets/ui_icons/scene.png").as_slice(),
            ),
            (
                "game.properties.icon.transform",
                include_bytes!("../../../../editor/assets/ui_icons/transform.png").as_slice(),
            ),
            (
                "game.properties.icon.material",
                include_bytes!("../../../../editor/assets/ui_icons/material.png").as_slice(),
            ),
            (
                "game.properties.icon.shape",
                include_bytes!("../../../../editor/assets/ui_icons/shape.png").as_slice(),
            ),
            (
                "game.properties.icon.variables",
                include_bytes!("../../../../editor/assets/ui_icons/variables.png").as_slice(),
            ),
        ] {
            let _ = self.bridge.register_embedded_png(key, bytes);
        }
        self.icons_registered = true;
    }
}

fn build_game_hierarchy_surface(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    search_query: &str,
    collapsed: &HashSet<SceneNodeId>,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut search = UiTextInput::new("game.hierarchy.search");
    search.placeholder_key = Some("app.search".to_string());
    search.max_length = 128;

    let mut list = UiNode::scroll_view("game.hierarchy.list", UiScrollAxis::Vertical)
        .with_class("game-surface-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 6.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    if scene.is_empty() {
        list = list.with_child(
            UiNode::new("game.hierarchy.empty", UiNodeKind::Label)
                .with_text_key("app.no_entities")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 28.0)),
        );
    } else {
        let filter = search_query.trim().to_lowercase();
        for root in scene.roots() {
            append_hierarchy_node(
                &mut list, scene, *root, 0, selected, collapsed, &filter, tokens,
            );
        }
    }

    let root = UiNode::new("game.hierarchy.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("game.hierarchy.header", UiNodeKind::Toolbar)
                .with_class("game-surface-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 4.0,
                    padding: UiSpacing::xy(10.0, 5.0),
                    ..UiLayout::fixed(0.0, 70.0)
                })
                .with_child(
                    UiNode::new("game.hierarchy.header-main", UiNodeKind::Toolbar)
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            align_items: UiAlign::Center,
                            ..UiLayout::fixed(0.0, 22.0)
                        })
                        .with_child(
                            UiNode::new("game.hierarchy.title", UiNodeKind::Label)
                                .with_text_key("app.hierarchy")
                                .with_text_style(UiTextStyle::panel_title(tokens.text))
                                .with_layout(UiLayout {
                                    grow: 1.0,
                                    ..UiLayout::default()
                                }),
                        )
                        .with_child(
                            UiNode::new("game.hierarchy.count", UiNodeKind::Label)
                                .with_text_key(format!(
                                    "{}: {}",
                                    t("app.entities_count", lang),
                                    scene.len()
                                ))
                                .with_text_style(UiTextStyle::body(tokens.text_muted))
                                .with_layout(UiLayout {
                                    grow: 1.0,
                                    justify_content: UiJustify::End,
                                    ..UiLayout::default()
                                }),
                        ),
                )
                .with_child(
                    UiNode::new("game.hierarchy.header-controls", UiNodeKind::Toolbar)
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            align_items: UiAlign::Center,
                            gap: 6.0,
                            ..UiLayout::fixed(0.0, 30.0)
                        })
                        .with_child(
                            UiNode::text_input("game.hierarchy.search.input", search)
                                .with_class("game-surface-search")
                                .with_layout(UiLayout {
                                    grow: 1.0,
                                    min_size: [0.0, 30.0],
                                    ..UiLayout::default()
                                }),
                        )
                        .with_child(command_icon_button(
                            "game.hierarchy.add-folder-control",
                            "game.hierarchy.add-root-folder",
                            "game.hierarchy.icon.folder",
                            "app.add_folder",
                            "game-tree-action",
                        )),
                ),
        )
        .with_child(list);

    let mut surface = UiSurface::new("game-hierarchy", palette, root);
    surface.style_sheet = game_surface_style_sheet(palette);
    surface
}

fn append_hierarchy_node(
    list: &mut UiNode,
    scene: &SceneGraph,
    id: SceneNodeId,
    depth: usize,
    selected: &[SceneNodeId],
    collapsed: &HashSet<SceneNodeId>,
    filter: &str,
    tokens: raf_ui::UiTokens,
) {
    let Some(node) = scene.get(id) else { return };
    if !filter.is_empty() && !node_matches_filter(scene, id, filter) {
        return;
    }

    let prefix = format!("game.hierarchy.node.{}", id.0);
    let is_selected = selected.contains(&id);
    let has_children = !node.children.is_empty();
    let is_collapsed = collapsed.contains(&id);
    let indent = 4.0 + depth as f32 * 16.0;

    if node.is_folder {
        let count = node.children.len();
        let row_class = if is_selected {
            "game-tree-folder-selected"
        } else {
            "game-tree-folder"
        };
        let mut row = UiNode::new(prefix.clone(), UiNodeKind::Panel)
            .with_class(row_class)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 5.0,
                padding: UiSpacing {
                    left: indent,
                    right: 6.0,
                    top: 3.0,
                    bottom: 3.0,
                },
                ..UiLayout::fixed(0.0, 24.0)
            });
        if has_children {
            row = row.with_child(
                UiNode::new(format!("{prefix}.expand"), UiNodeKind::Button)
                    .with_class("game-tree-expander")
                    .with_text_key(if is_collapsed { ">" } else { "v" })
                    .with_tooltip_key(if is_collapsed {
                        "game.hierarchy.expand"
                    } else {
                        "game.hierarchy.collapse"
                    })
                    .with_accessibility_label_key(if is_collapsed {
                        "game.hierarchy.expand"
                    } else {
                        "game.hierarchy.collapse"
                    })
                    .with_text_style(UiTextStyle::button(tokens.text_muted))
                    .with_layout(UiLayout::fixed(14.0, 18.0))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("game.hierarchy.toggle-open:{}", id.0),
                    )),
            );
        } else {
            row = row.with_child(
                UiNode::new(format!("{prefix}.spacer"), UiNodeKind::Panel)
                    .with_layout(UiLayout::fixed(14.0, 18.0))
                    .with_style(raf_ui::UiStyle::transparent()),
            );
        }
        row = row
            .with_child(
                UiNode::image(
                    format!("{prefix}.type-icon"),
                    UiImage {
                        source: UiImageSource::new("game.hierarchy.icon.folder"),
                        fit: UiImageFit::Contain,
                        tint: Some([255, 255, 255, 220]),
                    },
                )
                .with_layout(UiLayout::fixed(14.0, 14.0)),
            )
            .with_child(
                UiNode::new(format!("{prefix}.select"), UiNodeKind::Button)
                    .with_text_key(node.name.clone())
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_class("game-tree-select")
                    .with_tooltip_key("game.hierarchy.select")
                    .with_accessibility_label_key("game.hierarchy.select")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        min_size: [0.0, 20.0],
                        padding: UiSpacing::xy(4.0, 1.0),
                        ..UiLayout::default()
                    })
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("game.hierarchy.select:{}", id.0),
                    )),
            )
            .with_child(
                UiNode::new(format!("{prefix}.count"), UiNodeKind::Label)
                    .with_text_key(format!("{}", count))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fixed(20.0, 16.0)),
            )
            .with_child(command_icon_button(
                format!("{prefix}.more"),
                format!("game.hierarchy.context:{}", id.0),
                "game.hierarchy.icon.more",
                "app.more_menu",
                "game-tree-action",
            ));
        list.children.push(row);
    } else {
        let row_class = if is_selected {
            "game-tree-row-selected"
        } else {
            "game-tree-row"
        };
        let mut row = UiNode::new(prefix.clone(), UiNodeKind::Panel)
            .with_class(row_class)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                padding: UiSpacing {
                    left: indent,
                    right: 6.0,
                    top: 3.0,
                    bottom: 3.0,
                },
                ..UiLayout::fixed(0.0, 28.0)
            });
        if has_children {
            row = row.with_child(
                UiNode::new(format!("{prefix}.expand"), UiNodeKind::Button)
                    .with_class("game-tree-expander")
                    .with_text_key(if is_collapsed { ">" } else { "v" })
                    .with_tooltip_key(if is_collapsed {
                        "game.hierarchy.expand"
                    } else {
                        "game.hierarchy.collapse"
                    })
                    .with_accessibility_label_key(if is_collapsed {
                        "game.hierarchy.expand"
                    } else {
                        "game.hierarchy.collapse"
                    })
                    .with_text_style(UiTextStyle::button(tokens.text_muted))
                    .with_layout(UiLayout::fixed(14.0, 18.0))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("game.hierarchy.toggle-open:{}", id.0),
                    )),
            );
        } else {
            row = row.with_child(
                UiNode::new(format!("{prefix}.spacer"), UiNodeKind::Panel)
                    .with_layout(UiLayout::fixed(14.0, 18.0))
                    .with_style(raf_ui::UiStyle::transparent()),
            );
        }
        row = row
            .with_child(
                UiNode::image(
                    format!("{prefix}.type-icon"),
                    UiImage {
                        source: UiImageSource::new(node_type_icon_key(node)),
                        fit: UiImageFit::Contain,
                        tint: Some(if is_selected {
                            tokens.accent
                        } else {
                            [255, 255, 255, 235]
                        }),
                    },
                )
                .with_layout(UiLayout::fixed(15.0, 15.0)),
            )
            .with_child(
                UiNode::new(format!("{prefix}.select"), UiNodeKind::Button)
                    .with_text_key(node.name.clone())
                    .with_text_style(UiTextStyle::body(tokens.text))
                    .with_class("game-tree-select")
                    .with_tooltip_key("game.hierarchy.select")
                    .with_accessibility_label_key("game.hierarchy.select")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        min_size: [0.0, 24.0],
                        padding: UiSpacing::xy(5.0, 2.0),
                        ..UiLayout::default()
                    })
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("game.hierarchy.select:{}", id.0),
                    )),
            )
            .with_child(
                UiNode::new(format!("{prefix}.visibility"), UiNodeKind::Button)
                    .with_class(if node.visible {
                        "game-tree-visibility-on"
                    } else {
                        "game-tree-visibility-off"
                    })
                    .with_layout(UiLayout::fixed(22.0, 24.0))
                    .with_accessibility_label_key("game.hierarchy.toggle_visibility")
                    .with_tooltip_key("game.hierarchy.toggle_visibility")
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("game.hierarchy.toggle-visibility:{}", id.0),
                    ))
                    .with_child(
                        UiNode::image(
                            format!("{prefix}.visibility-icon"),
                            UiImage {
                                source: UiImageSource::new(if node.visible {
                                    "game.hierarchy.icon.visible"
                                } else {
                                    "game.hierarchy.icon.hidden"
                                }),
                                fit: UiImageFit::Contain,
                                tint: Some(if node.visible {
                                    [255, 255, 255, 235]
                                } else {
                                    [190, 196, 204, 200]
                                }),
                            },
                        )
                        .with_layout(UiLayout::fixed(14.0, 14.0)),
                    ),
            )
            .with_child(command_icon_button(
                format!("{prefix}.more"),
                format!("game.hierarchy.context:{}", id.0),
                "game.hierarchy.icon.more",
                "app.more_menu",
                "game-tree-action",
            ));
        list.children.push(row);
    }

    if has_children && !is_collapsed {
        for child in &node.children {
            append_hierarchy_node(
                list,
                scene,
                *child,
                depth + 1,
                selected,
                collapsed,
                filter,
                tokens,
            );
        }
    }
}

fn node_type_icon_key(node: &raf_core::scene::graph::SceneNode) -> &'static str {
    if node.is_folder {
        return "game.hierarchy.icon.folder";
    }
    match node.primitive {
        Primitive::Cube => "game.hierarchy.icon.cube",
        Primitive::Sphere => "game.hierarchy.icon.sphere",
        Primitive::Cylinder => "game.hierarchy.icon.cylinder",
        Primitive::Plane => "game.hierarchy.icon.plane",
        Primitive::Empty => "game.hierarchy.icon.empty",
    }
}

fn build_game_properties_surface(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("game.properties.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("game.properties.header", UiNodeKind::Toolbar)
                .with_class("game-surface-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(12.0, 0.0),
                    ..UiLayout::fixed(0.0, 34.0)
                })
                .with_child(
                    UiNode::new("game.properties.title", UiNodeKind::Label)
                        .with_text_key("app.inspector")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                ),
        )
        .with_child(properties_content(palette, scene, selected, lang));

    let mut surface = UiSurface::new("game-properties", palette, root);
    surface.style_sheet = game_surface_style_sheet(palette);
    surface
}

fn properties_content(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    lang: Language,
) -> UiNode {
    let tokens = palette.tokens();
    let Some(node) = selected.and_then(|id| scene.get(id)) else {
        return UiNode::new("game.properties.empty", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                gap: 8.0,
                padding: UiSpacing::xy(16.0, 24.0),
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::image(
                    "game.properties.empty.icon",
                    UiImage {
                        source: UiImageSource::new("game.properties.icon.scene"),
                        fit: UiImageFit::Contain,
                        tint: Some([151, 159, 170, 180]),
                    },
                )
                .with_layout(UiLayout::fixed(44.0, 44.0)),
            )
            .with_child(
                UiNode::new("game.properties.empty.title", UiNodeKind::Label)
                    .with_text_key("app.no_entity_selected")
                    .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
                    .with_layout(UiLayout::fixed(0.0, 22.0)),
            )
            .with_child(
                UiNode::new("game.properties.empty.hint", UiNodeKind::Label)
                    .with_text_key("app.properties_empty_hint")
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fixed(0.0, 18.0)),
            );
    };

    let mut name_input = UiTextInput::new("game.properties.name");
    name_input.max_length = 160;

    let mut content = UiNode::scroll_view("game.properties.scroll", UiScrollAxis::Vertical)
        .with_class("game-surface-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 10.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    content = content
        .with_child(section_label(palette, "app.node_info"))
        .with_child(
            UiNode::text_input("game.properties.name.input", name_input)
                .with_class("game-surface-input")
                .with_layout(UiLayout::fixed(0.0, 30.0))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("Enter".to_string()),
                    "game.properties.commit-name",
                )),
        )
        .with_child(meta_pills_row(palette, node, lang))
        .with_child(node_info_meta_row(palette))
        .with_child(section_label(palette, "app.transform"))
        .with_child(axis_row(
            palette,
            "app.position",
            "game.properties.position",
            [node.position.x, node.position.y, node.position.z],
            -1000.0,
            1000.0,
            0.1,
        ))
        .with_child(axis_row(
            palette,
            "app.rotation",
            "game.properties.rotation",
            [node.rotation.x, node.rotation.y, node.rotation.z],
            -360.0,
            360.0,
            0.5,
        ))
        .with_child(axis_row(
            palette,
            "app.scale",
            "game.properties.scale",
            [node.scale.x, node.scale.y, node.scale.z],
            0.01,
            100.0,
            0.05,
        ))
        .with_child(
            UiNode::new("game.properties.transform-actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 6.0,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(0.0, 28.0)
                })
                .with_style(raf_ui::UiStyle::transparent())
                .with_child(command_button(
                    "game.properties.reset-transform",
                    "app.reset",
                    "game.properties.reset-transform",
                    "game-surface-button",
                ))
                .with_child(command_button(
                    "game.properties.reset-all",
                    "app.reset_all",
                    "game.properties.reset-all",
                    "game-surface-button",
                )),
        )
        .with_child(section_label(palette, "app.material"))
        .with_child(
            UiNode::toggle(
                "game.properties.visible.toggle",
                UiToggle::new("game.properties.visible", node.visible),
            )
            .with_class("game-surface-toggle")
            .with_text_key("app.visible")
            .with_accessibility_label_key("game.properties.toggle_visibility")
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 28.0)),
        )
        .with_child(section_label(palette, "app.shape"))
        .with_child(
            UiNode::new("game.properties.primitive-grid", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::RowWrap,
                    gap: 6.0,
                    ..UiLayout::fill(UiFlow::RowWrap)
                })
                .with_style(raf_ui::UiStyle::transparent())
                .with_child(primitive_chip(palette, node, Primitive::Cube))
                .with_child(primitive_chip(palette, node, Primitive::Sphere))
                .with_child(primitive_chip(palette, node, Primitive::Plane))
                .with_child(primitive_chip(palette, node, Primitive::Cylinder))
                .with_child(primitive_chip(palette, node, Primitive::Empty)),
        )
        .with_child(section_label(palette, "app.components"))
        .with_child(components_row(palette, node));

    content
}

fn node_info_meta_row(palette: StudioUiPalette) -> UiNode {
    UiNode::new("game.properties.node-info-meta", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 56.0)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(meta_field_row(
            palette,
            "game.properties.layer",
            "app.layer",
            "Default",
        ))
        .with_child(meta_toggle_row(
            palette,
            "game.properties.static",
            "app.static",
            false,
        ))
}

fn meta_field_row(palette: StudioUiPalette, id: &str, label_key: &str, value: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 22.0)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(72.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Button)
                .with_class("game-properties-pill")
                .with_text_key(value)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    padding: UiSpacing::xy(8.0, 2.0),
                    ..UiLayout::fixed(0.0, 22.0)
                })
                .with_tooltip_key(label_key)
                .focusable(),
        )
}

fn meta_toggle_row(palette: StudioUiPalette, id: &str, label_key: &str, value: bool) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 22.0)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(72.0, 18.0)),
        )
        .with_child(
            UiNode::toggle(
                format!("{id}.toggle"),
                UiToggle::new(format!("{id}.value"), value),
            )
            .with_class(if value {
                "game-properties-toggle-on"
            } else {
                "game-properties-toggle-off"
            })
            .with_layout(UiLayout::fixed(40.0, 18.0)),
        )
}

fn components_row(palette: StudioUiPalette, node: &raf_core::scene::graph::SceneNode) -> UiNode {
    let mut row = UiNode::new("game.properties.components", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(raf_ui::UiStyle::transparent());
    if !node.is_folder {
        row = row
            .with_child(component_item(
                palette,
                "game.properties.component.mesh",
                "app.mesh_renderer",
                true,
            ))
            .with_child(component_item(
                palette,
                "game.properties.component.collider",
                "app.collider",
                true,
            ));
    }
    row
}

fn component_item(palette: StudioUiPalette, id: &str, label_key: &str, enabled: bool) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("game-properties-component")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(if id.contains("mesh") {
                        "game.properties.icon.mesh"
                    } else {
                        "game.properties.icon.collider"
                    }),
                    fit: UiImageFit::Contain,
                    tint: Some([255, 255, 255, 220]),
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 18.0)
                }),
        )
        .with_child(
            UiNode::toggle(
                format!("{id}.toggle"),
                UiToggle::new(format!("{id}.value"), enabled),
            )
            .with_class(if enabled {
                "game-properties-toggle-on"
            } else {
                "game-properties-toggle-off"
            })
            .with_layout(UiLayout::fixed(34.0, 18.0)),
        )
}

fn meta_pills_row(
    palette: StudioUiPalette,
    node: &raf_core::scene::graph::SceneNode,
    lang: Language,
) -> UiNode {
    let children_label = format!("{}: {}", t("app.children", lang), node.children.len());
    let mut row = UiNode::new("game.properties.meta", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            gap: 6.0,
            ..UiLayout::fill(UiFlow::RowWrap)
        })
        .with_style(raf_ui::UiStyle::transparent());
    row = row
        .with_child(meta_pill(
            palette,
            "game.properties.meta.primitive",
            node.primitive.label().to_string(),
        ))
        .with_child(meta_pill(
            palette,
            "game.properties.meta.children",
            children_label,
        ));
    row
}

fn meta_pill(palette: StudioUiPalette, id: &str, label: String) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Label)
        .with_class("game-properties-pill")
        .with_text_key(label)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 3.0),
            ..UiLayout::fixed(0.0, 20.0)
        })
}

fn primitive_chip(
    palette: StudioUiPalette,
    node: &raf_core::scene::graph::SceneNode,
    primitive: Primitive,
) -> UiNode {
    let active = node.primitive == primitive;
    let class = if active {
        "game-properties-primitive-active"
    } else {
        "game-properties-primitive"
    };
    UiNode::new(
        format!("game.properties.primitive.{}", primitive.label()),
        UiNodeKind::Button,
    )
    .with_class(class)
    .with_text_key(primitive.label())
    .with_text_style(UiTextStyle::button(if active {
        [18, 18, 20, 255]
    } else {
        palette.tokens().text
    }))
    .with_layout(UiLayout {
        basis: [0.0, 26.0],
        min_size: [60.0, 26.0],
        padding: UiSpacing::xy(10.0, 0.0),
        ..UiLayout::default()
    })
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("game.properties.primitive:{}", primitive.label()),
    ))
}

fn axis_row(
    palette: StudioUiPalette,
    label_key: &str,
    value_prefix: &str,
    values: [f32; 3],
    min: f32,
    max: f32,
    step: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let mut row = UiNode::new(format!("{value_prefix}.row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 30.0)
        })
        .with_style(raf_ui::UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{value_prefix}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(56.0, 18.0)),
        );
    for (index, axis) in ["x", "y", "z"].iter().enumerate() {
        let key = format!("{value_prefix}.{axis}");
        let mut axis_field = UiNode::new(format!("{key}.field"), UiNodeKind::Panel)
            .with_class("game-properties-axis-field")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 4.0,
                padding: UiSpacing::xy(6.0, 0.0),
                grow: 1.0,
                ..UiLayout::fixed(0.0, 24.0)
            });
        let accent = match *axis {
            "x" => [220, 88, 80, 255],
            "y" => [120, 200, 96, 255],
            _ => [96, 152, 220, 255],
        };
        axis_field = axis_field
            .with_child(
                UiNode::new(format!("{key}.axis"), UiNodeKind::Label)
                    .with_text_key(axis.to_uppercase())
                    .with_text_style(UiTextStyle::button(accent))
                    .with_layout(UiLayout::fixed(10.0, 18.0)),
            )
            .with_child(
                UiNode::range(
                    format!("{key}.control"),
                    UiRange::new(key, values[index], min, max, step),
                )
                .with_class("game-surface-range")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [0.0, 24.0],
                    ..UiLayout::fixed(0.0, 24.0)
                }),
            );
        row = row.with_child(axis_field);
    }
    row
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    let tokens = palette.tokens();
    let icon_key = match key {
        "app.node_info" => "game.properties.icon.scene",
        "app.transform" => "game.properties.icon.transform",
        "app.material" => "game.properties.icon.material",
        "app.shape" => "game.properties.icon.shape",
        "app.variables" => "game.properties.icon.variables",
        _ => "game.properties.icon.scene",
    };
    UiNode::new(format!("game.properties.section.{key}"), UiNodeKind::Panel)
        .with_class("game-properties-section")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing {
                left: 8.0,
                right: 8.0,
                top: 5.0,
                bottom: 5.0,
            },
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_child(
            UiNode::image(
                format!("game.properties.section.{key}.icon"),
                UiImage {
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: Some(tokens.text_muted),
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
        .with_child(
            UiNode::new(
                format!("game.properties.section.{key}.label"),
                UiNodeKind::Label,
            )
            .with_text_key(key)
            .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        )
}

fn command_button(
    id: impl Into<String>,
    label: impl Into<String>,
    command: impl Into<String>,
    class: &str,
) -> UiNode {
    let id = id.into();
    let label = label.into();
    UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label.clone())
        .with_accessibility_label_key(label)
        .with_layout(UiLayout {
            min_size: [28.0, 24.0],
            padding: UiSpacing::xy(6.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn command_icon_button(
    id: impl Into<String>,
    command: impl Into<String>,
    image_key: &str,
    tooltip_key: &str,
    class: &str,
) -> UiNode {
    let id = id.into();
    UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class(class)
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .with_layout(UiLayout::fixed(28.0, 24.0))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(image_key),
                    fit: UiImageFit::Contain,
                    tint: Some([226, 231, 238, 210]),
                },
            )
            .with_layout(UiLayout::fixed(12.0, 14.0)),
        )
}

fn game_surface_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-scroll".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-search".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-control-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-icon-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-icon-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-section".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-button-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-row".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-row".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [22, 26, 32, 220],
                        StudioUiPalette::PaperLight => [232, 234, 238, 220],
                    }),
                    border: Some(match palette {
                        StudioUiPalette::IndustrialDark => [38, 42, 48, 255],
                        StudioUiPalette::PaperLight => [210, 212, 216, 255],
                    }),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-folder".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-folder".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [18, 22, 28, 200],
                        StudioUiPalette::PaperLight => [238, 240, 244, 200],
                    }),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-folder-selected".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [30, 24, 14, 220],
                        StudioUiPalette::PaperLight => [252, 240, 224, 220],
                    }),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-row-selected".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [34, 26, 14, 230],
                        StudioUiPalette::PaperLight => [255, 240, 220, 230],
                    }),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-select".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-select".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [36, 40, 48, 190],
                        StudioUiPalette::PaperLight => [224, 226, 232, 190],
                    }),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-select".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-action".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-action".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [40, 44, 52, 220],
                        StudioUiPalette::PaperLight => [220, 222, 228, 220],
                    }),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-expander".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-expander".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [40, 44, 52, 220],
                        StudioUiPalette::PaperLight => [220, 222, 228, 220],
                    }),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-visibility-on".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-visibility-off".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-visibility-off".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [40, 44, 52, 220],
                        StudioUiPalette::PaperLight => [220, 222, 228, 220],
                    }),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-tree-visibility-on".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [40, 44, 52, 220],
                        StudioUiPalette::PaperLight => [220, 222, 228, 220],
                    }),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-toggle".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-range".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-surface-range".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-pill".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [22, 26, 32, 220],
                        StudioUiPalette::PaperLight => [232, 234, 238, 220],
                    }),
                    border: Some(match palette {
                        StudioUiPalette::IndustrialDark => [38, 42, 48, 255],
                        StudioUiPalette::PaperLight => [210, 212, 216, 255],
                    }),
                    border_width: Some(1.0),
                    radius: Some(10.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-primitive".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-primitive".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [30, 34, 40, 255],
                        StudioUiPalette::PaperLight => [220, 222, 228, 255],
                    }),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-primitive-active".to_string()),
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
                UiStyleSelector::Class("game-properties-axis-field".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [18, 22, 28, 255],
                        StudioUiPalette::PaperLight => [240, 242, 246, 255],
                    }),
                    border: Some(match palette {
                        StudioUiPalette::IndustrialDark => [32, 36, 42, 255],
                        StudioUiPalette::PaperLight => [220, 222, 226, 255],
                    }),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-toggle-on".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(9.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-toggle-off".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [40, 44, 52, 255],
                        StudioUiPalette::PaperLight => [220, 222, 228, 255],
                    }),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(9.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("game-properties-component".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [22, 26, 32, 200],
                        StudioUiPalette::PaperLight => [232, 234, 238, 200],
                    }),
                    border: Some(match palette {
                        StudioUiPalette::IndustrialDark => [32, 36, 42, 255],
                        StudioUiPalette::PaperLight => [220, 222, 226, 255],
                    }),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}

fn parse_node_command(command: &str, prefix: &str) -> Option<SceneNodeId> {
    command
        .strip_prefix(prefix)
        .and_then(|value| value.strip_prefix(':'))
        .and_then(|value| value.parse::<usize>().ok())
        .map(SceneNodeId)
}

fn parse_primitive(value: &str) -> Option<Primitive> {
    match value {
        "Cube" => Some(Primitive::Cube),
        "Sphere" => Some(Primitive::Sphere),
        "Plane" => Some(Primitive::Plane),
        "Cylinder" => Some(Primitive::Cylinder),
        "Empty" => Some(Primitive::Empty),
        _ => None,
    }
}

fn node_matches_filter(scene: &SceneGraph, id: SceneNodeId, filter: &str) -> bool {
    let Some(node) = scene.get(id) else {
        return false;
    };
    node.name.to_lowercase().contains(filter)
        || node
            .children
            .iter()
            .any(|child| node_matches_filter(scene, *child, filter))
}
