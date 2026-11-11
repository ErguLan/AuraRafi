//! Interface authoring uses the live viewport, Hierarchy and Properties.
//! The dock is an entry point; an isolated artboard is an optional view.
use super::interface_surface_host::InterfaceSurfaceHost;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::*;
#[path = "interface_surface/canvas.rs"]
mod canvas;
#[path = "interface_surface/chrome.rs"]
mod chrome;
#[path = "interface_surface/inspector.rs"]
mod inspector;
#[path = "interface_surface/navigator.rs"]
mod navigator;

pub use canvas::build as build_interface_canvas;
pub use canvas::{insert_menu as build_interface_insert_menu, INSERT_MENU_SIZE};
pub use inspector::build as build_interface_inspector;
pub use navigator::build as build_interface_hierarchy;

pub fn build_interface_surface(
    palette: StudioUiPalette,
    host: &InterfaceSurfaceHost,
    size: [f32; 2],
) -> UiSurface {
    use chrome::*;
    let t = tokens(palette);
    let mut actions = row("ui.designer.dock.actions");
    actions.layout.flow = UiFlow::RowWrap;
    for (id, key, icon, command) in [
        ("open", "ui.workspace.open_live", UiIconId::Pencil, "open"),
        (
            "canvas",
            "ui.workspace.open_canvas",
            UiIconId::Frame,
            "view:canvas",
        ),
        (
            "export",
            "ui.designer.export",
            UiIconId::ExternalLink,
            "export",
        ),
    ] {
        actions.children.push(button(
            format!("ui.designer.dock.{id}"),
            key,
            format!("ui.designer.{command}"),
            Some(icon),
            false,
            t,
        ));
    }
    let root = column("ui.designer.dock")
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
        .with_child(label(
            "ui.designer.dock.title",
            "ui.workspace.dock_title",
            t,
        ))
        .with_child(value("ui.designer.dock.document", &host.document.name, t))
        .with_child(actions)
        .with_child(hint(
            "ui.designer.dock.hint",
            "ui.workspace.dock_hint",
            size[0] - INSET * 2.0,
            t,
        ));
    surface("editor.interface.dock", palette, root)
}

pub fn build_interface_entry(palette: StudioUiPalette, width: f32) -> UiSurface {
    use chrome::*;
    let t = tokens(palette);
    let node = button(
        "ui.designer.entry",
        "ui.designer.title",
        "ui.designer.open",
        Some(UiIconId::Frame),
        false,
        t,
    )
    .with_layout(UiLayout::absolute(UiRect::new(
        (width - 124.0).max(0.0),
        if width < 528.0 { 54.0 } else { 8.0 },
        116.0_f32.min(width),
        30.0,
    )))
    .with_style(UiStyle {
        fill: t.surface,
        text: t.text,
        radius: 4.0,
        ..UiStyle::transparent()
    });
    surface(
        "editor.interface.entry",
        palette,
        UiNode::new("ui.designer.entry.root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_child(node),
    )
}

/// Actual authoring surfaces composed together for renderer-based offscreen QA.
pub fn build_interface_workspace(
    palette: StudioUiPalette,
    host: &InterfaceSurfaceHost,
    size: [f32; 2],
) -> UiSurface {
    let left = (size[0] * 0.19).clamp(150.0, 248.0);
    let right = (size[0] * 0.24).clamp(232.0, 304.0);
    let center = (size[0] - left - right).max(1.0);
    let mut root = UiNode::new("ui.designer.qa.workspace", UiNodeKind::Root)
        .with_layout(UiLayout::fixed(size[0], size[1]));
    let mut rules = Vec::new();
    for (surface, rect) in [
        (
            build_interface_hierarchy(palette, host, [left, size[1]]),
            UiRect::new(0.0, 0.0, left, size[1]),
        ),
        (
            build_interface_canvas(palette, host, [center, size[1]]),
            UiRect::new(left, 0.0, center, size[1]),
        ),
        (
            build_interface_inspector(palette, host, [right, size[1]]),
            UiRect::new(left + center, 0.0, right, size[1]),
        ),
    ] {
        rules.extend(surface.style_sheet.rules);
        root.children.push(
            UiNode::new(format!("{}.host", surface.id), UiNodeKind::Root)
                .with_layout(UiLayout {
                    overflow: UiOverflow::Clip,
                    ..UiLayout::absolute(rect)
                })
                .with_child(surface.root),
        );
    }
    UiSurface::new("editor.interface.workspace", palette, root)
        .with_style_sheet(UiStyleSheet { rules })
}
