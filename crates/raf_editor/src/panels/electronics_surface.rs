//! Native Electronics analysis/status surface.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
};
use raf_ui::{UiAlign, UiFontWeight, UiTextRole, UiTextStyle, UiTokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsAnalysisSurfaceAction {
    RunDrc,
    RunSimulation,
    Clear,
}

pub fn build_electronics_analysis_surface(
    palette: StudioUiPalette,
    title: &str,
    lines: &[String],
) -> UiSurface {
    let tokens = palette.tokens();
    let is_simulation = title.to_ascii_lowercase().contains("simulation");
    let header_icon = if is_simulation {
        UiIconId::Play
    } else {
        UiIconId::Warning
    };
    let mut root = UiNode::new("electronics.analysis", UiNodeKind::Panel)
        .with_class("electronics-analysis")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(10.0, 8.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("electronics.analysis.header", UiNodeKind::Toolbar)
                .with_class("electronics-analysis-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 7.0,
                    padding: UiSpacing::xy(8.0, 6.0),
                    ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("electronics.analysis.header.icon", UiNodeKind::Label)
                        .with_icon(UiIcon::new(header_icon).with_size(UiIconSize::Small)),
                )
                .with_child(
                    UiNode::new("electronics.analysis.header.title", UiNodeKind::Label)
                        .with_text_value(title.to_string())
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 12.0,
                            line_height_px: 16.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        }),
                ),
        );
    let mut lines_view = UiNode::scroll_view("electronics.analysis.lines", UiScrollAxis::Vertical)
        .with_class("electronics-analysis-lines")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(2.0, 2.0),
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    for (index, line) in lines.iter().enumerate() {
        let line_color = analysis_line_color(tokens, line);
        lines_view = lines_view.with_child(
            UiNode::new(
                format!("electronics.analysis.line.{index}"),
                UiNodeKind::Label,
            )
            .with_class("electronics-analysis-line")
            .with_text_value(line.clone())
            .with_text_style(UiTextStyle {
                role: UiTextRole::Body,
                size_px: 11.0,
                line_height_px: 15.0,
                weight: UiFontWeight::Regular,
                color: line_color,
                inherit_color: false,
            })
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    root = root.with_child(lines_view);
    let is_running = lines.iter().any(|line| line == "Status: running");
    let has_result = lines
        .iter()
        .any(|line| line.starts_with("Status:") && line != "Status: not run");
    let (command, label) = if is_running {
        ("electronics.analysis.cancel", "Cancel")
    } else if is_simulation {
        (
            "electronics.analysis.simulation",
            if has_result {
                "Re-run simulation"
            } else {
                "Simulate"
            },
        )
    } else {
        (
            "electronics.analysis.drc",
            if has_result { "Re-run DRC" } else { "Run DRC" },
        )
    };
    root = root.with_child(action(command, label));
    let mut surface = UiSurface::new("electronics.analysis", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            class_rule(
                "electronics-analysis",
                tokens.background,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "electronics-analysis-header",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "electronics-analysis-line",
                tokens.surface,
                tokens.border,
                tokens.text_muted,
            ),
            class_rule(
                "electronics-analysis-action",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
        ],
    };
    surface
}

fn analysis_line_color(tokens: UiTokens, line: &str) -> [u8; 4] {
    if !line.starts_with("Status:") {
        return tokens.text_muted;
    }
    let status = line.to_ascii_lowercase();
    if status.contains("passed") || status.contains("converged") {
        [112, 224, 136, 255]
    } else if status.contains("issues") || status.contains("not converged") {
        [255, 172, 64, 255]
    } else if status.contains("running") {
        [120, 190, 255, 255]
    } else if status.contains("cancelled") || status.contains("failed") {
        [255, 128, 128, 255]
    } else {
        tokens.text_muted
    }
}

fn action(id: &str, text: &str) -> UiNode {
    let tooltip = if id == "electronics.analysis.cancel" {
        "electronics.tooltip.analysis.cancel"
    } else if id == "electronics.analysis.simulation" {
        "electronics.tooltip.analysis.simulation"
    } else {
        "electronics.tooltip.analysis.drc"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class("electronics-analysis-action")
        .with_layout(
            UiLayout::fixed(0.0, 30.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_text_safe_area(true),
        )
        .with_text_value(text.to_string())
        .with_text_style(UiTextStyle::button([245, 245, 246, 255]))
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id))
}

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(4.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}
