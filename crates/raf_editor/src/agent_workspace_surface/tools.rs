//! Tool-result projections: evidence artifacts and the world plan panel.

use super::layout::label;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum EvidenceTab {
    #[default]
    Changes,
    World,
    Artifacts,
    Verification,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceKind {
    Capture,
    ContactSheet,
    TopDownMap,
    Artifact,
}

impl EvidenceKind {
    pub(super) fn label_key(self) -> &'static str {
        match self {
            Self::Capture => "agent_workspace.evidence_capture",
            Self::ContactSheet => "agent_workspace.evidence_contact_sheet",
            Self::TopDownMap => "agent_workspace.evidence_top_down_map",
            Self::Artifact => "agent_workspace.evidence_artifact",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EvidenceArtifact {
    pub uri: String,
    pub label: String,
    pub summary: String,
    pub kind: EvidenceKind,
    pub revision: Option<u64>,
    pub compare_lines: Vec<String>,
}

pub(crate) fn evidence_image_key(uri: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    uri.hash(&mut hasher);
    format!("agent.evidence.{:016x}", hasher.finish())
}

pub(crate) fn attachment_image_key(file: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "attachment".hash(&mut hasher);
    file.hash(&mut hasher);
    format!("agent.attachment.{:016x}", hasher.finish())
}

pub(crate) fn collect_evidence_artifacts(
    panel: &crate::panels::ai_chat::AgentPanel,
) -> Vec<EvidenceArtifact> {
    let mut seen = std::collections::HashSet::new();
    let mut artifacts = Vec::new();
    for message in panel
        .runtime
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == MessageRole::Tool)
        .take(32)
    {
        let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
            continue;
        };
        let mut candidates = Vec::new();
        if let Some(artifact) = result.data.get("artifact") {
            candidates.push(artifact.clone());
        }
        if candidates.is_empty() {
            if let Some(receipt) = result
                .data
                .pointer("/receipt/artifacts")
                .and_then(serde_json::Value::as_array)
            {
                candidates.extend(receipt.iter().cloned());
            }
            candidates.extend(result.artifacts.iter().cloned());
            if let Some(visual) = result
                .data
                .get("visual_evidence")
                .and_then(serde_json::Value::as_array)
            {
                candidates.extend(visual.iter().cloned());
            }
        }
        for candidate in candidates {
            let Some(uri) = candidate
                .get("uri")
                .or_else(|| candidate.get("file"))
                .and_then(serde_json::Value::as_str)
                .map(|uri| uri.replace('\\', "/"))
            else {
                continue;
            };
            if !uri.ends_with(".png") || !seen.insert(uri.clone()) {
                continue;
            }
            let top_down = candidate
                .get("top_down_map")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
                || result.data.get("mode").and_then(serde_json::Value::as_str)
                    == Some("top_down_map")
                || uri.contains("top-down-");
            let contact_sheet = candidate
                .get("contact_sheet")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
                || uri.contains("contact-");
            let kind = if top_down {
                EvidenceKind::TopDownMap
            } else if contact_sheet {
                EvidenceKind::ContactSheet
            } else if uri.contains("viewport-") || result.data.get("view").is_some() {
                EvidenceKind::Capture
            } else {
                EvidenceKind::Artifact
            };
            let width = candidate
                .get("width")
                .and_then(serde_json::Value::as_u64)
                .or_else(|| result.data.get("width").and_then(serde_json::Value::as_u64))
                .unwrap_or(0) as u32;
            let height = candidate
                .get("height")
                .and_then(serde_json::Value::as_u64)
                .or_else(|| {
                    result
                        .data
                        .get("height")
                        .and_then(serde_json::Value::as_u64)
                })
                .unwrap_or(0) as u32;
            let label = candidate
                .get("label")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    result
                        .data
                        .get("artifact")
                        .and_then(|artifact| artifact.get("label"))
                        .and_then(serde_json::Value::as_str)
                })
                .map(str::to_string)
                .unwrap_or_else(|| uri.rsplit('/').next().unwrap_or(&uri).to_string());
            let mut compare_lines = Vec::new();
            if width > 0 && height > 0 {
                compare_lines.push(format!("{width} x {height} px"));
            }
            if let Some(revision) = result.revision {
                compare_lines.push(format!("Revision {revision}"));
            }
            match kind {
                EvidenceKind::TopDownMap => {
                    let entities = result
                        .data
                        .get("entities")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len);
                    let colliders = result
                        .data
                        .get("colliders")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len);
                    let grid = result
                        .data
                        .pointer("/grid/spacing")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    let overlaps = result
                        .data
                        .get("overlap_count")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    compare_lines.push(format!("{entities} entities / {colliders} colliders"));
                    compare_lines.push(format!("Grid {grid:.2} m / {overlaps} overlaps"));
                }
                EvidenceKind::Capture => {
                    if let Some(view) = result.data.get("view").and_then(serde_json::Value::as_str)
                    {
                        compare_lines.push(format!("View {view}"));
                    }
                    if let Some(distance) = result
                        .data
                        .pointer("/camera/distance")
                        .and_then(serde_json::Value::as_f64)
                    {
                        compare_lines.push(format!("Camera {distance:.1} m"));
                    }
                }
                EvidenceKind::ContactSheet => {
                    let frames = result
                        .data
                        .get("frames")
                        .and_then(serde_json::Value::as_array)
                        .map_or(1, Vec::len);
                    compare_lines.push(format!("{frames} angles"));
                }
                EvidenceKind::Artifact => {}
            }
            artifacts.push(EvidenceArtifact {
                uri,
                label,
                summary: result.summary.clone(),
                kind,
                revision: result.revision,
                compare_lines,
            });
            if artifacts.len() >= 24 {
                return artifacts;
            }
        }
    }
    artifacts
}

#[derive(Debug, Clone)]
pub(super) struct WorldPlanSummary {
    id: String,
    mode: String,
    groups: u64,
    entities: u64,
    chunks: u64,
    design_status: String,
    overlap_count: u64,
    revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct WorldPanelModel {
    pub(super) project_name: String,
    pub(super) brief_path: Option<String>,
    pub(super) brief_rows: Vec<(String, String)>,
    pub(super) plan: Option<WorldPlanSummary>,
    pub(super) issues: Vec<String>,
    pub(super) evidence_count: usize,
    pub(super) saved_revision: Option<u64>,
    pub(super) journal_path: String,
}
impl WorldPanelModel {
    pub(super) fn load(
        project: &raf_core::project::Project,
        panel: &crate::panels::ai_chat::AgentPanel,
    ) -> Self {
        let brief_path = project.path.join(".aura_rafi/world/brief.json");
        let brief_text = std::fs::read_to_string(&brief_path).ok();
        let brief = brief_text
            .as_deref()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
        let brief_rows = brief.as_ref().map(world_brief_rows).unwrap_or_default();
        let mut plan = None;
        let mut issues = Vec::new();
        let mut seen_issues = std::collections::HashSet::new();
        for message in panel
            .runtime
            .messages
            .iter()
            .rev()
            .filter(|message| message.role == MessageRole::Tool)
            .take(24)
        {
            let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
                continue;
            };
            for warning in &result.warnings {
                if seen_issues.insert(warning.clone()) {
                    issues.push(warning.clone());
                }
            }
            if let Some(verification) = result.verification.as_ref() {
                if verification
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
                {
                    if let Some(failures) = verification
                        .get("failures")
                        .and_then(serde_json::Value::as_array)
                    {
                        for failure in failures.iter().filter_map(serde_json::Value::as_str) {
                            if seen_issues.insert(failure.to_string()) {
                                issues.push(failure.to_string());
                            }
                        }
                    }
                }
            }
            if plan.is_none() {
                if let (Some(plan_id), Some(preview)) = (
                    result
                        .data
                        .get("plan_id")
                        .and_then(serde_json::Value::as_str),
                    result
                        .data
                        .get("preview")
                        .and_then(serde_json::Value::as_object),
                ) {
                    plan = Some(WorldPlanSummary {
                        id: plan_id.to_string(),
                        mode: result
                            .data
                            .get("mode")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("build")
                            .to_string(),
                        groups: result
                            .data
                            .get("groups")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        entities: result
                            .data
                            .get("entities")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        chunks: preview
                            .get("chunks")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        design_status: result
                            .data
                            .get("design_audit")
                            .and_then(|audit| audit.get("status"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("not_run")
                            .to_string(),
                        overlap_count: result
                            .data
                            .get("overlap_count")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        revision: result.revision,
                    });
                }
            }
        }
        issues.truncate(8);
        Self {
            project_name: project.name.clone(),
            brief_path: brief_text
                .as_ref()
                .map(|_| ".aura_rafi/world/brief.json".to_string()),
            brief_rows,
            plan,
            issues,
            evidence_count: collect_evidence_artifacts(panel).len(),
            saved_revision: panel.saved_revision,
            journal_path: ".aura_rafi/world/journal.jsonl".to_string(),
        }
    }
}

fn short_world_plan_id(plan_id: &str) -> String {
    plan_id
        .strip_prefix("plan_")
        .unwrap_or(plan_id)
        .chars()
        .take(12)
        .collect()
}

fn compact_world_value(value: &serde_json::Value) -> String {
    let text = match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .take(12)
            .map(compact_world_value)
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::Object(object) => object
            .iter()
            .take(8)
            .map(|(key, value)| format!("{key}={}", compact_world_value(value)))
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    };
    text.chars().take(120).collect()
}

fn world_brief_rows(brief: &serde_json::Value) -> Vec<(String, String)> {
    let Some(object) = brief.as_object() else {
        return Vec::new();
    };
    let preferred = [
        "genre",
        "mood",
        "style",
        "palette",
        "camera",
        "core_loop",
        "constraints",
        "acceptance",
        "metrics",
    ];
    let mut rows = Vec::new();
    for key in preferred {
        if let Some(value) = object.get(key) {
            rows.push((key.to_string(), compact_world_value(value)));
        }
    }
    for (key, value) in object.iter().take(12) {
        if !preferred.contains(&key.as_str()) {
            rows.push((key.clone(), compact_world_value(value)));
        }
    }
    rows.truncate(16);
    rows
}

fn world_section(
    palette: StudioUiPalette,
    id: &str,
    title_key: &str,
    language: Language,
) -> (UiNode, UiTokens) {
    let tokens = workspace_tokens(palette);
    let node = UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(9.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            border_width: 1.0,
            radius: 8.0,
            ..UiStyle::transparent()
        })
        .with_child(label(&format!("{id}.title"), title_key, &tokens));
    let _ = language;
    (node, tokens)
}

pub(super) fn world_panel(
    palette: StudioUiPalette,
    language: Language,
    model: &WorldPanelModel,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new("agent.workspace.world", UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 8.0,
        grow: 1.0,
        ..UiLayout::fill(UiFlow::Column)
    });
    let mut header = UiNode::new("agent.workspace.world.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.workspace.world.title", UiNodeKind::Label)
                .with_text_key("agent_workspace.world")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    width_mode: UiSizeMode::Fill,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(
            UiNode::new("agent.workspace.world.project", UiNodeKind::Label)
                .with_text_value(model.project_name.clone())
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fit_content()),
        );
    if let Some(revision) = model.saved_revision {
        header = header.with_child(
            UiNode::new("agent.workspace.world.revision", UiNodeKind::Label)
                .with_text_value(format!("Rev {revision}"))
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    root = root.with_child(header);
    let (mut brief_section, _) = world_section(
        palette,
        "agent.workspace.world.brief",
        "agent_workspace.world_brief",
        language,
    );
    if model.brief_rows.is_empty() {
        brief_section = brief_section.with_child(
            label(
                "agent.workspace.world.brief.empty",
                "agent_workspace.world_brief_empty",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    } else {
        for (index, (key, value)) in model.brief_rows.iter().take(10).enumerate() {
            brief_section = brief_section.with_child(
                UiNode::new(
                    format!("agent.workspace.world.brief.row.{index}"),
                    UiNodeKind::Label,
                )
                .with_text_value(format!("{key}: {value}"))
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
    }
    if let Some(path) = model.brief_path.as_ref() {
        brief_section = brief_section.with_child(
            UiNode::new("agent.workspace.world.brief.path", UiNodeKind::Label)
                .with_text_value(path.clone())
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    root = root.with_child(brief_section);
    let (mut issues_section, _) = world_section(
        palette,
        "agent.workspace.world.issues",
        "agent_workspace.world_issues",
        language,
    );
    if model.issues.is_empty() {
        issues_section = issues_section.with_child(label(
            "agent.workspace.world.issues.empty",
            "agent_workspace.world_no_issues",
            &tokens,
        ));
    } else {
        for (index, issue) in model.issues.iter().take(5).enumerate() {
            issues_section = issues_section.with_child(
                UiNode::new(
                    format!("agent.workspace.world.issue.{index}"),
                    UiNodeKind::Label,
                )
                .with_text_value(format!("- {issue}"))
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
    }
    root = root.with_child(issues_section);
    let (mut plan_section, _) = world_section(
        palette,
        "agent.workspace.world.plan",
        "agent_workspace.world_last_plan",
        language,
    );
    if let Some(plan) = model.plan.as_ref() {
        plan_section = plan_section
            .with_child(
                UiNode::new("agent.workspace.world.plan.id", UiNodeKind::Label)
                    .with_text_value(format!("{} · {}", plan.mode, short_world_plan_id(&plan.id)))
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_layout(UiLayout::fit_content()),
            )
            .with_child(
                UiNode::new("agent.workspace.world.plan.counts", UiNodeKind::Label)
                    .with_text_value(format!(
                        "{} groups · {} entities · {} chunks",
                        plan.groups, plan.entities, plan.chunks
                    ))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            )
            .with_child(
                UiNode::new("agent.workspace.world.plan.validation", UiNodeKind::Label)
                    .with_text_value(format!(
                        "{} · {} overlaps",
                        plan.design_status, plan.overlap_count
                    ))
                    .with_text_style(UiTextStyle::body(
                        if plan.design_status == "failed" || plan.overlap_count > 0 {
                            tokens.warning
                        } else {
                            tokens.positive
                        },
                    ))
                    .with_layout(UiLayout::fit_content()),
            );
        if let Some(revision) = plan.revision {
            plan_section = plan_section.with_child(
                UiNode::new("agent.workspace.world.plan.revision", UiNodeKind::Label)
                    .with_text_value(format!("Revision {revision}"))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            );
        }
    } else {
        plan_section = plan_section.with_child(label(
            "agent.workspace.world.plan.empty",
            "agent_workspace.world_no_plan",
            &tokens,
        ));
    }
    root = root.with_child(plan_section);
    let (mut evidence_section, _) = world_section(
        palette,
        "agent.workspace.world.evidence",
        "agent_workspace.world_evidence",
        language,
    );
    evidence_section = evidence_section
        .with_child(
            UiNode::new("agent.workspace.world.evidence.count", UiNodeKind::Label)
                .with_text_value(format!("{}", model.evidence_count))
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(button(
            "agent.workspace.world.evidence.open",
            "agent_workspace.world_open_evidence",
            "agent.workspace.tab.artifacts",
            &tokens,
        ));
    root = root.with_child(evidence_section);
    let (mut checkpoint_section, _) = world_section(
        palette,
        "agent.workspace.world.checkpoints",
        "agent_workspace.world_checkpoints",
        language,
    );
    checkpoint_section = checkpoint_section.with_child(
        UiNode::new(
            "agent.workspace.world.checkpoints.journal",
            UiNodeKind::Label,
        )
        .with_text_value(model.journal_path.clone())
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    );
    if let Some(revision) = model.saved_revision {
        checkpoint_section = checkpoint_section.with_child(
            UiNode::new("agent.workspace.world.checkpoints.saved", UiNodeKind::Label)
                .with_text_value(format!(
                    "{} {revision}",
                    t("agent_workspace.world_saved_revision", language)
                ))
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fit_content()),
        );
    }
    root = root.with_child(checkpoint_section);
    root
}
