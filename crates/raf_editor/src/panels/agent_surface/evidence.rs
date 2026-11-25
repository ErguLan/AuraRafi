//! Plan review and evidence cards for the Agent surface.

use raf_ai::agent_runtime::AgentToolResult;
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon,
    UiIconId, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing, UiStyle,
    UiTextOverflow, UiTextStyle,
};
use raf_ui::UiTokens;

use crate::panels::ai_chat::AgentPanel;

use super::layout::{short_text, PLAN_REVIEW_HEIGHT, PLAN_REVIEW_TREE_ROWS};

#[derive(Debug, Clone)]
pub(super) struct PlanReview {
    pub(super) plan_id: String,
    pub(super) mode: String,
    pub(super) groups: u64,
    pub(super) entities: u64,
    pub(super) chunks: u64,
    pub(super) design_status: String,
    pub(super) overlap_count: u64,
    pub(super) budget_status: String,
    pub(super) navigation_status: String,
    pub(super) warnings: Vec<String>,
    pub(super) group_sample: Vec<String>,
    pub(super) entity_sample: Vec<String>,
    pub(super) hash: Option<String>,
    pub(super) persisted_path: Option<String>,
    pub(super) revision: Option<u64>,
}

pub(super) fn plan_review_from_message(message: &ChatMessage) -> Option<PlanReview> {
    if message.role != MessageRole::Tool {
        return None;
    }
    let result: AgentToolResult = serde_json::from_str(&message.content).ok()?;
    let data = &result.data;
    let plan_id = data.get("plan_id")?.as_str()?.to_string();
    let preview = data.get("preview")?.as_object()?;
    let gate_status = |gate: &str| {
        data.get("pipeline")
            .and_then(|pipeline| pipeline.get("gates"))
            .and_then(serde_json::Value::as_array)
            .and_then(|gates| {
                gates.iter().find(|candidate| {
                    candidate.get("gate").and_then(serde_json::Value::as_str) == Some(gate)
                })
            })
            .and_then(|candidate| candidate.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("not_declared")
            .to_string()
    };
    let names = |key: &str| {
        preview
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(serde_json::Value::as_str))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    Some(PlanReview {
        plan_id,
        mode: data
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("build")
            .to_string(),
        groups: data
            .get("groups")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        entities: data
            .get("entities")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        chunks: preview
            .get("chunks")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        design_status: data
            .get("design_audit")
            .and_then(|audit| audit.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("not_run")
            .to_string(),
        overlap_count: data
            .get("overlap_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        budget_status: gate_status("budget"),
        navigation_status: gate_status("navigation"),
        warnings: result.warnings,
        group_sample: names("group_sample"),
        entity_sample: names("entity_sample"),
        hash: data
            .get("plan_hash")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        persisted_path: data
            .get("persisted_path")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        revision: result.revision,
    })
}

pub(super) fn short_plan_id(plan_id: &str) -> String {
    plan_id
        .strip_prefix("plan_")
        .unwrap_or(plan_id)
        .chars()
        .take(12)
        .collect()
}

pub(super) fn plan_metric_chip(
    tokens: &UiTokens,
    id: &str,
    label_key: &str,
    value: String,
) -> UiNode {
    UiNode::new(id.to_string(), UiNodeKind::Panel)
        .with_class("agent-plan-metric")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 1.0,
            padding: UiSpacing::xy(8.0, 4.0),
            height_mode: UiSizeMode::FitContent,
            ..UiLayout::fit_content()
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            border_width: 1.0,
            radius: 6.0,
            ..UiStyle::transparent()
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_value(value)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
}

pub(super) fn plan_validation_chip(
    tokens: &UiTokens,
    language: Language,
    id: &str,
    label_key: &str,
    value: String,
    color: [u8; 4],
) -> UiNode {
    UiNode::new(id.to_string(), UiNodeKind::Label)
        .with_class("agent-plan-validation")
        .with_text_value(format!("{}: {}", t(label_key, language), value))
        .with_text_style(UiTextStyle::button(color))
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 3.0),
            ..UiLayout::fit_content()
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: color,
            border_width: 1.0,
            radius: 5.0,
            ..UiStyle::transparent()
        })
}

pub(super) fn plan_review_card(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    review: &PlanReview,
    width: f32,
    index: usize,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let card_width = (width * 0.94).clamp(320.0, 900.0);
    let mut card = UiNode::new(format!("agent.plan.review.{index}"), UiNodeKind::Panel)
        .with_class("agent-plan-review")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 7.0,
            padding: UiSpacing::same(13.0),
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fixed,
            basis: [card_width, PLAN_REVIEW_HEIGHT],
            overflow: UiOverflow::Clip,
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent_hot,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_label_key("app.agent_plan_title");
    let mut header = UiNode::new(
        format!("agent.plan.review.{index}.header"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 8.0,
        height_mode: UiSizeMode::Fixed,
        basis: [0.0, 28.0],
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(format!("agent.plan.review.{index}.icon"), UiNodeKind::Image)
            .with_icon(UiIcon::new(UiIconId::Grid).with_tint(tokens.accent_hot))
            .with_layout(UiLayout::fixed(20.0, 20.0)),
    )
    .with_child(
        UiNode::new(
            format!("agent.plan.review.{index}.title"),
            UiNodeKind::Label,
        )
        .with_text_key("app.agent_plan_title")
        .with_text_style(UiTextStyle::panel_title(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            width_mode: UiSizeMode::Fill,
            ..UiLayout::fit_content()
        }),
    )
    .with_child({
        let mut id_chip = UiNode::new(format!("agent.plan.review.{index}.id"), UiNodeKind::Label)
            .with_class("agent-tool-chip")
            .with_text_value(format!(
                "{} · {}",
                t("app.agent_plan_plan", panel.language),
                short_plan_id(&review.plan_id)
            ))
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content());
        let tooltip = [
            review.hash.as_ref().map(|hash| format!("Hash: {hash}")),
            review
                .persisted_path
                .as_ref()
                .map(|path| format!("Journal: {path}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        if !tooltip.is_empty() {
            id_chip = id_chip.with_tooltip_value(tooltip);
        }
        id_chip
    })
    .with_child(
        UiNode::new(format!("agent.plan.review.{index}.mode"), UiNodeKind::Label)
            .with_class("agent-tool-chip")
            .with_text_value(review.mode.clone())
            .with_text_style(UiTextStyle::button(tokens.accent_hot))
            .with_layout(UiLayout::fit_content()),
    );
    if let Some(revision) = review.revision {
        header = header.with_child(
            UiNode::new(
                format!("agent.plan.review.{index}.revision"),
                UiNodeKind::Label,
            )
            .with_class("agent-tool-chip")
            .with_text_value(format!("Rev {revision}"))
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
        );
    }
    card = card.with_child(header);
    let metrics = UiNode::new(
        format!("agent.plan.review.{index}.metrics"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 6.0,
        height_mode: UiSizeMode::FitContent,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(plan_metric_chip(
        &tokens,
        &format!("agent.plan.metric.groups.{index}"),
        "app.agent_plan_groups",
        review.groups.to_string(),
    ))
    .with_child(plan_metric_chip(
        &tokens,
        &format!("agent.plan.metric.entities.{index}"),
        "app.agent_plan_entities",
        review.entities.to_string(),
    ))
    .with_child(plan_metric_chip(
        &tokens,
        &format!("agent.plan.metric.chunks.{index}"),
        "app.agent_plan_chunks",
        review.chunks.to_string(),
    ));
    card = card.with_child(metrics);
    let (design_value, design_color) = match review.design_status.as_str() {
        "passed" => (t("app.agent_plan_passed", panel.language), tokens.positive),
        "failed" => (t("app.agent_plan_failed", panel.language), tokens.danger),
        _ => (t("app.agent_plan_not_run", panel.language), tokens.warning),
    };
    let overlap_color = if review.overlap_count == 0 {
        tokens.positive
    } else {
        tokens.warning
    };
    let gate_color = |status: &str| match status {
        "passed" => tokens.positive,
        "failed" => tokens.danger,
        _ => tokens.warning,
    };
    let budget_color = gate_color(&review.budget_status);
    let navigation_color = gate_color(&review.navigation_status);
    let mut validations = UiNode::new(
        format!("agent.plan.review.{index}.validations"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 6.0,
        height_mode: UiSizeMode::FitContent,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(plan_validation_chip(
        &tokens,
        panel.language,
        &format!("agent.plan.validation.design.{index}"),
        "app.agent_plan_design",
        design_value,
        design_color,
    ))
    .with_child(plan_validation_chip(
        &tokens,
        panel.language,
        &format!("agent.plan.validation.overlaps.{index}"),
        "app.agent_plan_overlaps",
        review.overlap_count.to_string(),
        overlap_color,
    ))
    .with_child(plan_validation_chip(
        &tokens,
        panel.language,
        &format!("agent.plan.validation.budget.{index}"),
        "app.agent_plan_budget",
        review.budget_status.clone(),
        budget_color,
    ))
    .with_child(plan_validation_chip(
        &tokens,
        panel.language,
        &format!("agent.plan.validation.navigation.{index}"),
        "app.agent_plan_navigation",
        review.navigation_status.clone(),
        navigation_color,
    ));
    if !review.warnings.is_empty() {
        validations = validations.with_child(plan_validation_chip(
            &tokens,
            panel.language,
            &format!("agent.plan.validation.warnings.{index}"),
            "app.agent_plan_warnings",
            review.warnings.len().to_string(),
            tokens.warning,
        ));
    }
    card = card.with_child(validations);
    let mut tree = UiNode::new(format!("agent.plan.review.{index}.tree"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 1.0,
            height_mode: UiSizeMode::FitContent,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(
                format!("agent.plan.review.{index}.tree.title"),
                UiNodeKind::Label,
            )
            .with_text_key("app.agent_plan_tree")
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
        );
    let mut rows = 0usize;
    for (row, name) in review.group_sample.iter().take(3).enumerate() {
        tree = tree.with_child(
            UiNode::new(
                format!("agent.plan.review.{index}.group.{row}"),
                UiNodeKind::Label,
            )
            .with_text_value(format!("Group · {name}"))
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout {
                width_mode: UiSizeMode::Fill,
                ..UiLayout::fit_content()
            }),
        );
        rows += 1;
    }
    for (row, name) in review
        .entity_sample
        .iter()
        .take(PLAN_REVIEW_TREE_ROWS.saturating_sub(rows))
        .enumerate()
    {
        tree = tree.with_child(
            UiNode::new(
                format!("agent.plan.review.{index}.entity.{row}"),
                UiNodeKind::Label,
            )
            .with_text_value(format!("  Entity · {name}"))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout {
                width_mode: UiSizeMode::Fill,
                ..UiLayout::fit_content()
            }),
        );
    }
    if review.group_sample.len() + review.entity_sample.len() > PLAN_REVIEW_TREE_ROWS {
        tree = tree.with_child(
            UiNode::new(
                format!("agent.plan.review.{index}.tree.more"),
                UiNodeKind::Label,
            )
            .with_text_key("app.agent_plan_more")
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
        );
    }
    card = card.with_child(tree);
    let mut actions = UiNode::new(
        format!("agent.plan.review.{index}.actions"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 8.0,
        height_mode: UiSizeMode::Fixed,
        basis: [0.0, 34.0],
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new("agent.plan.review.spacer", UiNodeKind::Panel).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new("agent.plan.cancel", UiNodeKind::Button)
            .with_class("agent-plan-cancel")
            .with_text_key("app.agent_plan_cancel")
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_icon(UiIcon::new(UiIconId::Close).with_tint(tokens.text_muted))
            .with_layout(UiLayout {
                padding: UiSpacing::xy(10.0, 6.0),
                height_mode: UiSizeMode::Fixed,
                basis: [0.0, 30.0],
                ..UiLayout::fit_content()
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                border: tokens.border,
                border_width: 1.0,
                radius: 6.0,
                ..UiStyle::transparent()
            })
            .focusable()
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("app.agent_plan_cancel")
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("agent.plan.cancel:{}", review.plan_id),
            )),
    )
    .with_child(
        UiNode::new("agent.plan.apply", UiNodeKind::Button)
            .with_class("agent-plan-apply")
            .with_text_key("app.agent_plan_apply")
            .with_text_style(UiTextStyle::button(tokens.background))
            .with_icon(UiIcon::new(UiIconId::Play).with_tint(tokens.background))
            .with_layout(UiLayout {
                padding: UiSpacing::xy(12.0, 6.0),
                height_mode: UiSizeMode::Fixed,
                basis: [0.0, 30.0],
                ..UiLayout::fit_content()
            })
            .with_style(UiStyle {
                fill: tokens.accent_hot,
                border: tokens.accent_hot,
                border_width: 1.0,
                radius: 6.0,
                ..UiStyle::transparent()
            })
            .focusable()
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("app.agent_plan_apply")
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("agent.plan.apply:{}", review.plan_id),
            )),
    );
    card = card.with_child(actions);
    card
}
