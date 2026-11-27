//! Central performance impact metadata and presentation for settings rows.
//!
//! The table below is the single source of truth for the badges, the
//! "performance only" filter and the section summary. Rows are identified by
//! the same stable id they use for their retained UI nodes, so adding a setting
//! never requires duplicating its key in the presentation code.

use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout,
    UiNode, UiNodeKind, UiSpacing, UiStyle, UiTextOverflow, UiTextStyle,
};
use raf_ui::{UiFontWeight, UiIcon, UiIconId, UiIconSize, UiTextRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImpactLevel {
    Low,
    Medium,
    High,
}

impl ImpactLevel {
    fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImpactDirection {
    /// Lowering or disabling the option tends to free CPU/GPU resources.
    LowersCost,
    /// Raising or enabling the option tends to free CPU/GPU resources.
    RaisesBenefit,
    /// The useful direction depends on the user's quality/performance target.
    Tradeoff,
}

impl ImpactDirection {
    fn key(self) -> &'static str {
        match self {
            Self::LowersCost => "lowers_cost",
            Self::RaisesBenefit => "raises_benefit",
            Self::Tradeoff => "tradeoff",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PerformanceImpact {
    pub level: ImpactLevel,
    pub direction: ImpactDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SettingImpact {
    pub performance: Option<PerformanceImpact>,
    pub visual: Option<ImpactLevel>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ImpactSummary {
    pub performance: usize,
    pub high: usize,
}

impl SettingImpact {
    const fn performance(level: ImpactLevel, direction: ImpactDirection) -> Self {
        Self {
            performance: Some(PerformanceImpact { level, direction }),
            visual: None,
        }
    }

    const fn with_visual(mut self, visual: ImpactLevel) -> Self {
        self.visual = Some(visual);
        self
    }
}

/// Returns the authored impact for one settings row, or `None` when the row is
/// purely behavioral, organizational or external.
pub(super) fn setting_impact(row_id: &str) -> Option<SettingImpact> {
    use ImpactDirection::{LowersCost, RaisesBenefit, Tradeoff};
    use ImpactLevel::{High, Low, Medium};

    Some(match row_id {
        // Performance: editor profile and frame pacing.
        "settings.quality" => SettingImpact::performance(High, LowersCost).with_visual(High),
        "settings.render_execution_policy" => SettingImpact::performance(High, Tradeoff),
        "settings.texture_quality" => {
            SettingImpact::performance(High, LowersCost).with_visual(High)
        }
        "settings.scene-packet-reuse" => {
            SettingImpact::performance(High, RaisesBenefit).with_visual(Low)
        }
        "settings.vsync" => SettingImpact::performance(Low, LowersCost).with_visual(Low),
        "settings.multithreading" => SettingImpact::performance(High, RaisesBenefit),
        "settings.show-fps-counter" => SettingImpact::performance(Low, LowersCost),
        "settings.fps-unlimited" => SettingImpact::performance(High, LowersCost),
        "settings.fps_limit" => SettingImpact::performance(High, LowersCost),

        // Performance: editor particle preview.
        "settings.particle_preview.enabled" => {
            SettingImpact::performance(Medium, LowersCost).with_visual(Medium)
        }
        "settings.particle_preview.paused" => SettingImpact::performance(Medium, RaisesBenefit),
        "settings.particle_preview.frozen" => SettingImpact::performance(Medium, RaisesBenefit),
        "settings.particle_preview.stopped" => SettingImpact::performance(Medium, RaisesBenefit),
        "settings.particle_preview.max_sprites" => {
            SettingImpact::performance(High, LowersCost).with_visual(Low)
        }

        // Editor workspace and viewport presentation.
        "settings.show-grid" => SettingImpact::performance(Low, LowersCost),
        "settings.inspector-live-transform-updates" => {
            SettingImpact::performance(Medium, LowersCost)
        }
        "settings.grid_load_distance" => SettingImpact::performance(Medium, LowersCost),
        "settings.auto-save" => SettingImpact::performance(Low, RaisesBenefit),
        "settings.hierarchy-animations" => SettingImpact::performance(Low, LowersCost),
        "settings.electronics-grid-visible" => {
            SettingImpact::performance(Low, LowersCost).with_visual(Low)
        }
        "settings.electronics-grid-step" => {
            SettingImpact::performance(Low, RaisesBenefit).with_visual(Low)
        }
        "settings.viewport-labels" => SettingImpact::performance(Low, LowersCost).with_visual(Low),
        "settings.surface-edges" => {
            SettingImpact::performance(Medium, LowersCost).with_visual(Medium)
        }
        "settings.xray" => SettingImpact::performance(Medium, LowersCost).with_visual(Medium),
        "settings.face-tonality" => SettingImpact::performance(Low, LowersCost).with_visual(Medium),

        // Appearance and accessibility choices that change compositing cost.
        "settings.reduce-transparency" => {
            SettingImpact::performance(Low, RaisesBenefit).with_visual(Low)
        }
        "settings.prefers-reduced-motion" => {
            SettingImpact::performance(Low, RaisesBenefit).with_visual(Low)
        }
        "settings.theme-experimental" => {
            SettingImpact::performance(Low, LowersCost).with_visual(Low)
        }

        // Runtime and platform launch budgets.
        "settings.runtime.launch" => SettingImpact::performance(Medium, Tradeoff),
        "settings.runtime.max_instances" => SettingImpact::performance(High, LowersCost),
        "settings.runtime.width" => SettingImpact::performance(Medium, LowersCost),
        "settings.runtime.height" => SettingImpact::performance(Medium, LowersCost),
        "settings.runtime.script_budget_ms" => SettingImpact::performance(Low, LowersCost),
        "settings.runtime.script_operation_limit" => SettingImpact::performance(Low, LowersCost),
        "settings.runtime.hot-reload" => SettingImpact::performance(Low, LowersCost),
        "settings.runtime.diagnostics" => SettingImpact::performance(Low, LowersCost),
        "settings.headless" => SettingImpact::performance(High, RaisesBenefit),

        _ => return None,
    })
}

/// True when the row carries a performance badge, which is also the contract
/// used by the "performance only" filter.
pub(super) fn row_has_performance(node: &UiNode) -> bool {
    node.id
        .strip_suffix(".row")
        .and_then(setting_impact)
        .is_some_and(|impact| impact.performance.is_some())
}

/// Walks a composed section and counts its performance rows.
pub(super) fn summarize(node: &UiNode) -> ImpactSummary {
    let mut summary = ImpactSummary::default();
    collect_summary(node, &mut summary);
    summary
}

fn collect_summary(node: &UiNode, summary: &mut ImpactSummary) {
    if node.classes.iter().any(|class| class == "settings-row") {
        if let Some(performance) = node
            .id
            .strip_suffix(".row")
            .and_then(setting_impact)
            .and_then(|impact| impact.performance)
        {
            summary.performance += 1;
            if performance.level == ImpactLevel::High {
                summary.high += 1;
            }
        }
        return;
    }
    for child in &node.children {
        collect_summary(child, summary);
    }
}

pub(super) fn summary_label(
    palette: StudioUiPalette,
    summary: ImpactSummary,
    language: Language,
) -> UiNode {
    let tokens = palette.tokens();
    let text = summary_text(summary, language);
    UiNode::new("settings.impact.summary", UiNodeKind::Label)
        .with_text_value(text.clone())
        .with_text_style(UiTextStyle {
            role: UiTextRole::Label,
            size_px: 11.0,
            line_height_px: 14.0,
            weight: UiFontWeight::Regular,
            color: tokens.text_muted,
            inherit_color: false,
        })
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_tooltip_value(text)
        .with_layout(UiLayout::fixed(0.0, 14.0).with_width_mode(raf_ui::UiSizeMode::Fill))
}

fn summary_text(summary: ImpactSummary, language: Language) -> String {
    if summary.high == 0 {
        return t("settings.impact.summary_no_high", language)
            .replace("{count}", &summary.performance.to_string());
    }
    t("settings.impact.summary", language)
        .replace("{count}", &summary.performance.to_string())
        .replace("{high}", &summary.high.to_string())
}

pub(super) fn performance_filter_button(palette: StudioUiPalette, active: bool) -> UiNode {
    let tokens = palette.tokens();
    let text_color = if active {
        tokens.accent
    } else {
        tokens.text_muted
    };
    let mut button = UiNode::new("settings.performance-filter", UiNodeKind::Button)
        .with_class("settings-impact-filter")
        .with_icon(UiIcon::new(UiIconId::Filter).with_size(UiIconSize::Small))
        .with_text_key("settings.impact.filter")
        .with_text_style(UiTextStyle {
            inherit_color: true,
            ..UiTextStyle::button(text_color)
        })
        .with_tooltip_key(if active {
            "settings.impact.filter_tooltip_active"
        } else {
            "settings.impact.filter_tooltip"
        })
        .with_accessibility_label_key("settings.impact.filter")
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_selected(active)
        .focusable()
        .with_layout(UiLayout::fixed(164.0, 28.0))
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            super::PERFORMANCE_FILTER_COMMAND,
        ));
    if active {
        button = button.with_class("settings-impact-filter-active");
    }
    button
}

pub(super) fn badges(palette: StudioUiPalette, row_id: &str, impact: SettingImpact) -> UiNode {
    let mut node =
        UiNode::new(format!("{row_id}.impact-badges"), UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fit_content().with_width_mode(raf_ui::UiSizeMode::Fill)
        });
    if let Some(performance) = impact.performance {
        node = node.with_child(performance_badge(palette, row_id, performance));
    }
    if let Some(level) = impact.visual.filter(|level| *level != ImpactLevel::Low) {
        node = node.with_child(visual_badge(palette, row_id, level));
    }
    node
}

fn performance_badge(palette: StudioUiPalette, row_id: &str, impact: PerformanceImpact) -> UiNode {
    let tokens = palette.tokens();
    let (fill, border, text_color) = match impact.level {
        ImpactLevel::High => (
            color_with_alpha(tokens.warning, 26),
            color_with_alpha(tokens.warning, 90),
            tokens.warning,
        ),
        ImpactLevel::Medium => (
            color_with_alpha(tokens.info, 22),
            color_with_alpha(tokens.info, 80),
            tokens.info,
        ),
        ImpactLevel::Low => (tokens.surface_alt, tokens.border, tokens.text_muted),
    };
    let tooltip_key = format!(
        "settings.impact.tooltip.performance.{}.{}",
        impact.level.key(),
        impact.direction.key()
    );
    badge_shell(
        format!("{row_id}.impact.performance"),
        format!("settings-impact-performance-{}", impact.level.key()),
        UiIconId::Frame,
        text_color,
        format!("settings.impact.performance.{}", impact.level.key()),
        fill,
        border,
        tooltip_key,
    )
}

fn visual_badge(palette: StudioUiPalette, row_id: &str, level: ImpactLevel) -> UiNode {
    let tokens = palette.tokens();
    badge_shell(
        format!("{row_id}.impact.visual"),
        format!("settings-impact-visual-{}", level.key()),
        UiIconId::Eye,
        tokens.text_muted,
        format!("settings.impact.visual.{}", level.key()),
        tokens.surface_alt,
        tokens.border,
        format!("settings.impact.tooltip.visual.{}", level.key()),
    )
}

#[allow(clippy::too_many_arguments)]
fn badge_shell(
    id: String,
    class: String,
    icon: UiIconId,
    text_color: [u8; 4],
    label_key: String,
    fill: [u8; 4],
    border: [u8; 4],
    tooltip_key: String,
) -> UiNode {
    let icon_id = format!("{id}.icon");
    let label_id = format!("{id}.label");
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("settings-impact-badge")
        .with_class(class)
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key(label_key.clone())
        .with_style(UiStyle {
            fill,
            border,
            text: text_color,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 0.0),
            basis: [0.0, 18.0],
            width_mode: raf_ui::UiSizeMode::FitContent,
            height_mode: raf_ui::UiSizeMode::Fixed,
            min_size: [0.0, 18.0],
            ..UiLayout::default()
        })
        .with_tooltip_key(tooltip_key)
        .with_child(
            UiNode::new(icon_id, UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Custom(11))
                        .with_tint(text_color),
                )
                .with_layout(UiLayout::fixed(11.0, 11.0)),
        )
        .with_child(
            UiNode::new(label_id, UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 10.0,
                    line_height_px: 12.0,
                    weight: UiFontWeight::Medium,
                    color: text_color,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        )
}

fn color_with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_copy_is_present_in_both_locale_catalogs() {
        let mut keys = vec![
            "settings.impact.filter".to_string(),
            "settings.impact.filter_tooltip".to_string(),
            "settings.impact.filter_tooltip_active".to_string(),
            "settings.impact.summary".to_string(),
            "settings.impact.summary_no_high".to_string(),
            "settings.impact.no_results".to_string(),
        ];
        for level in ["low", "medium", "high"] {
            keys.push(format!("settings.impact.performance.{level}"));
            keys.push(format!("settings.impact.visual.{level}"));
            keys.push(format!("settings.impact.tooltip.visual.{level}"));
            for direction in ["lowers_cost", "raises_benefit", "tradeoff"] {
                keys.push(format!(
                    "settings.impact.tooltip.performance.{level}.{direction}"
                ));
            }
        }

        for language in [Language::English, Language::Spanish] {
            for key in &keys {
                assert_ne!(
                    t(key, language),
                    *key,
                    "missing translation for {key} in {language:?}"
                );
            }
        }
    }
}
