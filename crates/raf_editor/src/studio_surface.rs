//! Retained native-studio surface blueprint.
//!
//! This is intentionally a renderer-independent editor shell definition. The
//! temporary egui panels remain the production adapter while the native WGPU
//! host takes ownership of concrete panel content one surface at a time.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind,
    UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiSurface,
};

pub fn build_studio_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let control_hover = match palette {
        StudioUiPalette::IndustrialDark => [44, 44, 46, 255],
        StudioUiPalette::PaperLight => [224, 224, 226, 255],
    };
    let root = UiNode::new("studio.root", UiNodeKind::Root)
        .with_class("studio-root")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(toolbar())
        .with_child(workspace())
        .with_child(bottom_dock());

    let mut surface = UiSurface::new("studio.editor", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("studio-root".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("dock-panel".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("canvas".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(control_hover),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("accent-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(match palette {
                        StudioUiPalette::IndustrialDark => [18, 18, 20, 255],
                        StudioUiPalette::PaperLight => [255, 255, 255, 255],
                    }),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("accent-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("dock-tab".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("dock-tab".to_string()),
                UiStylePatch {
                    fill: Some(control_hover),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    };
    surface
}

fn toolbar() -> UiNode {
    UiNode::new("studio.toolbar", UiNodeKind::Toolbar)
        .with_class("toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            padding: UiSpacing::same(8.0),
            gap: 6.0,
            ..UiLayout::fixed(0.0, 36.0)
        })
        .with_child(command_button(
            "studio.file",
            "app.file",
            "editor.menu.file",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.edit",
            "app.studio_edit",
            "editor.menu.edit",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.view",
            "app.studio_view",
            "editor.menu.view",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.project",
            "app.studio_project",
            "editor.menu.project",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.run",
            "app.studio_run",
            "editor.runtime.prepared",
            "accent-button",
        ))
}

fn workspace() -> UiNode {
    UiNode::new("studio.workspace", UiNodeKind::DockArea)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_child(dock_panel(
            "studio.hierarchy",
            "app.hierarchy",
            UiLayout::fixed(230.0, 0.0),
        ))
        .with_child(
            UiNode::new("studio.viewport", UiNodeKind::Canvas)
                .with_class("canvas")
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::None)
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "viewport.focus",
                )),
        )
        .with_child(dock_panel(
            "studio.inspector",
            "app.properties",
            UiLayout::fixed(300.0, 0.0),
        ))
}

fn bottom_dock() -> UiNode {
    UiNode::new("studio.bottom", UiNodeKind::Panel)
        .with_class("dock-panel")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(6.0),
            gap: 4.0,
            ..UiLayout::fixed(0.0, 190.0)
        })
        .with_child(
            UiNode::new("studio.bottom-tabs", UiNodeKind::Toolbar)
                .with_class("toolbar")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 4.0,
                    ..UiLayout::fixed(0.0, 28.0)
                })
                .with_child(command_button(
                    "studio.console",
                    "app.studio_console",
                    "bottom.console",
                    "dock-tab",
                ))
                .with_child(command_button(
                    "studio.assets",
                    "app.studio_assets",
                    "bottom.assets",
                    "dock-tab",
                ))
                .with_child(command_button(
                    "studio.node-editor",
                    "app.studio_node_editor",
                    "bottom.nodes",
                    "dock-tab",
                ))
                .with_child(command_button(
                    "studio.agent",
                    "app.agent_tab",
                    "bottom.agent",
                    "dock-tab",
                )),
        )
        .with_child(
            UiNode::new("studio.bottom-content", UiNodeKind::Panel)
                .with_class("canvas")
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::None)
                }),
        )
}

fn dock_panel(id: &str, title_key: &str, layout: UiLayout) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("dock-panel")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(8.0),
            gap: 6.0,
            ..layout
        })
        .with_child(
            UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
}

fn command_button(id: &str, text_key: &str, command: &str, class_name: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class_name)
        .with_text_key(text_key)
        .with_layout(UiLayout::fixed(82.0, 24.0))
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::Command {
                name: command.to_string(),
            },
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_surface_has_fixed_docks_and_a_growing_canvas() {
        let surface = build_studio_surface(StudioUiPalette::IndustrialDark);
        let frame = surface.build_frame(1440, 900, [8, 8, 8, 255]);

        let viewport = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "studio.viewport")
            .expect("viewport box");
        let hierarchy = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "studio.hierarchy")
            .expect("hierarchy box");
        let inspector = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "studio.inspector")
            .expect("inspector box");

        assert_eq!(hierarchy.rect.width, 230.0);
        assert_eq!(inspector.rect.width, 300.0);
        assert!(viewport.rect.width > 700.0);
        assert!(frame
            .hit_regions
            .iter()
            .any(|region| region.id == "studio.run"));
    }
}
