//! Retained RafUI surface for project sessions.
//!
//! The application remains the transaction boundary for activation, document
//! loading, and registry persistence.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::session::{ProjectSessionKind, ProjectSessionRegistry, SessionId};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle,
};
use uuid::Uuid;

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionsSurfaceAction {
    Activate(SessionId),
    Create {
        name: String,
        kind: ProjectSessionKind,
    },
}

pub struct SessionsSurfaceHost {
    bridge: RafUiSurfaceBridge,
    new_name: String,
    new_kind: ProjectSessionKind,
}

impl Default for SessionsSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_sessions"),
            new_name: String::new(),
            new_kind: ProjectSessionKind::World,
        }
    }
}

impl SessionsSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        registry: &ProjectSessionRegistry,
        lang: Language,
    ) -> Vec<SessionsSurfaceAction> {
        let surface = build_sessions_surface(palette, registry, self.new_kind, lang);
        let name = self.new_name.clone();
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("sessions.new-name", name.clone(), 120),
            |key| t(key, lang),
        );
        let mut output = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "sessions.new-name" => {
                    self.new_name = value;
                }
                UiAction::Command { name } => {
                    if let Some(id) = parse_session_id(&name, "sessions.activate") {
                        output.push(SessionsSurfaceAction::Activate(id));
                    } else if let Some(kind) = parse_session_kind(&name) {
                        self.new_kind = kind;
                    } else if name == "sessions.create" {
                        let trimmed = self.new_name.trim();
                        if !trimmed.is_empty() {
                            output.push(SessionsSurfaceAction::Create {
                                name: trimmed.to_string(),
                                kind: self.new_kind,
                            });
                            self.new_name.clear();
                        }
                    }
                }
                _ => {}
            }
        }
        output
    }
}

fn build_sessions_surface(
    palette: StudioUiPalette,
    registry: &ProjectSessionRegistry,
    new_kind: ProjectSessionKind,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut name_input = UiTextInput::new("sessions.new-name");
    name_input.placeholder_key = Some("app.session_name".to_string());
    name_input.max_length = 120;

    let mut session_list = UiNode::scroll_view("sessions.list", UiScrollAxis::Vertical)
        .with_class("sessions-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 5.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    for session in &registry.sessions {
        let active = session.id == registry.active_session;
        session_list = session_list.with_child(
            UiNode::new(
                format!("sessions.item.{}", session.id.0),
                UiNodeKind::Button,
            )
            .with_class(if active {
                "sessions-row-active"
            } else {
                "sessions-row"
            })
            .with_text_key(format!(
                "{} | {}",
                session.name,
                session_kind_label(session.kind, lang)
            ))
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_layout(UiLayout {
                min_size: [0.0, 32.0],
                padding: UiSpacing::xy(10.0, 5.0),
                ..UiLayout::default()
            })
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("sessions.activate:{}", session.id.0),
            )),
        );
    }

    let mut kind_row =
        UiNode::new("sessions.kind-row", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            ..UiLayout::fixed(0.0, 30.0)
        });
    for (index, (kind, key, command)) in [
        (
            ProjectSessionKind::World,
            "app.session_kind_world",
            "sessions.kind:world",
        ),
        (
            ProjectSessionKind::Interface,
            "app.session_kind_interface",
            "sessions.kind:interface",
        ),
        (
            ProjectSessionKind::ElectronicsDesign,
            "app.session_kind_electronics",
            "sessions.kind:electronics",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        kind_row = kind_row.with_child(command_button(
            format!("sessions.kind.{index}"),
            key,
            command.to_string(),
            if kind == new_kind {
                "sessions-kind-active"
            } else {
                "sessions-kind"
            },
            palette,
        ));
    }

    let root = UiNode::new("sessions.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(10.0),
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("sessions.header", UiNodeKind::Toolbar)
                .with_class("sessions-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(0.0, 34.0)
                })
                .with_child(
                    UiNode::new("sessions.title", UiNodeKind::Label)
                        .with_text_key("app.sessions")
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::new("sessions.count", UiNodeKind::Label)
                        .with_text_key(format!(
                            "{}: {}",
                            t("app.sessions", lang),
                            registry.sessions.len()
                        ))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            justify_content: raf_ui::UiJustify::End,
                            ..UiLayout::default()
                        }),
                ),
        )
        .with_child(session_list)
        .with_child(
            UiNode::new("sessions.create-section", UiNodeKind::Panel)
                .with_class("sessions-create")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 5.0,
                    padding: UiSpacing::same(10.0),
                    ..UiLayout::fixed(0.0, 118.0)
                })
                .with_child(
                    UiNode::new("sessions.create-title", UiNodeKind::Label)
                        .with_text_key("app.session_create")
                        .with_text_style(UiTextStyle::body(tokens.text)),
                )
                .with_child(
                    UiNode::text_input("sessions.new-name.input", name_input)
                        .with_class("sessions-input")
                        .with_layout(UiLayout::fixed(0.0, 30.0)),
                )
                .with_child(kind_row)
                .with_child(command_button(
                    "sessions.create".to_string(),
                    "app.session_create",
                    "sessions.create".to_string(),
                    "sessions-create-button",
                    palette,
                )),
        );

    let mut surface = UiSurface::new("sessions", palette, root);
    surface.style_sheet = sessions_style_sheet(palette);
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
        .with_accessibility_label_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [48.0, 26.0],
            padding: UiSpacing::xy(7.0, 3.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn session_kind_label(kind: ProjectSessionKind, lang: Language) -> String {
    let key = match kind {
        ProjectSessionKind::World => "app.session_kind_world",
        ProjectSessionKind::Interface => "app.session_kind_interface",
        ProjectSessionKind::ElectronicsDesign => "app.session_kind_electronics",
    };
    t(key, lang)
}

fn parse_session_kind(name: &str) -> Option<ProjectSessionKind> {
    match name {
        "sessions.kind:world" => Some(ProjectSessionKind::World),
        "sessions.kind:interface" => Some(ProjectSessionKind::Interface),
        "sessions.kind:electronics" => Some(ProjectSessionKind::ElectronicsDesign),
        _ => None,
    }
}

fn parse_session_id(name: &str, prefix: &str) -> Option<SessionId> {
    let raw = name.strip_prefix(prefix)?.strip_prefix(':')?;
    Uuid::parse_str(raw).ok().map(SessionId)
}

fn sessions_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
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
                UiStyleSelector::Class("sessions-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("sessions-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("sessions-row-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("sessions-create".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("sessions-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("sessions-kind".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("sessions-kind-active".to_string()),
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
                UiStyleSelector::Class("sessions-create-button".to_string()),
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
