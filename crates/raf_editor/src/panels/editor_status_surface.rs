//! Retained RafUI document for the editor status bar.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiFlow, UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiSizeMode, UiSpacing, UiStyle, UiSurface, UiTextOverflow, UiTextStyle,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;

fn is_performance_status_item(item: &str) -> bool {
    item.starts_with("CAP: ") || item.starts_with("CAP:") || item.starts_with("FPS: ")
}

pub fn build_status_surface(palette: StudioUiPalette, status: &[String]) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("editor.status.root", UiNodeKind::Toolbar)
        .with_class("editor-status-bar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 0.0,
            padding: UiSpacing::xy(10.0, 0.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(UiStyle {
            fill: [13, 24, 34, 255],
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        });

    for (index, item) in status.iter().enumerate() {
        if index > 0 {
            root = root.with_child(
                UiNode::new(
                    format!("editor.status.separator.{index}"),
                    UiNodeKind::Separator,
                )
                .with_class("status-separator")
                .with_layout(UiLayout::fixed(1.0, 14.0)),
            );
        }
        let performance_item = is_performance_status_item(item);
        let layout = if performance_item {
            // Performance telemetry is patched as paint-only text. Give it the remaining row
            // width instead of the old 76 px track: the telemetry string is
            // deliberately one line, so it can never grow downward and be
            // clipped by the bottom edge of the editor window.
            UiLayout {
                basis: [0.0, 22.0],
                grow: 1.0,
                max_size: [920.0, 22.0],
                min_size: [160.0, 22.0],
                width_mode: UiSizeMode::Auto,
                height_mode: UiSizeMode::Fixed,
                ..UiLayout::default()
            }
        } else {
            UiLayout::fit_content()
        };
        root = root.with_child(
            UiNode::new(format!("editor.status.item.{index}"), UiNodeKind::Label)
                .with_text_value(item.clone())
                .with_layout(layout)
                .with_text_overflow(if performance_item {
                    UiTextOverflow::Clip
                } else {
                    UiTextOverflow::Ellipsis
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    root = root.with_child(
        UiNode::new("editor.status.menu", UiNodeKind::Button)
            .with_class("status-menu")
            .with_layout(UiLayout {
                grow: 1.0,
                justify_content: UiJustify::End,
                ..UiLayout::fit_content()
            })
            .with_icon(UiIcon::new(UiIconId::Menu).with_size(UiIconSize::Small))
            .with_tooltip_key("editor.status.menu")
            .focusable(),
    );

    let mut surface = UiSurface::new("editor.status", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_measure_natural_width_without_growing_below_the_bar() {
        let surface = build_status_surface(
            StudioUiPalette::IndustrialDark,
            &["Long project name".to_string(), "Selected: 1".to_string()],
        );
        let mut session = raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text(
            &surface,
            800,
            28,
            [0, 0, 0, 255],
            str::to_string,
        );
        let project = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "editor.status.item.0")
            .expect("project status item");
        let selected = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "editor.status.item.1")
            .expect("selected status item");

        assert!(project.rect.height <= 28.0);
        assert!(selected.rect.y >= project.rect.y);
        assert!(selected.rect.x >= project.rect.x + project.rect.width);
    }

    #[test]
    fn fps_status_stays_on_one_visible_line_for_paint_only_updates() {
        let surface = build_status_surface(
            StudioUiPalette::IndustrialDark,
            &["Project".to_string(), "CAP:~323 FPS | FPS: 46/240 | VP: 46 | CPU 1.0ms | GPU 0.8ms | P95 1.2ms | D8 | U12K | H0 | Immediate".to_string()],
        );
        let fps = surface
            .root
            .find("editor.status.item.1")
            .expect("performance status item");

        assert_eq!(fps.layout.width_mode, UiSizeMode::Auto);
        assert_eq!(fps.layout.height_mode, UiSizeMode::Fixed);
        assert_eq!(fps.layout.basis, [0.0, 22.0]);
        assert_eq!(fps.layout.min_size, [160.0, 22.0]);
        assert_eq!(fps.text_overflow, UiTextOverflow::Clip);

        let mut session = raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text(
            &surface,
            1200,
            28,
            [0, 0, 0, 255],
            str::to_string,
        );
        let fps_box = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "editor.status.item.1")
            .expect("performance layout box");
        assert!(fps_box.rect.height <= 28.0);
        assert!(fps_box.rect.width >= 160.0);
        let fps_request = frame
            .text_requests
            .iter()
            .find(|request| request.node_id == "editor.status.item.1")
            .expect("performance text request");
        assert_eq!(fps_request.overflow, UiTextOverflow::Clip);
    }
}
