//! Native Electronics analysis/status surface.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
};
use raf_ui::{UiAlign, UiFontWeight, UiTextRole, UiTextStyle, UiTokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElectronicsAnalysisTone {
    Normal,
    Running,
    Passed,
    Issues,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ElectronicsAnalysisLine {
    pub text: String,
    pub tone: ElectronicsAnalysisTone,
}

impl ElectronicsAnalysisLine {
    pub(crate) fn normal(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ElectronicsAnalysisTone::Normal,
        }
    }

    pub(crate) fn with_tone(text: impl Into<String>, tone: ElectronicsAnalysisTone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsAnalysisSurfaceAction {
    RunDrc,
    RunSimulation,
    Clear,
}

pub(crate) fn build_electronics_analysis_surface(
    palette: StudioUiPalette,
    title: &str,
    lines: &[ElectronicsAnalysisLine],
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
                        .with_text_key(analysis_title_key(title))
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
        let line_color = analysis_line_color(tokens, line.tone);
        lines_view = lines_view.with_child(
            UiNode::new(
                format!("electronics.analysis.line.{index}"),
                UiNodeKind::Label,
            )
            .with_class("electronics-analysis-line")
            .with_text_value(line.text.clone())
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
    let is_running = lines
        .iter()
        .any(|line| line.tone == ElectronicsAnalysisTone::Running);
    let has_result = lines.iter().any(|line| {
        matches!(
            line.tone,
            ElectronicsAnalysisTone::Passed
                | ElectronicsAnalysisTone::Issues
                | ElectronicsAnalysisTone::Failed
        )
    });
    let (command, label_key) = if is_running {
        ("electronics.analysis.cancel", "electronics.analysis.cancel")
    } else if is_simulation {
        (
            "electronics.analysis.simulation",
            if has_result {
                "electronics.analysis.rerun_simulation"
            } else {
                "electronics.analysis.simulate"
            },
        )
    } else {
        (
            "electronics.analysis.drc",
            if has_result {
                "electronics.analysis.rerun_drc"
            } else {
                "electronics.analysis.run_drc"
            },
        )
    };
    root = root.with_child(action(command, label_key));
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

fn analysis_line_color(tokens: UiTokens, tone: ElectronicsAnalysisTone) -> [u8; 4] {
    match tone {
        ElectronicsAnalysisTone::Passed => [112, 224, 136, 255],
        ElectronicsAnalysisTone::Issues => [255, 172, 64, 255],
        ElectronicsAnalysisTone::Running => [120, 190, 255, 255],
        ElectronicsAnalysisTone::Failed => [255, 128, 128, 255],
        ElectronicsAnalysisTone::Normal => tokens.text_muted,
    }
}

fn action(id: &str, text_key: &str) -> UiNode {
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
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([245, 245, 246, 255]))
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id))
}

fn analysis_title_key(title: &str) -> &'static str {
    if title.to_ascii_lowercase().contains("simulation") {
        "electronics.analysis.simulation_title"
    } else {
        "electronics.analysis.drc_title"
    }
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
