//! Document navigation in the editor's existing Hierarchy area.
use super::chrome::icon_button;
use super::{chrome::*, InterfaceSurfaceHost};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::*;

pub fn build(palette: StudioUiPalette, host: &InterfaceSurfaceHost, size: [f32; 2]) -> UiSurface {
    let t = tokens(palette);
    let mut root = column("ui.designer.navigator")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(INSET),
            gap: INSET,
            ..UiLayout::fixed(size[0], size[1])
        })
        .with_style(UiStyle {
            fill: t.surface,
            ..UiStyle::transparent()
        })
        .with_child(label("ui.designer.hierarchy.title", "app.hierarchy", t))
        .with_child(
            row("ui.designer.context")
                .with_layout(UiLayout {
                    flow: UiFlow::RowWrap,
                    gap: 4.0,
                    ..UiLayout::fit_content().fixed_width((size[0] - INSET * 2.0).max(1.0))
                })
                .with_child(button(
                    "ui.designer.back",
                    "ui.workspace.scene",
                    "ui.designer.view:scene",
                    Some(UiIconId::Cube),
                    false,
                    t,
                ))
                .with_child(button(
                    "ui.designer.context.ui",
                    "ui.designer.title",
                    "ui.designer.open",
                    Some(UiIconId::Frame),
                    true,
                    t,
                )),
        );
    let mut tree =
        UiNode::scroll_view("ui.designer.tree", UiScrollAxis::Vertical).with_layout(UiLayout {
            gap: 2.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    append(
        &host.document.root,
        &mut tree,
        0,
        host,
        size[0] - INSET * 2.0,
        t,
    );
    root.children.push(tree);
    root.children.push(hint(
        "ui.designer.hierarchy.hint",
        "ui.workspace.tree_hint",
        size[0] - INSET * 2.0,
        t,
    ));
    surface("editor.interface.navigator", palette, root)
}
fn append(
    node: &UiNode,
    parent: &mut UiNode,
    depth: usize,
    host: &InterfaceSurfaceHost,
    width: f32,
    t: UiTokens,
) {
    let mut line = row(format!("ui.designer.tree.line.{}", node.id));
    line.layout = UiLayout::fixed(width, ROW_HEIGHT);
    line.layout.flow = UiFlow::Row;
    line.layout.align_items = UiAlign::Center;
    line.layout.padding.left = depth.min(10) as f32 * 12.0;
    if !node.children.is_empty() {
        line.children.push(icon_button(
            format!("ui.designer.tree.expand.{}", node.id),
            "ui.workspace.expand",
            format!("ui.designer.tree:{}", node.id),
            if host.workspace.collapsed_nodes.contains(&node.id) {
                UiIconId::ChevronRight
            } else {
                UiIconId::ChevronDown
            },
            t,
        ));
    }
    let mut item = button(
        format!("ui.designer.tree.row.{}", node.id),
        "ui.designer.title",
        format!("ui.designer.select:{}", node.id),
        Some(element_icon(node)),
        host.selected == node.id,
        t,
    );
    item.text_key = None;
    item.text_value = Some(if depth == 0 {
        host.document.name.clone()
    } else {
        node.id.clone()
    });
    item.layout.width_mode = UiSizeMode::Fill;
    item.text_overflow = UiTextOverflow::Ellipsis;
    if node.hidden {
        item.style.text = t.text_muted;
    }
    line.children.push(item);
    parent.children.push(line);
    if !host.workspace.collapsed_nodes.contains(&node.id) {
        for child in &node.children {
            append(child, parent, depth + 1, host, width, t);
        }
    }
}
