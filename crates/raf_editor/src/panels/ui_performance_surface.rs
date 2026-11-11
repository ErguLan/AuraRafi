//! Snapshot-based developer inspection of native UI costs.
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::*;

const INSPECTOR_MAX_SIZE: [f32; 2] = [720.0, 520.0];
const INSPECTOR_LAYER: i16 = 1000;

pub fn build_ui_performance_surface(
    palette: StudioUiPalette,
    snapshot: &UiPerformanceSnapshot,
    selected: Option<usize>,
    size: [f32; 2],
    selected_rect: Option<UiRect>,
) -> UiSurface {
    let tokens = palette.tokens();
    let metrics = UiThemeMetrics::default().for_control(UiControlKind::Button);
    let width = size[0].min(INSPECTOR_MAX_SIZE[0]).max(1.0);
    let height = size[1].min(INSPECTOR_MAX_SIZE[1]).max(1.0);
    let mut panel = UiNode::new("ui.performance.panel", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: metrics.icon_gap,
            padding: UiSpacing::same(metrics.horizontal_padding),
            overflow: UiOverflow::Clip,
            ..UiLayout::absolute(UiRect::new((size[0] - width).max(0.0), 0.0, width, height))
                .with_z_index(INSPECTOR_LAYER)
        })
        .with_style(palette.panel_style())
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("ui.performance.title");
    let mut header = UiNode::row("ui.performance.header", [])
        .with_layout(UiLayout::fill(UiFlow::Row).fixed_height(metrics.height.max(32.0)))
        .with_child(
            UiNode::new("ui.performance.title", UiNodeKind::Label)
                .with_text_key("ui.performance.title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content().fill_width()),
        );
    for (id, icon) in [
        ("refresh", UiIconId::Refresh),
        ("clear", UiIconId::Trash),
        ("close", UiIconId::Close),
    ] {
        let key = format!("ui.performance.{id}");
        header = header.with_child(icon_button_with_icon(&key, icon, &key, &key));
    }
    panel = panel
        .with_child(header)
        .with_child(
            UiNode::new("ui.performance.hint", UiNodeKind::Label)
                .with_text_key("ui.performance.snapshot_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content().fill_width()),
        )
        .with_child(
            UiNode::new("ui.performance.columns", UiNodeKind::Label)
                .with_text_key("ui.performance.columns")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content().fill_width()),
        );
    let mut list = UiNode::scroll_view("ui.performance.list", UiScrollAxis::Vertical)
        .with_layout(UiLayout::fill(UiFlow::Column));
    for (index, region) in snapshot.regions.iter().enumerate() {
        list = list.with_child(
            UiNode::new(format!("ui.performance.row.{index}"), UiNodeKind::Button)
                .with_text_value(format!(
                    "{}   {:.2} / {:.2}   {} / {}   {}",
                    region.id,
                    region.last_cpu_us as f64 / 1000.0,
                    region.peak_cpu_us as f64 / 1000.0,
                    region.rebuilds,
                    region.reuses,
                    region.slow_observations
                ))
                .with_layout(UiLayout::fixed(0.0, metrics.height).fill_width())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_style(UiStyle {
                    fill: if selected == Some(index) {
                        tokens.selection
                    } else {
                        [0; 4]
                    },
                    text: tokens.text,
                    ..UiStyle::transparent()
                })
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("ui.performance.region:{index}"),
                )),
        );
    }
    panel = panel.with_child(list);
    if let Some(sample) = selected.and_then(|index| {
        snapshot
            .samples
            .iter()
            .filter(|sample| sample.region == index)
            .max_by_key(|sample| sample.cpu_us)
    }) {
        let phase = match sample.phase {
            UiWorkPhase::Document => "document",
            UiWorkPhase::Input => "input",
            UiWorkPhase::Layout => "layout",
            UiWorkPhase::IntrinsicLayout => "intrinsic",
            UiWorkPhase::Text => "text",
            UiWorkPhase::Paint => "paint",
            UiWorkPhase::Submission => "submission",
            UiWorkPhase::ResourceAdmission => "resources",
        };
        let cause = match sample.cause {
            UiWorkCause::Initial => "initial",
            UiWorkCause::Document => "document",
            UiWorkCause::Geometry => "geometry",
            UiWorkCause::Scroll => "scroll",
            UiWorkCause::Text => "text",
            UiWorkCause::Visual => "visual",
            UiWorkCause::Resources => "resources",
            UiWorkCause::Animation => "animation",
            UiWorkCause::Explicit => "explicit",
        };
        panel = panel.with_child(
            UiNode::row(
                "ui.performance.detail",
                [
                    UiNode::new("ui.performance.phase", UiNodeKind::Label)
                        .with_text_key(format!("ui.performance.phase.{phase}"))
                        .with_text_style(UiTextStyle::body(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                    UiNode::new("ui.performance.cause", UiNodeKind::Label)
                        .with_text_key(format!("ui.performance.cause.{cause}"))
                        .with_text_style(UiTextStyle::body(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                    UiNode::new("ui.performance.cost", UiNodeKind::Label)
                        .with_text_value(format!("{} / {}", sample.nodes, sample.bytes))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                ],
            )
            .with_layout(UiLayout::fit_content().fill_width()),
        );
    }
    let mut root = UiNode::new("ui.performance.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None));
    if let Some(rect) = selected_rect.filter(|_| selected.is_some()) {
        root = root.with_child(
            UiNode::new("ui.performance.outline", UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(rect).with_z_index(INSPECTOR_LAYER - 1))
                .with_style(UiStyle {
                    border: tokens.focus,
                    border_width: 2.0,
                    ..UiStyle::transparent()
                }),
        );
    }
    UiSurface::new("ui.performance", palette, root.with_child(panel))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_cost_guard_inspector_uses_retained_localized_controls() {
        let surface = build_ui_performance_surface(
            StudioUiPalette::IndustrialDark,
            &UiPerformanceSnapshot::default(),
            None,
            [640.0, 400.0],
            None,
        );
        for command in ["refresh", "clear", "close"] {
            let id = format!("ui.performance.{command}");
            let node = surface.root.find(&id).unwrap();
            assert_eq!(node.tooltip_key.as_deref(), Some(id.as_str()));
            assert!(node.focusable);
        }
        let mut session = raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text_at_scale(
            &surface,
            640,
            400,
            [0; 4],
            1.25,
            str::to_owned,
        );
        assert!(
            frame
                .layout_boxes
                .iter()
                .find(|b| b.id == "ui.performance.panel")
                .unwrap()
                .rect
                .right()
                <= 640.0
        );
    }
}
