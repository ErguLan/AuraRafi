//! Native RafUI New Project form.
//!
//! Project creation remains an application transaction; this module only
//! owns the declarative form and translates typed surface actions.

use std::path::PathBuf;

use raf_core::config::Language;
use raf_core::project::ProjectType;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleSelector, UiStyleSheet,
    UiTextInput, UiTextStyle,
};

use crate::editor_layout::EditorRect;
use crate::folder_picker;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewProjectSurfaceAction {
    SetName(String),
    SetPath(String),
    ChoosePath,
    SetType(ProjectType),
    Cancel,
    Create,
}

pub struct NewProjectSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    name: String,
    path: String,
    project_type: ProjectType,
}

impl NewProjectSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
        path: PathBuf,
    ) -> Self {
        let path = path.to_string_lossy().to_string();
        let mut host = graphics.create_ui_host(
            build_new_project_surface(palette, "", &path, ProjectType::Game),
            [0, 0, 0, 0],
        );
        host.session_mut()
            .interaction
            .controls
            .set_text("new-project.name", "", 256);
        host.session_mut()
            .interaction
            .controls
            .set_text("new-project.path", &path, 1024);
        Self {
            region: InputRegionId::from_static("native.editor.new-project"),
            rect,
            host,
            name: String::new(),
            path,
            project_type: ProjectType::Game,
        }
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        language: Language,
    ) -> Vec<NewProjectSurfaceAction> {
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            InputOwner::RetainedUi(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        let mut output = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "new-project.name" => {
                    self.name = value.clone();
                    output.push(NewProjectSurfaceAction::SetName(value));
                }
                UiAction::SetText { key, value } if key == "new-project.path" => {
                    self.path = value.clone();
                    output.push(NewProjectSurfaceAction::SetPath(value));
                }
                UiAction::Command { name } => match name.as_str() {
                    "new-project.choose-path" => {
                        if let Some(path) = folder_picker::pick_folder(&self.path) {
                            self.path = path.to_string_lossy().to_string();
                            self.host.session_mut().interaction.controls.set_text(
                                "new-project.path",
                                &self.path,
                                1024,
                            );
                            output.push(NewProjectSurfaceAction::ChoosePath);
                        }
                    }
                    "new-project.game" => {
                        self.project_type = ProjectType::Game;
                        output.push(NewProjectSurfaceAction::SetType(ProjectType::Game));
                    }
                    "new-project.electronics" => {
                        self.project_type = ProjectType::Electronics;
                        output.push(NewProjectSurfaceAction::SetType(ProjectType::Electronics));
                    }
                    "new-project.cancel" => output.push(NewProjectSurfaceAction::Cancel),
                    "new-project.create" => output.push(NewProjectSurfaceAction::Create),
                    _ => {}
                },
                _ => {}
            }
        }
        output
    }

    pub fn set_rect(&mut self, rect: EditorRect) {
        self.rect = rect;
    }

    pub fn set_surface(&mut self, palette: StudioUiPalette) {
        self.host.set_surface(build_new_project_surface(
            palette,
            &self.name,
            &self.path,
            self.project_type,
        ));
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }
}

pub fn build_new_project_surface(
    palette: StudioUiPalette,
    _name: &str,
    _path: &str,
    project_type: ProjectType,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("new-project.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("new-project.card", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 10.0,
                    padding: UiSpacing::xy(24.0, 22.0),
                    ..UiLayout::fixed(540.0, 340.0)
                })
                .with_style(UiStyle {
                    fill: tokens.surface,
                    border: tokens.border,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 8.0,
                    opacity: 1.0,
                })
                .with_child(
                    UiNode::new("new-project.title", UiNodeKind::Label)
                        .with_text_key(match project_type {
                            ProjectType::Game => "app.new_game_project",
                            ProjectType::Electronics => "app.new_electronics_project",
                        })
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::text_input(
                        "new-project.name",
                        UiTextInput {
                            value_key: "new-project.name".to_string(),
                            placeholder_key: Some("app.hub_project_name_placeholder".to_string()),
                            max_length: 256,
                            multiline: false,
                            password: false,
                            submit_command: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(
                    UiNode::text_input(
                        "new-project.path",
                        UiTextInput {
                            value_key: "new-project.path".to_string(),
                            placeholder_key: Some("app.choose_location".to_string()),
                            max_length: 1024,
                            multiline: false,
                            password: false,
                            submit_command: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(
                    UiNode::new("new-project.type", UiNodeKind::Toolbar)
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            gap: 6.0,
                            ..UiLayout::fit_content()
                        })
                        .with_child(type_button(
                            "new-project.game",
                            "app.hub_game_kind",
                            project_type == ProjectType::Game,
                        ))
                        .with_child(type_button(
                            "new-project.electronics",
                            "app.hub_electronics_kind",
                            project_type == ProjectType::Electronics,
                        )),
                )
                .with_child(
                    UiNode::new("new-project.actions", UiNodeKind::Toolbar)
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            justify_content: UiJustify::End,
                            gap: 8.0,
                            ..UiLayout::fixed(0.0, 40.0).with_width_mode(UiSizeMode::Fill)
                        })
                        .with_child(command_button(
                            "new-project.choose-path",
                            "app.choose_location",
                        ))
                        .with_child(command_button("new-project.cancel", "app.cancel"))
                        .with_child(command_button("new-project.create", "app.create_project")),
                ),
        );
    let mut surface = UiSurface::new("new-project", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-active".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-command".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("new-project-command".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Focused),
        ],
    };
    surface
}

fn type_button(id: &str, text: &str, active: bool) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active { "new-project-active" } else { "" })
        .with_text_value(text.to_string())
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id))
}

fn command_button(id: &str, text: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("new-project-command")
        .with_text_value(text.to_string())
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id))
}
