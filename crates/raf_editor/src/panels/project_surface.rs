//! Retained RafUI document for the project browser panel.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiFlow, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiSurface, UiTextStyle,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTreeEntry {
    pub label: String,
    pub icon: UiIconId,
    pub depth: u8,
}

pub fn build_project_surface(
    palette: StudioUiPalette,
    project_name: &str,
    session_name: &str,
    entries: &[ProjectTreeEntry],
) -> UiSurface {
    let tokens = palette.tokens();
    let mut tree = UiNode::scroll_view("project.tree", UiScrollAxis::Vertical)
        .with_class("project-tree")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 6.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("project.root", UiNodeKind::Label)
                .with_class("project-root-row")
                .with_text_value(project_name.to_string())
                .with_icon(UiIcon::new(UiIconId::Project).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(6.0, 3.0),
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        );

    if !session_name.trim().is_empty() {
        tree = tree.with_child(
            UiNode::new("project.session", UiNodeKind::Label)
                .with_class("project-session-row")
                .with_text_value(session_name.to_string())
                .with_icon(UiIcon::new(UiIconId::Scene).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing {
                        left: 20.0,
                        right: 6.0,
                        top: 3.0,
                        bottom: 3.0,
                    },
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }

    for (index, entry) in entries.iter().enumerate() {
        tree = tree.with_child(
            UiNode::new(format!("project.entry.{index}"), UiNodeKind::Label)
                .with_class("project-tree-row")
                .with_text_value(entry.label.clone())
                .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing {
                        left: 6.0 + f32::from(entry.depth) * 14.0,
                        right: 6.0,
                        top: 2.0,
                        bottom: 2.0,
                    },
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }

    let root = UiNode::new("project.root-surface", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(palette.panel_style())
        .with_child(
            UiNode::new("project.toolbar", UiNodeKind::Toolbar)
                .with_class("project-toolbar")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(8.0, 3.0),
                    ..UiLayout::fixed(0.0, 31.0)
                })
                .with_text_key("app.studio_project")
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted)),
        )
        .with_child(tree);

    let mut surface = UiSurface::new("editor.bottom.project", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child<'a>(node: &'a UiNode, id: &str) -> &'a UiNode {
        if node.id == id {
            return node;
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
            .unwrap_or_else(|| panic!("missing node {id}"))
    }

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
    }

    #[test]
    fn project_hierarchy_uses_padding_not_leading_spaces() {
        let surface = build_project_surface(
            StudioUiPalette::IndustrialDark,
            "HELLO WORLD",
            "Main",
            &[ProjectTreeEntry {
                label: "scene.ron".to_string(),
                icon: UiIconId::Scene,
                depth: 1,
            }],
        );
        let session = child(&surface.root, "project.session");
        let entry = child(&surface.root, "project.entry.0");

        assert_eq!(session.text_value.as_deref(), Some("Main"));
        assert_eq!(session.layout.padding.left, 20.0);
        assert_eq!(entry.layout.padding.left, 20.0);
        assert_eq!(
            child(&surface.root, "project.root").layout.width_mode,
            UiSizeMode::Fill
        );
        assert_eq!(
            child(&surface.root, "project.root").layout.height_mode,
            UiSizeMode::FitContent
        );
    }
}
