//! Declarative retained RafUI surface for the game Inspector.
//!
//! The document contains presentation and semantic events only. Scene edits
//! are translated by `inspector_surface_host.rs` and committed by the editor
//! application so they share selection, history and persistence with
//! Hierarchy.

use glam::Vec3;
use raf_core::scene::{
    NodeColor, Primitive, SceneGraph, SceneNodeId, SceneVariable, VariableValue,
};
use raf_core::session::{ProjectSessionKind, ProjectSessionRegistry};
use raf_core::units::DisplayUnit;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiImage, UiImageFit, UiImageSource, UiSurface,
};
use raf_ui::components::select_trigger;
use raf_ui::{
    UiAlign, UiColorPicker, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiRange, UiRect, UiScrollAxis, UiSelect, UiSelectOption, UiSizeMode, UiSpacing,
    UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiSurfaceMaterial, UiTextInput, UiTextOverflow, UiTextStyle, UiToggle,
};

use crate::color_math::rgb_to_hsv;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorDropdown {
    Primitive,
    Collider,
    BodyType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorTab {
    Properties,
    Sessions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorSection {
    Identity,
    Transform,
    Appearance,
    Components,
    Variables,
    Audio,
    Physics,
    Metadata,
    Debug,
}

impl InspectorSection {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Transform => "transform",
            Self::Appearance => "appearance",
            Self::Components => "components",
            Self::Variables => "variables",
            Self::Audio => "audio",
            Self::Physics => "physics",
            Self::Metadata => "metadata",
            Self::Debug => "debug",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InspectorViewState {
    pub tab: InspectorTab,
    pub identity: bool,
    pub transform: bool,
    pub appearance: bool,
    pub components: bool,
    pub variables: bool,
    pub audio: bool,
    pub physics: bool,
    pub metadata: bool,
    pub debug: bool,
    pub dropdown: Option<InspectorDropdown>,
    pub color_picker: bool,
}

impl Default for InspectorViewState {
    fn default() -> Self {
        Self {
            tab: InspectorTab::Properties,
            identity: true,
            transform: true,
            appearance: true,
            components: true,
            variables: false,
            audio: false,
            physics: false,
            metadata: false,
            debug: false,
            dropdown: None,
            color_picker: false,
        }
    }
}

impl InspectorViewState {
    pub fn section(self, section: InspectorSection) -> bool {
        match section {
            InspectorSection::Identity => self.identity,
            InspectorSection::Transform => self.transform,
            InspectorSection::Appearance => self.appearance,
            InspectorSection::Components => self.components,
            InspectorSection::Variables => self.variables,
            InspectorSection::Audio => self.audio,
            InspectorSection::Physics => self.physics,
            InspectorSection::Metadata => self.metadata,
            InspectorSection::Debug => self.debug,
        }
    }
}

pub fn build_inspector_surface(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    transition: f32,
    view: InspectorViewState,
) -> UiSurface {
    build_inspector_surface_with_unit(
        palette,
        scene,
        selected,
        sessions,
        transition,
        view,
        DisplayUnit::Metric,
    )
}

pub fn build_inspector_surface_with_unit(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    transition: f32,
    view: InspectorViewState,
    display_unit: DisplayUnit,
) -> UiSurface {
    let mut root_style = palette.panel_style();
    root_style.opacity = (0.55 + transition.clamp(0.0, 1.0) * 0.45).clamp(0.0, 1.0);
    let root = UiNode::new("inspector.root", UiNodeKind::Root)
        .with_class("inspector-root")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::same(8.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(root_style)
        .with_child(header(palette))
        .with_child(tabs(palette, view.tab))
        .with_child(content(
            palette,
            scene,
            selected,
            sessions,
            view,
            display_unit,
        ));

    let mut surface = UiSurface::new("editor.inspector", palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

fn header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("inspector.header", UiNodeKind::Toolbar)
        .with_class("inspector-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Entity))
                .with_layout(UiLayout::fixed(20.0, 22.0)),
        )
        .with_child(
            UiNode::new("inspector.header.title", UiNodeKind::Label)
                .with_text_key("app.inspector")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(icon_button(
            palette,
            "inspector.close",
            UiIconId::Close,
            "inspector.panel.toggle",
            "ui.close",
        ))
}

fn tabs(palette: StudioUiPalette, active: InspectorTab) -> UiNode {
    UiNode::new("inspector.tabs", UiNodeKind::Toolbar)
        .with_class("inspector-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(tab_button(
            palette,
            "inspector.properties-tab",
            "app.properties",
            "inspector.tab:properties",
            active == InspectorTab::Properties,
        ))
        .with_child(tab_button(
            palette,
            "inspector.sessions-tab",
            "app.sessions",
            "inspector.tab:sessions",
            active == InspectorTab::Sessions,
        ))
}

fn tab_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
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
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(if active {
            palette.tokens().text
        } else {
            palette.tokens().text_muted
        }))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn content(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    view: InspectorViewState,
    display_unit: DisplayUnit,
) -> UiNode {
    if view.tab == InspectorTab::Sessions {
        return sessions_content(palette, sessions);
    }
    let tokens = palette.tokens();
    let Some(id) = selected.filter(|id| scene.is_valid_node(*id)) else {
        return UiNode::new("inspector.empty", UiNodeKind::Panel)
            .with_class("inspector-empty")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: raf_ui::UiJustify::Center,
                gap: 8.0,
                padding: UiSpacing::same(16.0),
                grow: 1.0,
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::new("inspector.empty.icon", UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(UiIconId::Entity)
                            .with_size(raf_render::api_graphic_basic::ui_surface::UiIconSize::Panel)
                            .with_tint(tokens.text_muted),
                    )
                    .with_layout(UiLayout::fixed(28.0, 28.0)),
            )
            .with_child(
                UiNode::new("inspector.empty.label", UiNodeKind::Label)
                    .with_text_key("app.inspector_empty")
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_layout(UiLayout::fit_content()),
            )
            .with_child(
                UiNode::new("inspector.empty.hint", UiNodeKind::Label)
                    .with_text_key("app.inspector_empty_hint")
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            );
    };
    let node = scene.get(id).expect("selected node was validated");
    let mut scroll =
        UiNode::scroll_view("inspector.scroll", UiScrollAxis::Vertical).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            grow: 1.0,
            padding: UiSpacing {
                left: 2.0,
                right: 5.0,
                top: 2.0,
                bottom: 18.0,
            },
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    scroll = scroll.with_child(section_title(
        palette,
        "app.identity",
        InspectorSection::Identity,
        view.identity,
    ));
    if view.identity {
        scroll = scroll.with_child(
            UiNode::text_input(
                "inspector.name",
                UiTextInput {
                    value_key: "inspector.name".to_string(),
                    placeholder_key: Some("app.name".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("inspector.rename.commit:{}", id.0)),
                },
            )
            .with_class("inspector-input")
            .with_layout(UiLayout {
                min_size: [120.0, 30.0],
                ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
            }),
        );
        scroll = scroll
            .with_child(node_info(palette, scene, node))
            .with_child(status_row(palette, id, node.visible, node.locked));
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.transform",
        InspectorSection::Transform,
        view.transform,
    ));
    if view.transform {
        scroll = scroll
            .with_child(vector_row(palette, "position", node.position, display_unit))
            .with_child(vector_row(palette, "rotation", node.rotation, display_unit))
            .with_child(vector_row(palette, "scale", node.scale, display_unit))
            .with_child(transform_actions(palette, id));
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.appearance",
        InspectorSection::Appearance,
        view.appearance,
    ));
    if view.appearance {
        let (primitive_options, primitive_selected_index) = primitive_options(node.primitive);
        scroll = scroll
            .with_child(dropdown_field(
                palette,
                "inspector.primitive",
                "app.primitive_type",
                InspectorDropdown::Primitive,
                view.dropdown,
                primitive_options,
                primitive_selected_index,
            ))
            .with_child(section_label(palette, "app.material"))
            .with_child(color_picker(palette, node.color, view.color_picker))
            .with_child(range_labeled(
                palette,
                "app.opacity",
                "inspector.appearance.opacity",
                f32::from(node.color.a) / 255.0 * 100.0,
                0.0,
                100.0,
                1.0,
            ));
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.components",
        InspectorSection::Components,
        view.components,
    ));
    if view.components {
        scroll = scroll.with_child(section_title(
            palette,
            "app.variables",
            InspectorSection::Variables,
            view.variables,
        ));
        if view.variables {
            scroll = scroll.with_child(variables_section(palette, id, &node.variables));
        }
        scroll = scroll.with_child(section_title(
            palette,
            "app.audio_source",
            InspectorSection::Audio,
            view.audio,
        ));
        if view.audio {
            scroll = scroll.with_child(audio_section(palette, id, node));
        }
        scroll = scroll.with_child(section_title(
            palette,
            "app.physics",
            InspectorSection::Physics,
            view.physics,
        ));
        if view.physics {
            scroll = scroll.with_child(physics_section(
                palette,
                id,
                node,
                view.dropdown,
                display_unit,
            ));
        }
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.metadata",
        InspectorSection::Metadata,
        view.metadata,
    ));
    if view.metadata {
        scroll = scroll.with_child(metadata_section(palette, scene, id, node));
    }
    scroll = scroll.with_child(section_title(
        palette,
        "app.debug",
        InspectorSection::Debug,
        view.debug,
    ));
    if view.debug {
        scroll = scroll.with_child(debug_section(palette, id, node));
    }

    UiNode::new("inspector.content", UiNodeKind::Panel)
        .with_class("inspector-content")
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(scroll)
}

pub(crate) fn sessions_content(
    palette: StudioUiPalette,
    registry: &ProjectSessionRegistry,
) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("inspector.sessions.list", UiScrollAxis::Vertical)
        .with_class("inspector-session-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            grow: 1.0,
            padding: UiSpacing {
                left: 2.0,
                right: 5.0,
                top: 0.0,
                bottom: 4.0,
            },
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    for session in &registry.sessions {
        list = list.with_child(session_row(palette, registry, session));
    }

    let controls = UiNode::new("inspector.sessions.controls", UiNodeKind::Panel)
        .with_class("inspector-sessions-controls")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.sessions.summary", UiNodeKind::Toolbar)
                .with_class("inspector-sessions-heading")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("inspector.sessions.summary.icon", UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Folder))
                        .with_layout(UiLayout::fixed(18.0, 20.0)),
                )
                .with_child(
                    UiNode::new("inspector.sessions.summary.label", UiNodeKind::Label)
                        .with_text_key("app.session_registry")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        }),
                ),
        )
        .with_child(
            UiNode::new("inspector.session.create-row", UiNodeKind::Toolbar)
                .with_class("inspector-session-create-row")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::text_input(
                        "inspector.session.new_name",
                        UiTextInput {
                            value_key: "inspector.session.new_name".to_string(),
                            placeholder_key: Some("app.session_name".to_string()),
                            max_length: 128,
                            multiline: false,
                            password: false,
                            submit_command: None,
                        },
                    )
                    .with_class("inspector-input")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        min_size: [120.0, 30.0],
                        ..UiLayout::fixed(0.0, 30.0)
                    }),
                )
                .with_child(session_create_button(palette)),
        );

    UiNode::new("inspector.sessions.content", UiNodeKind::Panel)
        .with_class("inspector-sessions-content")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::xy(6.0, 6.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(controls)
        .with_child(list)
}

fn session_create_button(palette: StudioUiPalette) -> UiNode {
    UiNode::new("inspector.session.create", UiNodeKind::Button)
        .with_class("inspector-session-create")
        .with_layout(UiLayout::fixed(124.0, 30.0))
        .with_text_key("app.session_create")
        .with_text_style(UiTextStyle::button(palette.accent_style().text))
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "inspector.session.create",
        ))
}

fn session_row(
    palette: StudioUiPalette,
    registry: &ProjectSessionRegistry,
    session: &raf_core::session::ProjectSession,
) -> UiNode {
    let active = registry.active_session == session.id;
    let tokens = palette.tokens();
    let row = UiNode::new(
        format!("inspector.session.row.{}", session.id.0),
        UiNodeKind::Panel,
    )
    .with_class(if active {
        "inspector-session-active"
    } else {
        "inspector-session-row"
    })
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 4.0,
        padding: UiSpacing::same(7.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(session_identity(palette, session, active))
    .with_child(
        UiNode::new(
            format!("inspector.session.kind.{}", session.id.0),
            UiNodeKind::Label,
        )
        .with_class("inspector-session-kind")
        .with_text_key(session_kind_label(session.kind))
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout {
            padding: UiSpacing {
                left: 24.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            },
            ..UiLayout::fit_content()
        }),
    );

    let id = session.id.0;
    let mut actions = UiNode::new(
        format!("inspector.session.actions.{}", id),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 4.0,
        ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
    });
    actions = actions
        .with_child(
            session_action_button(
                palette,
                &format!("inspector.session.open.{id}"),
                "app.session_open",
                format!("inspector.session.open:{id}"),
            )
            .disabled(active),
        )
        .with_child(session_action_button(
            palette,
            &format!("inspector.session.duplicate.{id}"),
            "app.session_duplicate",
            format!("inspector.session.duplicate:{id}"),
        ))
        .with_child(
            session_action_button(
                palette,
                &format!("inspector.session.remove.{id}"),
                "app.session_remove",
                format!("inspector.session.remove:{id}"),
            )
            .disabled(active),
        );
    row.with_child(actions)
}

fn session_identity(
    palette: StudioUiPalette,
    session: &raf_core::session::ProjectSession,
    active: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let mut identity = UiNode::new(
        format!("inspector.session.identity.{}", session.id.0),
        UiNodeKind::Toolbar,
    )
    .with_class("inspector-session-identity")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("inspector.session.icon.{}", session.id.0),
            UiNodeKind::Label,
        )
        .with_icon(UiIcon::new(session_kind_icon(session.kind)))
        .with_layout(UiLayout::fixed(18.0, 20.0)),
    )
    .with_child(
        UiNode::new(
            format!("inspector.session.name.{}", session.id.0),
            UiNodeKind::Label,
        )
        .with_text_value(session.name.clone())
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    );
    if active {
        identity = identity.with_child(
            UiNode::new(
                format!("inspector.session.active.{}", session.id.0),
                UiNodeKind::Label,
            )
            .with_class("inspector-session-active-label")
            .with_text_key("app.session_active")
            .with_text_style(UiTextStyle::button(tokens.accent))
            .with_layout(UiLayout::fit_content()),
        );
    }
    identity
}

fn session_kind_icon(kind: ProjectSessionKind) -> UiIconId {
    match kind {
        ProjectSessionKind::World => UiIconId::Scene,
        ProjectSessionKind::Interface => UiIconId::Node,
        ProjectSessionKind::ElectronicsDesign => UiIconId::Schematic,
    }
}

fn session_action_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: String,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("inspector-button")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [58.0, 26.0],
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 26.0)
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn session_kind_label(kind: ProjectSessionKind) -> &'static str {
    match kind {
        ProjectSessionKind::World => "app.session_kind_world",
        ProjectSessionKind::Interface => "app.session_kind_interface",
        ProjectSessionKind::ElectronicsDesign => "app.session_kind_electronics",
    }
}

fn node_info(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    let parent = node
        .parent
        .and_then(|parent| scene.get(parent).map(|parent| parent.name.clone()))
        .unwrap_or_else(|| "-".to_string());
    UiNode::new("inspector.node-info", UiNodeKind::Panel)
        .with_class("inspector-node-info")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(metadata_line(palette, "app.type", node.primitive.label()))
        .with_child(metadata_line(palette, "app.parent", &parent))
        .with_child(metadata_line(
            palette,
            "app.children",
            &node.children.len().to_string(),
        ))
        .with_child(metadata_line(
            palette,
            "app.scripts",
            &node.scripts.len().to_string(),
        ))
}

fn metadata_section(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.metadata-section", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(metadata_line(palette, "app.uuid", &node.uuid.to_string()))
        .with_child(metadata_line(
            palette,
            "app.entity_index",
            &node
                .entity_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ))
        .with_child(metadata_line(
            palette,
            "app.scene_path",
            &scene.node_path(id).unwrap_or_else(|| "-".to_string()),
        ))
        .with_child(metadata_line(
            palette,
            "app.source_asset",
            node.source_asset.as_deref().unwrap_or("-"),
        ))
        .with_child(metadata_line(
            palette,
            "app.schema_version",
            &node
                .source_schema_version
                .map(|version| version.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ))
}

fn debug_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.debug-section", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(metadata_key_line(
            palette,
            "app.renderable",
            if node.is_folder || node.primitive == Primitive::Empty {
                "app.no"
            } else {
                "app.yes"
            },
        ))
        .with_child(metadata_line(palette, "app.node_id", &id.0.to_string()))
        .with_child(metadata_key_line(
            palette,
            "app.node_kind",
            if node.is_folder {
                "app.folder"
            } else {
                "app.entity"
            },
        ))
}

fn metadata_key_line(palette: StudioUiPalette, label_key: &str, value_key: &str) -> UiNode {
    UiNode::new(
        format!("inspector.metadata.{label_key}"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 5.0,
        ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.label"),
            UiNodeKind::Label,
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.value"),
            UiNodeKind::Label,
        )
        .with_text_key(value_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text))
        .with_layout(UiLayout::fit_content()),
    )
}

fn metadata_line(palette: StudioUiPalette, label_key: &str, value: &str) -> UiNode {
    UiNode::new(
        format!("inspector.metadata.{label_key}"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 5.0,
        ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.label"),
            UiNodeKind::Label,
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.value"),
            UiNodeKind::Label,
        )
        .with_text_value(value.to_string())
        .with_text_style(UiTextStyle::body(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [72.0, 18.0],
            max_size: [210.0, 20.0],
            overflow: UiOverflow::Clip,
            ..UiLayout::fit_content()
        })
        .with_tooltip_value(value.to_string()),
    )
}

fn section_title(
    palette: StudioUiPalette,
    key: &str,
    section: InspectorSection,
    expanded: bool,
) -> UiNode {
    raf_ui::components::disclosure_header(
        format!("inspector.section.{}", section.slug()),
        key,
        section_icon(key),
        expanded,
        format!("inspector.section.toggle:{}", section.slug()),
        palette,
    )
    .with_class("inspector-section-title")
}

fn section_icon(key: &str) -> UiIconId {
    match key {
        "app.identity" => UiIconId::Entity,
        "app.transform" => UiIconId::Move,
        "app.appearance" => UiIconId::Shaded,
        "app.components" => UiIconId::Node,
        "app.metadata" => UiIconId::Assets,
        "app.debug" => UiIconId::Settings,
        "app.primitive" => UiIconId::Cube,
        "app.material" => UiIconId::Shaded,
        "app.variables" => UiIconId::Node,
        "app.audio_source" => UiIconId::Assets,
        "app.physics" => UiIconId::Warning,
        _ => UiIconId::ChevronRight,
    }
}

fn status_row(palette: StudioUiPalette, id: SceneNodeId, visible: bool, locked: bool) -> UiNode {
    UiNode::new("inspector.status", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(status_button(
            palette,
            "inspector.visibility",
            if visible { "app.visible" } else { "app.hidden" },
            format!("inspector.visibility:{id}", id = id.0),
            visible,
            if visible {
                UiIconId::Eye
            } else {
                UiIconId::EyeOff
            },
        ))
        .with_child(status_button(
            palette,
            "inspector.lock",
            if locked { "app.locked" } else { "app.unlocked" },
            format!("inspector.lock:{id}", id = id.0),
            locked,
            if locked {
                UiIconId::Lock
            } else {
                UiIconId::Unlock
            },
        ))
}

fn status_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: String,
    active: bool,
    icon: UiIconId,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "inspector-status-on"
        } else {
            "inspector-status-off"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            gap: 5.0,
            grow: 1.0,
            min_size: [80.0, 30.0],
            ..UiLayout::fixed(0.0, 30.0)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(raf_render::api_graphic_basic::ui_surface::UiIconSize::Small),
        )
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(if active {
            tokens.text
        } else {
            tokens.text_muted
        }))
        .with_tooltip_key(text_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn vector_row(
    palette: StudioUiPalette,
    label: &str,
    value: Vec3,
    display_unit: DisplayUnit,
) -> UiNode {
    let values = [value.x, value.y, value.z];
    let unit = match label {
        "position" => display_unit.distance_suffix(),
        "rotation" => "deg",
        "scale" => "x",
        _ => "",
    };
    let label_line = UiNode::new(
        format!("inspector.vector.{label}.label-line"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 4.0,
        ..UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(format!("inspector.vector.{label}.label"), UiNodeKind::Label)
            .with_text_key(format!("app.{label}"))
            .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
            .with_layout(UiLayout::fit_content()),
    )
    .with_child(
        UiNode::new(format!("inspector.vector.{label}.unit"), UiNodeKind::Label)
            .with_text_value(format!("({unit})"))
            .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
            .with_layout(UiLayout::fit_content()),
    );
    let row = UiNode::new(format!("inspector.vector.{label}"), UiNodeKind::Panel)
        .with_class("inspector-vector")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fixed(0.0, 58.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(label_line);
    let mut controls = UiNode::new(
        format!("inspector.vector.{label}.controls"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 3.0,
        ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
    });
    for (axis, current) in [("x", values[0]), ("y", values[1]), ("z", values[2])] {
        let axis_cell = UiNode::new(format!("inspector.{label}.{axis}.cell"), UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 2.0,
                grow: 1.0,
                min_size: [42.0, 32.0],
                ..UiLayout::fixed(0.0, 32.0)
            })
            .with_child(
                UiNode::new(
                    format!("inspector.{label}.{axis}.caption"),
                    UiNodeKind::Label,
                )
                .with_text_value(axis.to_ascii_uppercase())
                .with_text_style(UiTextStyle::button(axis_color(axis)))
                .with_layout(UiLayout::fit_content()),
            )
            .with_child(numeric_text_input(palette, label, axis, current));
        controls = controls.with_child(axis_cell);
    }
    row.with_child(controls)
}

fn axis_color(axis: &str) -> [u8; 4] {
    match axis {
        "x" => [255, 102, 102, 255],
        "y" => [92, 214, 128, 255],
        "z" => [92, 166, 255, 255],
        _ => [210, 214, 220, 255],
    }
}

fn numeric_text_input(palette: StudioUiPalette, label: &str, axis: &str, _current: f32) -> UiNode {
    let id = format!("inspector.{label}.{axis}.input");
    let value_key = numeric_text_key(label, axis);
    UiNode::text_input(
        id,
        UiTextInput {
            value_key,
            placeholder_key: Some("app.value".to_string()),
            max_length: 24,
            multiline: false,
            password: false,
            submit_command: Some("inspector.numeric.commit".to_string()),
        },
    )
    .with_class("inspector-number-input")
    .with_layout(UiLayout {
        min_size: [42.0, 24.0],
        ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_text_style(UiTextStyle::body(palette.tokens().text))
}

fn numeric_text_key(label: &str, axis: &str) -> String {
    format!("inspector.{label}.{axis}.text")
}

fn primitive_options(current: Primitive) -> (Vec<UiSelectOption>, usize) {
    let primitives = [
        Primitive::Empty,
        Primitive::Cube,
        Primitive::Sphere,
        Primitive::Plane,
        Primitive::Cylinder,
    ];
    let selected_index = primitives
        .iter()
        .position(|primitive| *primitive == current)
        .unwrap_or(0);
    let options = primitives
        .into_iter()
        .map(|primitive| {
            UiSelectOption::new(
                primitive.label().to_ascii_lowercase(),
                primitive_label_key(primitive),
            )
        })
        .collect();
    (options, selected_index)
}

fn dropdown_field(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    dropdown: InspectorDropdown,
    open: Option<InspectorDropdown>,
    options: Vec<UiSelectOption>,
    selected_index: usize,
) -> UiNode {
    let is_open = open == Some(dropdown);
    let value_key = id.to_string();
    let trigger_id = format!("{id}.trigger");
    let popup_id = format!("{id}.menu");
    let select = UiSelect::new(value_key.clone(), options.clone(), selected_index)
        .with_popup_id(popup_id.clone());
    let mut select = select;
    select.open = is_open;
    let mut field = UiNode::new(format!("{id}.field"), UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 3.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    field = field
        .with_child(section_label(palette, label_key))
        .with_child(
            select_trigger(trigger_id.clone(), select, palette)
                .with_class("inspector-dropdown-trigger")
                .with_layout(UiLayout::fixed(0.0, 29.0).with_width_mode(UiSizeMode::Fill))
                .with_accessibility_label_key(label_key),
        );
    if is_open {
        let menu_height = (options.len() as f32 * 28.0 + 8.0).clamp(36.0, 196.0);
        let mut menu = UiNode::new(format!("{id}.menu"), UiNodeKind::Menu)
            .with_class("inspector-dropdown-menu")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_style(UiStyle {
                fill: palette.tokens().surface_raised,
                border: palette.tokens().border,
                text: palette.tokens().text,
                border_width: 1.0,
                radius: 4.0,
                opacity: 1.0,
            })
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 1.0,
                padding: UiSpacing::same(3.0),
                ..UiLayout::absolute(UiRect::new(0.0, 29.0, 252.0, menu_height)).with_z_index(24)
            });
        for (index, option) in options.iter().enumerate() {
            let selected = index == selected_index;
            menu = menu.with_child(
                UiNode::new(format!("{id}.option.{}", option.value), UiNodeKind::Button)
                    .with_class(if selected {
                        "inspector-dropdown-option-selected"
                    } else {
                        "inspector-dropdown-option"
                    })
                    .with_layout(UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill))
                    .with_text_key(option.label_key.clone())
                    .with_text_style(UiTextStyle::body(if selected {
                        palette.tokens().text
                    } else {
                        palette.tokens().text_muted
                    }))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_accessibility_role(raf_ui::UiAccessibilityRole::Option)
                    .with_accessibility_selected(selected)
                    .focusable()
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: raf_ui::UiAction::SetSelect {
                            key: value_key.clone(),
                            value: option.value.clone(),
                            index,
                        },
                    })
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: raf_ui::UiAction::SetSelectOpen {
                            id: trigger_id.clone(),
                            open: false,
                        },
                    }),
            );
        }
        field = field.with_child(menu);
    }
    field
}

const COLOR_PICKER_POPOVER_PADDING: f32 = 10.0;
const COLOR_PICKER_POPOVER_GAP: f32 = 7.0;
const COLOR_PICKER_VISUAL_SIZE: f32 = 220.0;
const COLOR_PICKER_IMAGE_SIZE: f32 = UiColorPicker::CANVAS_SIZE;
const COLOR_PICKER_SQUARE_START: f32 = UiColorPicker::SQUARE_START;
const COLOR_PICKER_SQUARE_SIDE: f32 = UiColorPicker::SQUARE_SIDE;
const COLOR_PICKER_RING_RADIUS: f32 =
    (UiColorPicker::RING_INNER_RADIUS + UiColorPicker::RING_OUTER_RADIUS) * 0.5;
const COLOR_PICKER_POPOVER_WIDTH: f32 = 304.0;
const COLOR_PICKER_POPOVER_HEIGHT: f32 = 382.0;

fn color_hex(color: NodeColor) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.r, color.g, color.b, color.a
    )
}

fn color_picker(palette: StudioUiPalette, color: NodeColor, open: bool) -> UiNode {
    let tokens = palette.tokens();
    let hex = color_hex(color);
    let mut picker = UiNode::new("inspector.color", UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    let mut trigger = UiNode::new("inspector.color.trigger", UiNodeKind::Button)
        .with_class("inspector-color-trigger")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 9.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.color.swatch", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(44.0, 22.0))
                .with_style(UiStyle {
                    fill: [color.r, color.g, color.b, color.a.max(32)],
                    border: tokens.border,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 3.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("inspector.color.hex", UiNodeKind::Label)
                .with_text_value(hex)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_child(
            UiNode::new("inspector.color.chevron", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if open {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(raf_render::api_graphic_basic::ui_surface::UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "inspector.color.toggle",
        ));
    if open {
        trigger = trigger.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "inspector.color.toggle",
        ));
    }
    picker = picker.with_child(trigger);
    if open {
        let mut popover = UiNode::new("inspector.color.popover", UiNodeKind::Menu)
            .with_class("inspector-color-popover")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_style(UiStyle {
                fill: tokens.surface_raised,
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 5.0,
                opacity: 1.0,
            })
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: COLOR_PICKER_POPOVER_GAP,
                padding: UiSpacing::same(COLOR_PICKER_POPOVER_PADDING),
                ..UiLayout::absolute(UiRect::new(
                    0.0,
                    36.0,
                    COLOR_PICKER_POPOVER_WIDTH,
                    COLOR_PICKER_POPOVER_HEIGHT,
                ))
                .with_z_index(24)
            });
        popover = popover.with_child(
            UiNode::text_input(
                "inspector.color.hex.input",
                UiTextInput {
                    value_key: "inspector.color.hex".to_string(),
                    placeholder_key: None,
                    max_length: 9,
                    multiline: false,
                    password: false,
                    submit_command: Some("inspector.color.hex.commit".to_string()),
                },
            )
            .with_class("inspector-color-hex-input")
            .with_layout(UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill))
            .with_text_style(UiTextStyle::body(tokens.text)),
        );
        let hsv = rgb_to_hsv(color);
        popover = popover.with_child(color_picker_visual(palette, hsv));
        for (channel, key, value) in [
            ("r", "app.color_r", color.r),
            ("g", "app.color_g", color.g),
            ("b", "app.color_b", color.b),
        ] {
            popover = popover.with_child(range_labeled(
                palette,
                key,
                &format!("inspector.color.{channel}"),
                value as f32,
                0.0,
                255.0,
                1.0,
            ));
        }
        picker = picker.with_child(popover);
    }
    picker
}

fn color_picker_visual(palette: StudioUiPalette, hsv: crate::color_math::HsvColor) -> UiNode {
    let tokens = palette.tokens();
    let scale = COLOR_PICKER_VISUAL_SIZE / COLOR_PICKER_IMAGE_SIZE;
    let center = COLOR_PICKER_IMAGE_SIZE * 0.5 * scale;
    let ring_radius = COLOR_PICKER_RING_RADIUS * scale;
    let square_start = COLOR_PICKER_SQUARE_START * scale;
    let square_extent = (COLOR_PICKER_SQUARE_SIDE * scale - scale).max(0.0);
    let hue_angle = hsv.hue.to_radians() - std::f32::consts::FRAC_PI_2;
    let hue_marker_center = [
        center + hue_angle.cos() * ring_radius,
        center + hue_angle.sin() * ring_radius,
    ];
    let saturation_marker_center = [
        square_start + hsv.saturation * square_extent,
        square_start + (1.0 - hsv.value) * square_extent,
    ];
    let marker_size = 14.0;
    let visual = UiNode::new("inspector.color.visual", UiNodeKind::Panel)
        .with_class("inspector-color-visual")
        .with_layout(UiLayout {
            align_self: Some(UiAlign::Center),
            ..UiLayout::fixed(COLOR_PICKER_VISUAL_SIZE, COLOR_PICKER_VISUAL_SIZE)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::image(
                "inspector.color.visual.image",
                UiImage {
                    source: UiImageSource::new(format!(
                        "builtin://color-picker/hsv/{:.0}",
                        hsv.hue
                    )),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(
                COLOR_PICKER_VISUAL_SIZE,
                COLOR_PICKER_VISUAL_SIZE,
            )),
        )
        .with_child(
            UiNode::color_picker(
                "inspector.color.visual.surface",
                UiColorPicker::new("inspector.color", hsv.hue, hsv.saturation, hsv.value),
            )
            .with_class("inspector-color-surface")
            .with_layout(
                UiLayout::absolute(UiRect::new(
                    0.0,
                    0.0,
                    COLOR_PICKER_VISUAL_SIZE,
                    COLOR_PICKER_VISUAL_SIZE,
                ))
                .with_z_index(3),
            )
            .with_style(UiStyle::transparent())
            .with_tooltip_key("app.material")
            .with_accessibility_label_key("app.material"),
        );

    let marker_style = |border: [u8; 4], width: f32, radius: f32| UiStyle {
        fill: [0, 0, 0, 0],
        border,
        text: tokens.text,
        border_width: width,
        radius,
        opacity: 1.0,
    };
    visual
        .with_child(
            UiNode::new("inspector.color.visual.sv-marker.outer", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        saturation_marker_center[0] - marker_size * 0.5,
                        saturation_marker_center[1] - marker_size * 0.5,
                        marker_size,
                        marker_size,
                    ))
                    .with_z_index(5),
                )
                .with_style(marker_style([0, 0, 0, 230], 3.0, marker_size * 0.5)),
        )
        .with_child(
            UiNode::new("inspector.color.visual.sv-marker.inner", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        saturation_marker_center[0] - 5.0,
                        saturation_marker_center[1] - 5.0,
                        10.0,
                        10.0,
                    ))
                    .with_z_index(6),
                )
                .with_style(marker_style(tokens.text, 2.0, 5.0)),
        )
        .with_child(
            UiNode::new("inspector.color.visual.hue-marker", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        hue_marker_center[0] - 7.0,
                        hue_marker_center[1] - 7.0,
                        14.0,
                        14.0,
                    ))
                    .with_z_index(6),
                )
                .with_style(marker_style(tokens.text, 2.0, 7.0)),
        )
}

fn variables_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    variables: &[SceneVariable],
) -> UiNode {
    let mut section = UiNode::new("inspector.variables", UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 4.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    for (index, variable) in variables.iter().enumerate() {
        let mut row = UiNode::new(format!("inspector.variable.{index}"), UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 3.0,
                ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
            });
        row = row
            .with_child(text_field(
                &format!("inspector.variable.{index}.name"),
                &format!("inspector.variable.{index}.name"),
                &variable.name,
                palette,
                1.0,
            ))
            .with_child(text_field(
                &format!("inspector.variable.{index}.value"),
                &format!("inspector.variable.{index}.value"),
                &variable_value_text(&variable.value),
                palette,
                1.0,
            ))
            .with_child(
                action_button(
                    palette,
                    &format!("inspector.variable.{index}.type"),
                    "app.variable_type",
                    format!("inspector.variable.type:{}:{index}", id.0),
                )
                .with_text_value(variable.value.type_label()),
            )
            .with_child(action_button(
                palette,
                &format!("inspector.variable.{index}.remove"),
                "app.remove",
                format!("inspector.variable.remove:{}:{index}", id.0),
            ));
        section = section.with_child(row);
    }
    if variables.is_empty() {
        section = section.with_child(
            UiNode::new("inspector.variables.empty", UiNodeKind::Label)
                .with_text_key("app.no_variables")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    section.with_child(action_button(
        palette,
        "inspector.variable.add",
        "app.add_variable",
        format!("inspector.variable.add:{}", id.0),
    ))
}

fn audio_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.audio", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(toggle_row(
            palette,
            "app.audio_enabled",
            "inspector.audio.enabled",
            node.audio_source.enabled,
        ))
        .with_child(text_field(
            "inspector.audio.clip",
            "inspector.audio.clip",
            &node.audio_source.clip,
            palette,
            1.0,
        ))
        .with_child(toggle_row(
            palette,
            "app.audio_autoplay",
            "inspector.audio.autoplay",
            node.audio_source.autoplay,
        ))
        .with_child(toggle_row(
            palette,
            "app.audio_looping",
            "inspector.audio.looping",
            node.audio_source.looping,
        ))
        .with_child(range_labeled(
            palette,
            "app.audio_volume",
            "inspector.audio.volume",
            node.audio_source.volume,
            0.0,
            1.0,
            0.01,
        ))
        .with_child(
            UiNode::new("inspector.audio.id", UiNodeKind::Label)
                .with_text_value(format!("Node {}", id.0))
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn collider_options(current: raf_core::scene::ColliderType) -> (Vec<UiSelectOption>, usize) {
    let kinds = [
        raf_core::scene::ColliderType::None,
        raf_core::scene::ColliderType::Aabb,
        raf_core::scene::ColliderType::ConvexHull,
        raf_core::scene::ColliderType::MeshCollider,
    ];
    let selected_index = kinds.iter().position(|kind| *kind == current).unwrap_or(0);
    let options = kinds
        .into_iter()
        .map(|kind| {
            UiSelectOption::new(
                kind_label(kind).to_ascii_lowercase(),
                collider_label_key(kind),
            )
        })
        .collect();
    (options, selected_index)
}

fn body_type_options(current: raf_core::scene::RigidBodyType) -> (Vec<UiSelectOption>, usize) {
    let kinds = [
        raf_core::scene::RigidBodyType::Static,
        raf_core::scene::RigidBodyType::Dynamic,
        raf_core::scene::RigidBodyType::Kinematic,
    ];
    let selected_index = kinds.iter().position(|kind| *kind == current).unwrap_or(0);
    let options = kinds
        .into_iter()
        .map(|kind| {
            UiSelectOption::new(
                body_type_label(kind).to_ascii_lowercase(),
                body_type_label_key(kind),
            )
        })
        .collect();
    (options, selected_index)
}

fn physics_section(
    palette: StudioUiPalette,
    _id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
    open_dropdown: Option<InspectorDropdown>,
    display_unit: DisplayUnit,
) -> UiNode {
    let (collider_options, collider_selected_index) = collider_options(node.collider.collider_type);
    let (body_options, body_selected_index) = body_type_options(node.rigid_body.body_type);
    UiNode::new("inspector.physics", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(toggle_row(
            palette,
            "app.enable_rigidbody",
            "inspector.physics.enabled",
            node.rigid_body.enabled,
        ))
        .with_child(toggle_row(
            palette,
            "app.use_gravity",
            "inspector.physics.gravity",
            node.rigid_body.use_gravity,
        ))
        .with_child(toggle_row(
            palette,
            "app.is_trigger",
            "inspector.physics.trigger",
            node.rigid_body.is_trigger,
        ))
        .with_child(dropdown_field(
            palette,
            "inspector.collider",
            "app.collider_type",
            InspectorDropdown::Collider,
            open_dropdown,
            collider_options,
            collider_selected_index,
        ))
        .with_child(dropdown_field(
            palette,
            "inspector.body-type",
            "app.body_type",
            InspectorDropdown::BodyType,
            open_dropdown,
            body_options,
            body_selected_index,
        ))
        .with_child(range_labeled(
            palette,
            "app.physics_damping",
            "inspector.physics.damping",
            node.rigid_body.damping,
            0.0,
            1.0,
            0.01,
        ))
        .with_child(vector_row(
            palette,
            "velocity",
            node.rigid_body.velocity,
            display_unit,
        ))
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(format!("inspector.label.{key}"), UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fit_content())
}

fn range_labeled(
    palette: StudioUiPalette,
    label_key: &str,
    value_key: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) -> UiNode {
    UiNode::new(format!("{value_key}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, label_key))
        .with_child(
            UiNode::range(value_key, UiRange::new(value_key, value, min, max, step))
                .with_class("inspector-range")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [54.0, 26.0],
                    ..UiLayout::fixed(0.0, 26.0)
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::PointerUp(raf_ui::UiPointerButton::Primary),
                    "inspector.range.end",
                )),
        )
        .with_child(
            UiNode::new(format!("{value_key}.value"), UiNodeKind::Label)
                .with_class("inspector-range-value")
                .with_text_value(
                    if value_key.ends_with(".a") || value_key.ends_with(".opacity") {
                        format!("{:.0}%", value / max.max(1.0) * 100.0)
                    } else {
                        format!("{value:.0}")
                    },
                )
                .with_text_overflow(UiTextOverflow::Clip)
                .with_text_style(UiTextStyle::button(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(42.0, 24.0).with_text_safe_area(true)),
        )
}

fn toggle_row(palette: StudioUiPalette, label_key: &str, value_key: &str, value: bool) -> UiNode {
    UiNode::new(format!("{value_key}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, label_key).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }))
        .with_child(
            UiNode::toggle(value_key, UiToggle::new(value_key, value))
                .with_class("inspector-toggle")
                .with_layout(UiLayout::fixed(36.0, 24.0))
                .focusable()
                .with_event(UiEventBinding {
                    event: UiEventKind::Click,
                    action: raf_ui::UiAction::SetToggle {
                        key: value_key.to_string(),
                        value: !value,
                    },
                }),
        )
}

fn text_field(
    id: &str,
    value_key: &str,
    value: &str,
    palette: StudioUiPalette,
    grow: f32,
) -> UiNode {
    let _ = value;
    UiNode::text_input(
        id,
        UiTextInput {
            value_key: value_key.to_string(),
            placeholder_key: Some("app.name".to_string()),
            max_length: 256,
            multiline: false,
            password: false,
            submit_command: None,
        },
    )
    .with_class("inspector-input")
    .with_layout(UiLayout {
        grow,
        min_size: [48.0, 28.0],
        ..UiLayout::fixed(0.0, 28.0)
    })
    .with_text_style(UiTextStyle::body(palette.tokens().text))
}

fn variable_value_text(value: &VariableValue) -> String {
    match value {
        VariableValue::Bool(value) => value.to_string(),
        VariableValue::Number(value) => value.to_string(),
        VariableValue::Text(value) => value.clone(),
    }
}

fn kind_label(kind: raf_core::scene::ColliderType) -> &'static str {
    match kind {
        raf_core::scene::ColliderType::None => "None",
        raf_core::scene::ColliderType::Aabb => "Aabb",
        raf_core::scene::ColliderType::ConvexHull => "ConvexHull",
        raf_core::scene::ColliderType::MeshCollider => "MeshCollider",
    }
}

fn collider_label_key(kind: raf_core::scene::ColliderType) -> &'static str {
    match kind {
        raf_core::scene::ColliderType::None => "app.collider_none",
        raf_core::scene::ColliderType::Aabb => "app.collider_aabb",
        raf_core::scene::ColliderType::ConvexHull => "app.collider_convex_hull",
        raf_core::scene::ColliderType::MeshCollider => "app.collider_mesh",
    }
}

fn body_type_label(kind: raf_core::scene::RigidBodyType) -> &'static str {
    match kind {
        raf_core::scene::RigidBodyType::Static => "Static",
        raf_core::scene::RigidBodyType::Dynamic => "Dynamic",
        raf_core::scene::RigidBodyType::Kinematic => "Kinematic",
    }
}

fn body_type_label_key(kind: raf_core::scene::RigidBodyType) -> &'static str {
    match kind {
        raf_core::scene::RigidBodyType::Static => "app.body_static",
        raf_core::scene::RigidBodyType::Dynamic => "app.body_dynamic",
        raf_core::scene::RigidBodyType::Kinematic => "app.body_kinematic",
    }
}

fn primitive_label_key(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Empty => "app.primitive_empty",
        Primitive::Cube => "app.primitive_cube",
        Primitive::Sphere => "app.primitive_sphere",
        Primitive::Plane => "app.primitive_plane",
        Primitive::Cylinder => "app.primitive_cylinder",
    }
}

fn action_button(palette: StudioUiPalette, id: &str, text_key: &str, command: String) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("inspector-button")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [80.0, 28.0],
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn transform_actions(palette: StudioUiPalette, id: SceneNodeId) -> UiNode {
    UiNode::new("inspector.transform.actions", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(action_button(
            palette,
            "inspector.reset-transform",
            "app.reset_transform",
            format!("inspector.reset-transform:{id}", id = id.0),
        ))
        .with_child(action_button(
            palette,
            "inspector.reset-all",
            "app.reset_all",
            format!("inspector.reset-all:{id}", id = id.0),
        ))
}

fn icon_button(
    _palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    command: &str,
    tooltip: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("inspector-icon-button")
        .with_layout(UiLayout::fixed(24.0, 26.0))
        .with_icon(UiIcon::new(icon))
        .with_tooltip_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(crate) fn inspector_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            class_rule(
                "inspector-root",
                with_alpha(tokens.surface, 238),
                with_alpha(tokens.border, 180),
                tokens.text,
            ),
            class_rule(
                "inspector-header",
                with_alpha(tokens.surface_raised, 238),
                with_alpha(tokens.border, 190),
                tokens.text,
            ),
            class_rule("inspector-tabs", [0, 0, 0, 0], [0, 0, 0, 0], tokens.text),
            class_rule(
                "inspector-tab",
                with_alpha(tokens.surface_alt, 170),
                with_alpha(tokens.border, 150),
                tokens.text_muted,
            ),
            class_rule(
                "inspector-tab-active",
                with_alpha(tokens.surface_raised, 238),
                tokens.accent,
                tokens.text,
            ),
            class_rule(
                "inspector-content",
                with_alpha(tokens.surface, 220),
                with_alpha(tokens.border, 150),
                tokens.text,
            ),
            class_rule(
                "inspector-sessions-content",
                with_alpha(tokens.surface, 210),
                with_alpha(tokens.border, 140),
                tokens.text,
            ),
            class_rule(
                "inspector-sessions-controls",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-sessions-heading",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text_muted,
            ),
            class_rule(
                "inspector-session-create-row",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-session-list",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-session-row",
                with_alpha(tokens.surface_alt, 195),
                with_alpha(tokens.border, 165),
                tokens.text,
            ),
            class_rule(
                "inspector-session-active",
                with_alpha(tokens.surface_raised, 238),
                tokens.accent,
                tokens.text,
            ),
            class_rule(
                "inspector-session-identity",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-session-kind",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text_muted,
            ),
            class_rule(
                "inspector-session-active-label",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.accent,
            ),
            class_rule(
                "inspector-session-create",
                palette.accent_style().fill,
                palette.accent_style().border,
                palette.accent_style().text,
            ),
            class_rule(
                "inspector-input",
                with_alpha(tokens.surface_alt, 205),
                with_alpha(tokens.border, 180),
                tokens.text,
            ),
            class_rule(
                "inspector-number-input",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-button",
                with_alpha(tokens.surface_alt, 195),
                with_alpha(tokens.border, 175),
                tokens.text,
            ),
            class_rule(
                "inspector-icon-button",
                with_alpha(tokens.surface_alt, 175),
                with_alpha(tokens.border, 160),
                tokens.text_muted,
            ),
            class_rule(
                "inspector-dropdown-trigger",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-dropdown-menu",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-dropdown-option",
                tokens.surface_alt,
                [0, 0, 0, 0],
                tokens.text_muted,
            ),
            class_rule(
                "inspector-dropdown-option-selected",
                [116, 67, 24, 72],
                tokens.accent,
                tokens.text,
            ),
            class_rule(
                "inspector-color-trigger",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-color-popover",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-color-hex-input",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-color-visual",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-status-on",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-status-off",
                tokens.surface,
                tokens.border,
                tokens.text_muted,
            ),
            class_rule(
                "inspector-range",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-section-title",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-section-title".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 225)),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-dropdown-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 225)),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-row".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 215)),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 205)),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-create".to_string()),
                UiStylePatch {
                    fill: Some(palette.accent_style().border),
                    border: Some(palette.accent_style().border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-icon-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-create".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface, 150)),
                    border: Some(with_alpha(tokens.border, 110)),
                    text: Some(tokens.text_muted),
                    opacity: Some(0.72),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Disabled),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-number-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-color-hex-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-color-surface".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

fn with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(3.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}
