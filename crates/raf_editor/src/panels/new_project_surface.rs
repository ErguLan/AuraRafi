//! Retained RafUI surface for the new-project flow.
//!
//! Project creation remains an application transaction. This host only owns
//! the form controls and emits the user's intent.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout,
    UiNode, UiNodeKind, UiSpacing, UiStylePatch, UiStyleRule, UiStyleSelector, UiStyleSheet,
    UiTextInput, UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewProjectSurfaceAction {
    SetName(String),
    SetPath(String),
    Cancel,
    Create,
}

pub struct NewProjectSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for NewProjectSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_new_project"),
        }
    }
}

impl NewProjectSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        name: &str,
        path: &str,
        project_type: ProjectType,
        lang: Language,
    ) -> Vec<NewProjectSurfaceAction> {
        let type_label = match project_type {
            ProjectType::Game => t("app.game_project", lang),
            ProjectType::Electronics => t("app.electronics_project", lang),
        };
        let surface = build_surface(palette, project_type, type_label);
        self.bridge
            .show_with_control_state(
                ui,
                render_state,
                palette,
                surface,
                |controls| {
                    controls.set_text("new-project.name", name.to_string(), 160);
                    controls.set_text("new-project.path", path.to_string(), 512);
                },
                |key| t(key, lang),
            )
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::SetText { key, value } if key == "new-project.name" => {
                    Some(NewProjectSurfaceAction::SetName(value))
                }
                UiAction::SetText { key, value } if key == "new-project.path" => {
                    Some(NewProjectSurfaceAction::SetPath(value))
                }
                UiAction::Command { name } if name == "new-project.cancel" => {
                    Some(NewProjectSurfaceAction::Cancel)
                }
                UiAction::Command { name } if name == "new-project.create" => {
                    Some(NewProjectSurfaceAction::Create)
                }
                _ => None,
            })
            .collect()
    }
}

fn build_surface(
    palette: StudioUiPalette,
    project_type: ProjectType,
    type_label: String,
) -> UiSurface {
    let tokens = palette.tokens();
    let title_key = match project_type {
        ProjectType::Game => "app.new_game_project",
        ProjectType::Electronics => "app.new_electronics_project",
    };

    let mut name_input = UiTextInput::new("new-project.name");
    name_input.placeholder_key = Some("app.project_name".to_string());
    name_input.max_length = 160;
    let mut path_input = UiTextInput::new("new-project.path");
    path_input.placeholder_key = Some("app.location".to_string());
    path_input.max_length = 512;

    let form = UiNode::new("new-project.card", UiNodeKind::Panel)
        .with_class("new-project-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            padding: UiSpacing::same(20.0),
            ..UiLayout::fixed(560.0, 0.0)
        })
        .with_child(
            UiNode::new("new-project.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("new-project.subtitle", UiNodeKind::Label)
                .with_text_key("app.create_project")
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(field_label(palette, "app.project_name"))
        .with_child(
            UiNode::text_input("new-project.name.input", name_input)
                .with_class("new-project-input")
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        )
        .with_child(field_label(palette, "app.location"))
        .with_child(
            UiNode::text_input("new-project.path.input", path_input)
                .with_class("new-project-input")
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        )
        .with_child(field_label(palette, "app.type"))
        .with_child(
            UiNode::new("new-project.type", UiNodeKind::Label)
                .with_text_key(type_label)
                .with_text_style(UiTextStyle::body(tokens.text)),
        )
        .with_child(
            UiNode::new("new-project.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 38.0)
                })
                .with_child(command_button(
                    "new-project.cancel",
                    "app.cancel",
                    "new-project.cancel",
                    "new-project-secondary",
                    palette,
                ))
                .with_child(command_button(
                    "new-project.create",
                    "app.create_project",
                    "new-project-primary",
                    "new-project-primary",
                    palette,
                )),
        );

    let root = UiNode::new("new-project.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            padding: UiSpacing::same(32.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(form);
    let mut surface = UiSurface::new("new-project", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn field_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(format!("new-project.label.{key}"), UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fixed(0.0, 20.0))
}

fn command_button(
    id: &str,
    label_key: &str,
    command: &str,
    class: &str,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [104.0, 32.0],
            padding: UiSpacing::xy(12.0, 6.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-secondary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}
