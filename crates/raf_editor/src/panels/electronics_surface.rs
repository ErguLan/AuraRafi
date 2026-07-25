//! Retained RafUI surfaces for Electronics analysis output.
//!
//! The DRC and simulation engines remain owned by the editor. This module only
//! renders their current report and emits explicit run commands.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_electronics::drc::{DrcReport, DrcSeverity};
use raf_electronics::schematic::Schematic;
use raf_electronics::simulation::SimulationResults;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsAnalysisSurfaceAction {
    RunDrc,
    RunSimulation,
}

pub struct ElectronicsAnalysisSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for ElectronicsAnalysisSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_electronics_analysis"),
        }
    }
}

impl ElectronicsAnalysisSurfaceHost {
    pub fn show_drc(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        report: Option<&DrcReport>,
        lang: Language,
    ) -> Option<ElectronicsAnalysisSurfaceAction> {
        let surface = build_drc_surface(palette, report, lang);
        self.show(
            ui,
            render_state,
            palette,
            surface,
            lang,
            "electronics.analysis.run-drc",
        )
        .then_some(ElectronicsAnalysisSurfaceAction::RunDrc)
    }

    pub fn show_simulation(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        schematic: &Schematic,
        results: Option<&SimulationResults>,
        lang: Language,
    ) -> Option<ElectronicsAnalysisSurfaceAction> {
        let surface = build_simulation_surface(palette, schematic, results, lang);
        self.show(
            ui,
            render_state,
            palette,
            surface,
            lang,
            "electronics.analysis.run-simulation",
        )
        .then_some(ElectronicsAnalysisSurfaceAction::RunSimulation)
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        lang: Language,
        run_command: &str,
    ) -> bool {
        self.bridge
            .show(ui, render_state, palette, surface, |key| t(key, lang))
            .into_iter()
            .any(|action| {
                matches!(
                    action.action,
                    raf_ui::UiAction::Command { ref name } if name == run_command
                )
            })
    }
}

fn build_drc_surface(
    palette: StudioUiPalette,
    report: Option<&DrcReport>,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut content = UiNode::scroll_view("electronics.drc.scroll", UiScrollAxis::Vertical)
        .with_class("electronics-analysis-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    if let Some(report) = report {
        content = content.with_child(metric_row(
            palette,
            &[
                format!(
                    "{} {}",
                    t("app.electronics_errors", lang),
                    report.errors.len()
                ),
                format!(
                    "{} {}",
                    t("app.electronics_warnings", lang),
                    report.warnings.len()
                ),
                format!("{} {}", t("app.electronics_info", lang), report.info.len()),
            ],
        ));

        if report.total() == 0 {
            content = content.with_child(state_line(
                palette,
                "electronics-analysis-success",
                "app.drc_ok",
            ));
        } else {
            for (index, issue) in report.all_issues().iter().enumerate() {
                let (severity_key, class) = match issue.severity {
                    DrcSeverity::Error => ("app.electronics_errors", "electronics-analysis-error"),
                    DrcSeverity::Warning => {
                        ("app.electronics_warnings", "electronics-analysis-warning")
                    }
                    DrcSeverity::Info => ("app.electronics_info", "electronics-analysis-info"),
                };
                content = content.with_child(
                    UiNode::new(format!("electronics.drc.issue.{index}"), UiNodeKind::Panel)
                        .with_class(class)
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            gap: 3.0,
                            padding: UiSpacing::xy(10.0, 7.0),
                            ..UiLayout::fixed(0.0, 54.0)
                        })
                        .with_child(
                            UiNode::new(
                                format!("electronics.drc.issue.{index}.rule"),
                                UiNodeKind::Label,
                            )
                            .with_text_key(format!("{} | {}", t(severity_key, lang), issue.rule))
                            .with_text_style(UiTextStyle::button(tokens.text))
                            .with_layout(UiLayout::fixed(0.0, 18.0)),
                        )
                        .with_child(
                            UiNode::new(
                                format!("electronics.drc.issue.{index}.message"),
                                UiNodeKind::Label,
                            )
                            .with_text_key(issue.message.clone())
                            .with_text_style(UiTextStyle::body(tokens.text_muted))
                            .with_layout(UiLayout::fixed(0.0, 28.0)),
                        ),
                );
            }
        }
    } else {
        content = content.with_child(state_line(
            palette,
            "electronics-analysis-empty",
            "app.electronics_drc_empty",
        ));
    }

    build_analysis_surface(
        palette,
        "app.electronics_drc",
        "app.electronics_run_drc",
        "electronics.analysis.run-drc",
        content,
    )
}

fn build_simulation_surface(
    palette: StudioUiPalette,
    schematic: &Schematic,
    results: Option<&SimulationResults>,
    lang: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut content = UiNode::scroll_view("electronics.simulation.scroll", UiScrollAxis::Vertical)
        .with_class("electronics-analysis-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    let Some(results) = results else {
        return build_analysis_surface(
            palette,
            "app.electronics_simulation",
            "app.electronics_run_simulation",
            "electronics.analysis.run-simulation",
            content.with_child(state_line(
                palette,
                "electronics-analysis-empty",
                "app.electronics_simulation_empty",
            )),
        );
    };

    content = content.with_child(metric_row(
        palette,
        &[
            format!(
                "{} {}",
                t("app.electronics_nets", lang),
                results.node_voltages.len()
            ),
            format!(
                "{} {}",
                t("app.electronics_branches", lang),
                results.component_currents.len()
            ),
            t(
                if results.converged {
                    "app.electronics_converged"
                } else {
                    "app.electronics_failed"
                },
                lang,
            ),
        ],
    ));

    if !results.messages.is_empty() {
        for (index, message) in results.messages.iter().enumerate() {
            content = content.with_child(
                UiNode::new(
                    format!("electronics.simulation.message.{index}"),
                    UiNodeKind::Panel,
                )
                .with_class("electronics-analysis-info")
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(10.0, 7.0),
                    ..UiLayout::fixed(0.0, 36.0)
                })
                .with_child(
                    UiNode::new(
                        format!("electronics.simulation.message.{index}.text"),
                        UiNodeKind::Label,
                    )
                    .with_text_key(message.clone())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
                ),
            );
        }
    }

    content = content.with_child(section_label(palette, "app.electronics_nets"));
    let mut voltages: Vec<_> = results.node_voltages.iter().collect();
    voltages.sort_by_key(|(id, _)| **id);
    for (net, voltage) in voltages {
        content = content.with_child(value_row(
            palette,
            &format!("Net-{net}"),
            &format!("{voltage:.3} V"),
        ));
    }

    content = content.with_child(section_label(palette, "app.electronics_branches"));
    let mut currents: Vec<_> = results.component_currents.iter().collect();
    currents.sort_by_key(|(index, _)| **index);
    for (index, current) in currents {
        let label = schematic
            .components
            .get(*index)
            .map(|component| component.designator.clone())
            .unwrap_or_else(|| format!("#{}", index + 1));
        let power = results
            .component_power
            .get(index)
            .copied()
            .unwrap_or_default();
        content = content.with_child(value_row(
            palette,
            &label,
            &format!("{current:.4} A | {power:.4} W"),
        ));
    }

    build_analysis_surface(
        palette,
        "app.electronics_simulation",
        "app.electronics_run_simulation",
        "electronics.analysis.run-simulation",
        content,
    )
}

fn build_analysis_surface(
    palette: StudioUiPalette,
    title_key: &str,
    run_label_key: &str,
    run_command: &str,
    content: UiNode,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("electronics.analysis.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("electronics.analysis.header", UiNodeKind::Toolbar)
                .with_class("electronics-analysis-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(12.0, 0.0),
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 38.0)
                })
                .with_child(
                    UiNode::new("electronics.analysis.title", UiNodeKind::Label)
                        .with_text_key(title_key)
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(
                    UiNode::new("electronics.analysis.run", UiNodeKind::Button)
                        .with_class("electronics-analysis-run")
                        .with_text_key(run_label_key)
                        .with_text_style(UiTextStyle::button(tokens.text))
                        .with_layout(UiLayout::fixed(164.0, 28.0))
                        .focusable()
                        .with_event(UiEventBinding::command(UiEventKind::Click, run_command)),
                ),
        )
        .with_child(content);

    let mut surface = UiSurface::new("electronics-analysis", palette, root);
    surface.style_sheet = electronics_analysis_style_sheet(palette);
    surface
}

fn metric_row(palette: StudioUiPalette, values: &[String]) -> UiNode {
    let tokens = palette.tokens();
    let mut row =
        UiNode::new("electronics.analysis.metrics", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 32.0)
        });
    for (index, value) in values.iter().enumerate() {
        row = row.with_child(
            UiNode::new(
                format!("electronics.analysis.metric.{index}"),
                UiNodeKind::Panel,
            )
            .with_class("electronics-analysis-metric")
            .with_text_key(value.clone())
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_layout(UiLayout {
                grow: 1.0,
                padding: UiSpacing::xy(8.0, 5.0),
                ..UiLayout::default()
            }),
        );
    }
    row
}

fn state_line(palette: StudioUiPalette, class: &str, key: &str) -> UiNode {
    UiNode::new(
        format!("electronics.analysis.state.{key}"),
        UiNodeKind::Panel,
    )
    .with_class(class)
    .with_layout(UiLayout {
        padding: UiSpacing::xy(10.0, 8.0),
        ..UiLayout::fixed(0.0, 36.0)
    })
    .with_child(
        UiNode::new(
            format!("electronics.analysis.state.{key}.label"),
            UiNodeKind::Label,
        )
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(palette.tokens().text)),
    )
}

fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(
        format!("electronics.analysis.section.{key}"),
        UiNodeKind::Label,
    )
    .with_text_key(key)
    .with_text_style(UiTextStyle::panel_title(palette.tokens().text_muted))
    .with_layout(UiLayout::fixed(0.0, 20.0))
}

fn value_row(palette: StudioUiPalette, label: &str, value: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("electronics.analysis.value.{}", label.replace(' ', "-")),
        UiNodeKind::Panel,
    )
    .with_class("electronics-analysis-value")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        padding: UiSpacing::xy(10.0, 5.0),
        ..UiLayout::fixed(0.0, 28.0)
    })
    .with_child(
        UiNode::new("label", UiNodeKind::Label)
            .with_text_key(label)
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
    )
    .with_child(
        UiNode::new("value", UiNodeKind::Label)
            .with_text_key(value)
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(150.0, 18.0)),
    )
}

fn electronics_analysis_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-scroll".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-run".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-run".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-metric".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-value".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-success".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.positive),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-warning".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.warning),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-error".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.danger),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-analysis-info".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Panel),
                UiStylePatch {
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}
