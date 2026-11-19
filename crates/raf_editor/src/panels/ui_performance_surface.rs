//! Snapshot-based developer inspection of native UI costs.
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::*;

use crate::performance_pressure::{self, PerformanceReadout, PressureLevel};

const INSPECTOR_MAX_SIZE: [f32; 2] = [720.0, 520.0];
const INSPECTOR_LAYER: i16 = 1000;
/// Two component gaps visually separate metric pairs without a divider node.
const MEMORY_PAIR_GAP_SCALE: f32 = 2.0;

pub fn build_ui_performance_surface(
    palette: StudioUiPalette,
    snapshot: &UiPerformanceSnapshot,
    selected: Option<usize>,
    size: [f32; 2],
    selected_rect: Option<UiRect>,
) -> UiSurface {
    build_ui_performance_surface_with_readout(
        palette,
        snapshot,
        selected,
        size,
        selected_rect,
        performance_pressure::last_published(),
    )
}

fn build_ui_performance_surface_with_readout(
    palette: StudioUiPalette,
    snapshot: &UiPerformanceSnapshot,
    selected: Option<usize>,
    size: [f32; 2],
    selected_rect: Option<UiRect>,
    readout: Option<PerformanceReadout>,
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
        .with_child(build_memory_group(palette, readout))
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

/// Compact ledger group: three rows, no scrolling, no new visual language.
fn build_memory_group(palette: StudioUiPalette, readout: Option<PerformanceReadout>) -> UiNode {
    let tokens = palette.tokens();
    let gap = UiThemeMetrics::default()
        .for_control(UiControlKind::Button)
        .icon_gap;
    let (resident, peak, budget, pressure, rejected, upload_rejected) = match readout {
        Some(readout) => {
            let budget = if readout.budget_mb > 0.0 {
                format!("{:.0} MB", readout.budget_mb)
            } else {
                "--".to_string()
            };
            let upload_color = if readout.upload_rejected > 0 {
                pressure_color(readout.upload_pressure, tokens)
            } else {
                tokens.text
            };
            (
                memory_text_value(
                    "ui.performance.memory.resident",
                    format!("{:.1} MB", readout.resident_mb),
                    tokens.text,
                ),
                memory_text_value(
                    "ui.performance.memory.peak",
                    format!("{:.1} MB", readout.peak_mb),
                    tokens.text,
                ),
                memory_text_value("ui.performance.memory.budget", budget, tokens.text),
                memory_key_value(
                    "ui.performance.memory.pressure",
                    pressure_label_key(readout.pressure),
                    pressure_color(readout.pressure, tokens),
                ),
                memory_text_value(
                    "ui.performance.memory.rejected",
                    readout.rejected_reservations.to_string(),
                    tokens.text,
                ),
                memory_text_value(
                    "ui.performance.memory.upload_rejected",
                    readout.upload_rejected.to_string(),
                    upload_color,
                ),
            )
        }
        None => (
            memory_text_value(
                "ui.performance.memory.resident",
                "--".to_string(),
                tokens.text_muted,
            ),
            memory_text_value(
                "ui.performance.memory.peak",
                "--".to_string(),
                tokens.text_muted,
            ),
            memory_text_value(
                "ui.performance.memory.budget",
                "--".to_string(),
                tokens.text_muted,
            ),
            memory_text_value(
                "ui.performance.memory.pressure",
                "--".to_string(),
                tokens.text_muted,
            ),
            memory_text_value(
                "ui.performance.memory.rejected",
                "--".to_string(),
                tokens.text_muted,
            ),
            memory_text_value(
                "ui.performance.memory.upload_rejected",
                "--".to_string(),
                tokens.text_muted,
            ),
        ),
    };

    UiNode::column(
        "ui.performance.memory",
        [
            UiNode::new("ui.performance.memory.title", UiNodeKind::Label)
                .with_text_key("ui.performance.memory")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content().fill_width()),
            memory_row(
                "ui.performance.memory.row.usage",
                [
                    memory_metric(
                        "ui.performance.memory.resident",
                        "ui.performance.memory.resident",
                        resident,
                        tokens,
                        gap,
                    ),
                    memory_metric(
                        "ui.performance.memory.peak",
                        "ui.performance.memory.peak",
                        peak,
                        tokens,
                        gap,
                    ),
                ],
                gap,
            ),
            memory_row(
                "ui.performance.memory.row.budget",
                [
                    memory_metric(
                        "ui.performance.memory.budget",
                        "ui.performance.memory.budget",
                        budget,
                        tokens,
                        gap,
                    ),
                    memory_metric(
                        "ui.performance.memory.pressure",
                        "ui.performance.memory.pressure",
                        pressure,
                        tokens,
                        gap,
                    ),
                ],
                gap,
            ),
            memory_row(
                "ui.performance.memory.row.uploads",
                [
                    memory_metric(
                        "ui.performance.memory.rejected",
                        "ui.performance.memory.rejected",
                        rejected,
                        tokens,
                        gap,
                    ),
                    memory_metric(
                        "ui.performance.memory.upload_rejected",
                        "ui.performance.memory.upload_rejected",
                        upload_rejected,
                        tokens,
                        gap,
                    ),
                ],
                gap,
            ),
        ],
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap,
        ..UiLayout::fit_content().fill_width()
    })
}

fn pressure_label_key(level: PressureLevel) -> &'static str {
    match level {
        PressureLevel::Nominal => "ui.performance.pressure.nominal",
        PressureLevel::Elevated => "ui.performance.pressure.elevated",
        PressureLevel::Critical => "ui.performance.pressure.critical",
    }
}

fn pressure_color(level: PressureLevel, tokens: UiTokens) -> [u8; 4] {
    match level {
        PressureLevel::Nominal => tokens.text,
        PressureLevel::Elevated => tokens.warning,
        PressureLevel::Critical => tokens.danger,
    }
}

fn memory_row(id: &str, cells: [UiNode; 2], gap: f32) -> UiNode {
    UiNode::row(id, cells).with_layout(UiLayout {
        flow: UiFlow::Row,
        // Double spacing separates the two metric pairs without adding a
        // decorative divider node.
        gap: gap * MEMORY_PAIR_GAP_SCALE,
        align_items: UiAlign::Center,
        ..UiLayout::fit_content().fill_width()
    })
}

fn memory_metric(id: &str, label_key: &str, value: UiNode, tokens: UiTokens, gap: f32) -> UiNode {
    UiNode::row(
        id,
        [
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
            value,
        ],
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap,
        align_items: UiAlign::Center,
        ..UiLayout::fit_content()
    })
}

fn memory_text_value(id: &str, value: String, color: [u8; 4]) -> UiNode {
    UiNode::new(format!("{id}.value"), UiNodeKind::Label)
        .with_text_value(value)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fit_content())
}

fn memory_key_value(id: &str, key: &'static str, color: [u8; 4]) -> UiNode {
    UiNode::new(format!("{id}.value"), UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fit_content())
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

    #[test]
    fn memory_group_projects_ledger_pressure_with_semantic_tokens() {
        let palette = StudioUiPalette::IndustrialDark;
        let tokens = palette.tokens();
        let readout = PerformanceReadout {
            resident_mb: 12.0,
            peak_mb: 14.5,
            budget_mb: 256.0,
            pressure: PressureLevel::Elevated,
            rejected_reservations: 3,
            upload_rejected: 2,
            upload_pressure: PressureLevel::Critical,
        };
        let surface = build_ui_performance_surface_with_readout(
            palette,
            &UiPerformanceSnapshot::default(),
            None,
            [640.0, 400.0],
            None,
            Some(readout),
        );

        let group = surface
            .root
            .find("ui.performance.memory")
            .expect("memory group");
        assert_eq!(
            group.children.len(),
            4,
            "memory group stays bounded to a title and three rows"
        );
        for id in [
            "ui.performance.memory.resident",
            "ui.performance.memory.peak",
            "ui.performance.memory.budget",
            "ui.performance.memory.pressure",
            "ui.performance.memory.rejected",
            "ui.performance.memory.upload_rejected",
        ] {
            let label = surface
                .root
                .find(&format!("{id}.label"))
                .expect("memory label");
            assert!(label.text_key.is_some(), "memory labels stay localized");
            assert!(
                label.text_value.is_none(),
                "memory labels carry no literals"
            );
        }

        let resident = surface
            .root
            .find("ui.performance.memory.resident.value")
            .expect("resident value");
        assert_eq!(resident.text_value.as_deref(), Some("12.0 MB"));

        let budget = surface
            .root
            .find("ui.performance.memory.budget.value")
            .expect("budget value");
        assert_eq!(budget.text_value.as_deref(), Some("256 MB"));

        let pressure = surface
            .root
            .find("ui.performance.memory.pressure.value")
            .expect("pressure value");
        assert_eq!(
            pressure.text_key.as_deref(),
            Some("ui.performance.pressure.elevated")
        );
        assert_eq!(
            pressure.text_style.map(|style| style.color),
            Some(tokens.warning)
        );

        let upload = surface
            .root
            .find("ui.performance.memory.upload_rejected.value")
            .expect("upload rejected value");
        assert_eq!(upload.text_value.as_deref(), Some("2"));
        assert_eq!(
            upload.text_style.map(|style| style.color),
            Some(tokens.danger)
        );

        let scroll_views = count_kind(&surface.root, UiNodeKind::ScrollView);
        assert_eq!(scroll_views, 1, "the memory group adds no scroll work");
    }

    #[test]
    fn memory_group_reports_unavailable_values_without_fake_zeros() {
        let surface = build_ui_performance_surface_with_readout(
            StudioUiPalette::PaperLight,
            &UiPerformanceSnapshot::default(),
            None,
            [640.0, 400.0],
            None,
            None,
        );

        for id in [
            "ui.performance.memory.resident.value",
            "ui.performance.memory.peak.value",
            "ui.performance.memory.budget.value",
            "ui.performance.memory.pressure.value",
            "ui.performance.memory.rejected.value",
            "ui.performance.memory.upload_rejected.value",
        ] {
            let node = surface.root.find(id).expect("memory value");
            assert_eq!(node.text_value.as_deref(), Some("--"));
            assert!(node.text_key.is_none());
        }
    }

    fn count_kind(node: &UiNode, kind: UiNodeKind) -> usize {
        let own = usize::from(node.kind == kind);
        own + node
            .children
            .iter()
            .map(|child| count_kind(child, kind))
            .sum::<usize>()
    }
}
