//! Creative native Agent workspace: project rail, conversation and evidence.

use crate::agent_service::AgentService;
use crate::agent_workspace_layout::{WorkspaceDivider, WorkspaceLayout};
use crate::studio_surface::HubSurfaceProject;
use raf_ai::agent_runtime::AgentToolResult;
use raf_ai::chat::MessageRole;
use raf_core::i18n::t;
use raf_core::{EngineSettings, Language};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction, UiSurface};
use raf_ui::*;

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
    fn label_key(self) -> &'static str {
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
                || result
                    .data
                    .get("mode")
                    .and_then(serde_json::Value::as_str)
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
                .or_else(|| result.data.get("height").and_then(serde_json::Value::as_u64))
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
                .unwrap_or_else(|| {
                    uri.rsplit('/').next().unwrap_or(&uri).to_string()
                });
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
                    compare_lines.push(format!(
                        "{entities} entities / {colliders} colliders"
                    ));
                    compare_lines.push(format!("Grid {grid:.2} m / {overlaps} overlaps"));
                }
                EvidenceKind::Capture => {
                    if let Some(view) = result.data.get("view").and_then(serde_json::Value::as_str) {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceConfirmation {
    StartAll,
    CloseAll,
    CloseProject(std::path::PathBuf),
}

pub(crate) fn workspace_tokens(palette: StudioUiPalette) -> UiTokens {
    let mut tokens = palette.tokens();
    if palette == StudioUiPalette::IndustrialDark {
        tokens.background = [9, 9, 10, 255];
        tokens.surface = [14, 14, 15, 255];
        tokens.surface_alt = [20, 20, 21, 255];
        tokens.surface_raised = [26, 26, 27, 255];
        tokens.border = [48, 48, 50, 255];
    }
    tokens
}

pub(crate) fn loading_ring(palette: StudioUiPalette, step: u8) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut ring = UiNode::new("agent.workspace.spinner", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(32.0, 32.0))
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key("agent_workspace.loading");
    for index in 0..12u8 {
        let angle = f32::from(index) * std::f32::consts::TAU / 12.0;
        let distance = (index + 12 - step % 12) % 12;
        let mut color = tokens.accent_hot;
        color[3] = (220.0 * (1.0 - f32::from(distance) / 15.0)).max(40.0) as u8;
        ring = ring.with_child(
            UiNode::new(
                format!("agent.workspace.spinner.{index}"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::absolute(UiRect::new(
                14.0 + angle.cos() * 11.0,
                14.0 + angle.sin() * 11.0,
                4.0,
                4.0,
            )))
            .with_style(UiStyle {
                fill: color,
                radius: 2.0,
                ..UiStyle::transparent()
            }),
        );
    }
    ring
}

pub(crate) fn button(id: &str, label: &str, command: &str, tokens: &UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("agent-workspace-button")
        .with_layout(UiLayout {
            padding: UiSpacing::xy(12.0, 7.0),
            min_size: [0.0, 32.0],
            ..UiLayout::fit_content()
        })
        .with_text_key(label)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            radius: 6.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_label_key(label)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn label(id: &str, key: &str, tokens: &UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout::fit_content())
}

#[derive(Debug, Clone)]
struct WorldPlanSummary {
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
struct WorldPanelModel {
    project_name: String,
    brief_path: Option<String>,
    brief_rows: Vec<(String, String)>,
    plan: Option<WorldPlanSummary>,
    issues: Vec<String>,
    evidence_count: usize,
    saved_revision: Option<u64>,
    journal_path: String,
}

impl WorldPanelModel {
    fn load(
        project: &raf_core::project::Project,
        panel: &crate::panels::ai_chat::AgentPanel,
    ) -> Self {
        let brief_path = project.path.join(".aura_rafi/world/brief.json");
        let brief_text = std::fs::read_to_string(&brief_path).ok();
        let brief = brief_text
            .as_deref()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
        let brief_rows = brief
            .as_ref()
            .map(world_brief_rows)
            .unwrap_or_default();
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
                if verification.get("status").and_then(serde_json::Value::as_str)
                    == Some("failed")
                {
                    if let Some(failures) = verification
                        .get("failures")
                        .and_then(serde_json::Value::as_array)
                    {
                        for failure in failures
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                        {
                            if seen_issues.insert(failure.to_string()) {
                                issues.push(failure.to_string());
                            }
                        }
                    }
                }
            }
            if plan.is_none() {
                if let (Some(plan_id), Some(preview)) = (
                    result.data.get("plan_id").and_then(serde_json::Value::as_str),
                    result.data.get("preview").and_then(serde_json::Value::as_object),
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
            brief_path: brief_text.as_ref().map(|_| {
                ".aura_rafi/world/brief.json".to_string()
            }),
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

fn world_panel(
    palette: StudioUiPalette,
    language: Language,
    model: &WorldPanelModel,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new("agent.workspace.world", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    let mut header = UiNode::new(
        "agent.workspace.world.header",
        UiNodeKind::Toolbar,
    )
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
                    .with_text_value(format!(
                        "{} · {}",
                        plan.mode,
                        short_world_plan_id(&plan.id)
                    ))
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
    checkpoint_section = checkpoint_section
        .with_child(
            UiNode::new("agent.workspace.world.checkpoints.journal", UiNodeKind::Label)
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

pub(crate) fn build_workspace_surface(
    palette: StudioUiPalette,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    size: [f32; 2],
    scroll: f32,
    step: u8,
    evidence: EvidenceTab,
    review_open: bool,
) -> UiSurface {
    build_workspace_surface_with_layout(
        palette,
        service,
        projects,
        settings,
        size,
        scroll,
        step,
        evidence,
        review_open,
        &WorkspaceLayout::default(),
    )
}

pub(crate) fn build_workspace_surface_with_layout(
    palette: StudioUiPalette,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    size: [f32; 2],
    scroll: f32,
    step: u8,
    evidence: EvidenceTab,
    review_open: bool,
    layout: &WorkspaceLayout,
) -> UiSurface {
    build_workspace_surface_with_laboratory(
        palette, service, projects, settings, size, scroll, step, evidence, review_open, layout,
        service.selected_lab(None),
    )
}

pub(crate) fn build_workspace_surface_with_laboratory(
    palette: StudioUiPalette,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    size: [f32; 2],
    scroll: f32,
    step: u8,
    evidence: EvidenceTab,
    review_open: bool,
    layout: &WorkspaceLayout,
    laboratory: Option<&crate::electronics_controller::NativeElectronicsEditor>,
) -> UiSurface {
    let tokens = workspace_tokens(palette);
    let width = size[0].max(1.0);
    let height = size[1].max(1.0);
    let ready = service.selected_workspace().filter(|w| !w.closing);
    let is_lab = laboratory.is_some();
    let lab_chat = laboratory.is_some_and(|editor| editor.lab.view == crate::electronics_lab::LabView::Chat);
    let geometry = layout.resolve(
        size,
        review_open && ready.is_some() && !is_lab,
        ready.is_some_and(|w| w.panel.borrow().sidebar_open),
    );
    let rail = geometry.projects;
    let review = geometry.review.map_or(0.0, |r| r.width);
    let mut header = UiNode::new("agent.workspace.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 12.0,
            padding: UiSpacing::xy(18.0, 6.0),
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, width, 52.0))
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        })
        .with_child(button(
            "agent.workspace.back",
            if is_lab { "lab.back" } else { "agent_workspace.projects" },
            "agent.workspace.back",
            &tokens,
        ))
        .with_child(
            UiNode::new("agent.workspace.title", UiNodeKind::Label)
                .with_text_key(if is_lab { "lab.title" } else { "agent_workspace.title" })
                .with_event(UiEventBinding::command(
                    UiEventKind::DragStart,
                    "window.drag",
                ))
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(
            button(
                "agent.workspace.review",
                "agent_workspace.review",
                "agent.workspace.review",
                &tokens,
            )
            .disabled(ready.is_none()),
        )
        .with_child(button(
            "agent.workspace.settings",
                if is_lab { "lab.provider" } else { "app.settings_menu" },
            "agent.workspace.settings",
            &tokens,
        ))
        .with_child(
            button(
                "agent.workspace.editor",
                "agent_workspace.open_editor",
                "agent.workspace.editor",
                &tokens,
            )
            .disabled(ready.is_none()),
        )
        .with_child({
            let mut preview = button(
                "agent.workspace.preview",
                "agent_workspace.preview_3d",
                "agent.workspace.tab.preview",
                &tokens,
            )
            .with_icon(UiIcon::new(UiIconId::ScenePreview).with_tint(tokens.text))
            .with_tooltip_key("agent_workspace.preview_3d");
            preview.text_key = None;
            preview.layout = UiLayout::fixed(34.0, 32.0);
            preview.disabled(ready.is_none())
        });
    if is_lab {
        header.children.retain(|child| !matches!(child.id.as_str(),
            "agent.workspace.review" | "agent.workspace.preview" | "agent.workspace.editor"));
    }
    header = header
        .with_child(window_button(
            "minimize",
            "agent_workspace.minimize",
            UiIconId::ChevronDown,
            &tokens,
        ))
        .with_child(window_button(
            "maximize",
            "agent_workspace.maximize",
            UiIconId::Focus,
            &tokens,
        ))
        .with_child(window_button(
            "close",
            "agent_workspace.close",
            UiIconId::Close,
            &tokens,
        ));
    if let Some(workspace) = ready.filter(|_| width >= 1350.0) {
        header = header.with_child(
            UiNode::new("agent.workspace.scope", UiNodeKind::Label)
                .with_text_value(workspace.project.name.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fixed(150.0, 24.0)),
        );
    }
    let mut rail_node = UiNode::scroll_view("agent.workspace.projects", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(10.0, 16.0),
            ..UiLayout::absolute(UiRect::new(0.0, 52.0, rail, height - 52.0))
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        })
        .with_child(label(
            "agent.workspace.projects.title",
            "agent_workspace.projects",
            &tokens,
        ))
        .with_child(
            button(
                "agent.workspace.start-all",
                "agent_workspace.start_all",
                "agent.workspace.start-all",
                &tokens,
            )
            .disabled(projects.is_empty()),
        )
        .with_child(
            button(
                "agent.workspace.close-all",
                "agent_workspace.close_all",
                "agent.workspace.close-all",
                &tokens,
            )
            .disabled(!service.has_projects()),
        );
    for (index, project) in projects.iter().enumerate().take(128) {
        let selected = service.is_selected(&project.path);
        let open = service.is_open(&project.path);
        let loading = service.is_loading(&project.path);
        let closing = service.is_closing(&project.path);
        let status_key = if closing {
            "agent_workspace.closing"
        } else if loading {
            "agent_workspace.loading"
        } else if open {
            "agent_workspace.loaded"
        } else {
            "agent_workspace.unloaded"
        };
        let mut row = UiNode::new(
            format!("agent.workspace.project.{index}"),
            UiNodeKind::Button,
        )
        .with_class("agent-workspace-project")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(10.0, 9.0),
            ..UiLayout::fixed(rail - 48.0, 54.0)
        })
        .with_style(UiStyle {
            fill: if selected {
                tokens.surface_raised
            } else {
                [0; 4]
            },
            radius: 6.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::Custom {
                channel: "agent.workspace.project".into(),
                payload: serde_json::json!({"path": project.path}),
            },
        })
        .with_child(
            UiNode::new(
                format!("agent.workspace.project.{index}.heading"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("agent.workspace.project.{index}.status"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout::fixed(7.0, 7.0))
                .with_style(UiStyle {
                    fill: if closing || loading {
                        tokens.warning
                    } else if open {
                        tokens.positive
                    } else {
                        tokens.text_muted
                    },
                    radius: 3.5,
                    ..UiStyle::transparent()
                })
                .with_accessibility_role(UiAccessibilityRole::Status)
                .with_accessibility_label_key(status_key)
                .with_tooltip_key(status_key),
            )
            .with_child(
                UiNode::new(
                    format!("agent.workspace.project.{index}.name"),
                    UiNodeKind::Label,
                )
                .with_text_value(project.name.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed((rail - 87.0).max(1.0), 20.0)
                }),
            ),
        );
        if let Some(workspace) = service.workspace(&project.path) {
            let panel = workspace.panel.borrow();
            row = row.with_child(
                UiNode::new(
                    format!("agent.workspace.project.{index}.detail"),
                    UiNodeKind::Label,
                )
                .with_text_value(format!(
                    "{} · {}",
                    panel.history.sessions.len(),
                    t(
                        if closing {
                            "agent_workspace.closing"
                        } else if panel.has_pending_review() {
                            "agent_workspace.awaiting_review"
                        } else if panel.has_live_output() {
                            "agent_workspace.running"
                        } else {
                            "agent_workspace.conversations"
                        },
                        settings.language
                    )
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
            );
        } else {
            row = row.with_child(
                label(
                    &format!("agent.workspace.project.{index}.detail"),
                    status_key,
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
            );
        }
        let close = button(
            &format!("agent.workspace.project.{index}.close"),
            "agent_workspace.close_project",
            "agent.workspace.close-project",
            &tokens,
        );
        let close = UiNode {
            text_key: None,
            layout: UiLayout::fixed(24.0, 28.0),
            event_handlers: vec![UiEventBinding {
                event: UiEventKind::Click,
                action: UiAction::Custom {
                    channel: "agent.workspace.close-project".into(),
                    payload: serde_json::json!({"path": project.path}),
                },
            }],
            ..close
        }
        .with_icon(UiIcon::new(UiIconId::Close))
        .with_tooltip_key("agent_workspace.close_project")
        .disabled((!open && !loading) || closing);
        rail_node = rail_node.with_child(
            UiNode::new(
                format!("agent.workspace.project.{index}.row"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 4.0,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(rail - 20.0, 54.0)
            })
            .with_child(row)
            .with_child(close),
        );
    }
    let mut root = UiNode::new("agent.workspace.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle {
            fill: tokens.background,
            ..UiStyle::transparent()
        })
        .with_child(header)
        .with_child(rail_node);
    let mut rules = Vec::new();
    if let Some(editor) = laboratory {
        root = root.with_child(crate::electronics_lab_surface::tabs(
            editor, UiRect::new(rail, 52.0, width - rail, 44.0), &tokens,
        ));
        if !lab_chat {
            let body_height = (height - 140.0).max(1.0);
            root = root.with_child(crate::electronics_lab_surface::body(
                editor, UiRect::new(rail, 96.0, width - rail, body_height),
                &tokens, settings.language,
            ));
            if let Some(workspace) = ready {
                root = root.with_child(crate::electronics_lab_surface::assistant(
                    &workspace.panel.borrow(), settings,
                    UiRect::new(rail, 96.0 + body_height, width - rail, 44.0), &tokens,
                ));
            }
        }
    }
    if let Some(workspace) = ready.filter(|_| !is_lab || lab_chat) {
        let top = if is_lab { 96.0 } else { 52.0 };
        let panel = workspace.panel.borrow();
        let mut chat = crate::panels::agent_surface::build_workspace_agent_surface(
            palette,
            &panel,
            settings,
            panel.readiness(settings),
            workspace.project.project_type,
            [(width - rail - review).max(1.0), (height - top).max(1.0)],
            scroll,
            geometry.sessions,
        );
        chat.root.layout = UiLayout::absolute(UiRect::new(
            rail,
            top,
            width - rail - review,
            (height - top).max(1.0),
        ));
        root = root.with_child(chat.root);
        rules.extend(chat.style_sheet.rules);
        if let Some(review_rect) = geometry.review {
            root = root.with_child(evidence_surface(
                palette,
                &panel,
                settings.language,
                review_rect,
                evidence,
                workspace.project.project_type == raf_core::ProjectType::Game,
                Some(&workspace.project),
            ));
            if evidence == EvidenceTab::Preview
                && workspace.project.project_type == raf_core::ProjectType::Game
            {
                root.style.fill = [0; 4];
                root.children.insert(
                    0,
                    UiNode::new("agent.workspace.background", UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(UiRect::new(
                            0.0,
                            52.0,
                            review_rect.x,
                            height - 52.0,
                        )))
                        .with_style(UiStyle {
                            fill: tokens.background,
                            ..UiStyle::transparent()
                        }),
                );
            }
        }
    } else if !is_lab {
        let mut empty =
            UiNode::new("agent.workspace.empty", UiNodeKind::Panel).with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 16.0,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                padding: UiSpacing::same(32.0),
                ..UiLayout::absolute(UiRect::new(rail, 52.0, width - rail, height - 52.0))
            });
        let closing = service
            .selected
            .as_deref()
            .is_some_and(|p| service.is_closing(p));
        let close_error = service.selected_close_error();
        if service.loading() || (closing && close_error.is_none()) {
            empty = empty.with_child(loading_ring(palette, step));
        }
        empty = empty.with_child(label(
            "agent.workspace.empty.title",
            if closing {
                "agent_workspace.closing"
            } else if service.loading() {
                "agent_workspace.loading"
            } else if service.error().is_some() {
                "agent_workspace.load_failed"
            } else {
                "agent_workspace.choose_project"
            },
            &tokens,
        ));
        if let Some(error) = close_error.as_deref().or_else(|| service.error()) {
            empty = empty
                .with_child(
                    UiNode::new("agent.workspace.empty.error", UiNodeKind::Label)
                        .with_text_value(localized_error(error, settings.language))
                        .with_text_style(UiTextStyle::body(tokens.warning))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(button(
                    "agent.workspace.retry",
                    "agent_workspace.retry",
                    "agent.workspace.retry",
                    &tokens,
                ));
        } else if !service.loading() && !closing {
            empty = empty.with_child(label(
                "agent.workspace.empty.hint",
                "agent_workspace.choose_project_hint",
                &tokens,
            ));
        }
        root = root.with_child(empty);
    }
    for (divider, rect) in geometry.dividers().into_iter().filter(|_| !is_lab) {
        let name = match divider {
            WorkspaceDivider::Projects => "projects",
            WorkspaceDivider::Sessions => "sessions",
            WorkspaceDivider::Review => "review",
        };
        root = root.with_child(
            UiNode::new(
                format!("agent.workspace.divider.{name}"),
                UiNodeKind::Button,
            )
            .with_class("agent-workspace-divider")
            .with_layout(UiLayout::absolute(rect).with_z_index(50))
            .with_style(UiStyle::transparent())
            .with_accessibility_label_key("agent_workspace.resize")
            .with_tooltip_key("agent_workspace.resize")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowLeft".into()),
                format!("agent.workspace.resize.{name}:-24"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowRight".into()),
                format!("agent.workspace.resize.{name}:24"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("Home".into()),
                format!("agent.workspace.resize.{name}:reset"),
            ))
            .with_child(
                UiNode::new(
                    format!("agent.workspace.divider.{name}.line"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout::absolute(UiRect::new(3.5, 0.0, 1.0, rect.height)))
                .with_style(UiStyle {
                    fill: tokens.border,
                    ..UiStyle::transparent()
                }),
            ),
        );
    }
    let mut surface = UiSurface::new("editor.agent.workspace", palette, root);
    if is_lab {
        for class in ["lab.signal-target", "lab.nav", "lab.run-primary"] {
            rules.push(UiStyleRule::new(UiStyleSelector::Class(class.into()), UiStylePatch {
                fill: Some(tokens.surface_raised), ..Default::default()
            }).when(UiStyleRuleState::Hovered));
        }
    }
    for class in ["agent-workspace-button", "agent-workspace-project"] {
        rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class(class.into()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    ..Default::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        );
        rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class(class.into()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    ..Default::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        );
    }
    surface.style_sheet.rules = rules;
    for state in [UiStyleRuleState::Hovered, UiStyleRuleState::Focused] {
        surface.style_sheet.rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class("agent-workspace-divider".into()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    ..Default::default()
                },
            )
            .when(state),
        );
    }
    surface
}

pub(crate) fn localized_error(error: &str, language: Language) -> String {
    if error.starts_with("agent_workspace.") {
        t(error, language)
    } else {
        format!(
            "{}: {error}",
            t("agent_workspace.operation_failed", language)
        )
    }
}

/// The modal is window-scoped. Background controls leave the focus/hit tree.
pub(crate) fn with_confirmation(
    mut surface: UiSurface,
    size: [f32; 2],
    confirmation: Option<&WorkspaceConfirmation>,
) -> UiSurface {
    let Some(confirmation) = confirmation else {
        return surface;
    };
    fn disable(node: &mut UiNode) {
        node.disabled = true;
        node.focusable = false;
        node.interactive = false;
        node.event_handlers.clear();
        for child in &mut node.children {
            disable(child);
        }
    }
    for child in &mut surface.root.children {
        disable(child);
    }
    let tokens = workspace_tokens(surface.palette);
    let (title, message, confirm) = match confirmation {
        WorkspaceConfirmation::StartAll => (
            "agent_workspace.start_all",
            "agent_workspace.start_all_warning",
            "agent_workspace.start_all",
        ),
        WorkspaceConfirmation::CloseAll => (
            "agent_workspace.close_all",
            "agent_workspace.close_all_warning",
            "agent_workspace.close_all",
        ),
        WorkspaceConfirmation::CloseProject(_) => (
            "agent_workspace.close_project",
            "agent_workspace.close_project_warning",
            "agent_workspace.close_project",
        ),
    };
    let width = (size[0] - 40.0).clamp(240.0, 500.0);
    let card = UiNode::new("agent.workspace.confirmation.card", UiNodeKind::Panel)
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key(title)
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 16.0,
            padding: UiSpacing::same(24.0),
            basis: [width, 0.0],
            width_mode: UiSizeMode::Fixed,
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("agent.workspace.confirmation.title", UiNodeKind::Label)
                .with_text_key(title)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("agent.workspace.confirmation.message", UiNodeKind::Label)
                .with_text_key(message)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("agent.workspace.confirmation.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 12.0,
                    justify_content: UiJustify::End,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(button(
                    "agent.workspace.confirmation.cancel",
                    "app.cancel",
                    "agent.workspace.confirmation.cancel",
                    &tokens,
                ))
                .with_child(button(
                    "agent.workspace.confirmation.accept",
                    confirm,
                    "agent.workspace.confirmation.accept",
                    &tokens,
                )),
        );
    surface.root.children.push(
        UiNode::new("agent.workspace.confirmation", UiNodeKind::Overlay)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                ..UiLayout::absolute(UiRect::new(0.0, 0.0, size[0], size[1])).with_z_index(100)
            })
            .with_style(UiStyle {
                fill: [0, 0, 0, 170],
                ..UiStyle::transparent()
            })
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "agent.workspace.confirmation.cancel",
            ))
            .with_child(card),
    );
    surface
}

fn window_button(command: &str, key: &str, icon: UiIconId, tokens: &UiTokens) -> UiNode {
    let mut node = button(
        &format!("agent.workspace.window.{command}"),
        key,
        &format!("window.{command}"),
        tokens,
    );
    node.text_key = None;
    node.layout = UiLayout::fixed(30.0, 30.0);
    if command == "minimize" {
        node.with_text_value("—").with_tooltip_key(key)
    } else {
        node.with_icon(UiIcon::new(icon)).with_tooltip_key(key)
    }
}

fn evidence_surface(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    language: Language,
    rect: UiRect,
    tab: EvidenceTab,
    game: bool,
    project: Option<&raf_core::project::Project>,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new("agent.workspace.evidence", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 12.0,
            padding: UiSpacing::same(12.0),
            ..UiLayout::absolute(rect).with_z_index(30)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        });
    let mut tabs =
        UiNode::new("agent.workspace.evidence.tabs", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        });
    for (id, key, value) in [
        ("changes", "agent_workspace.changes", EvidenceTab::Changes),
        ("world", "agent_workspace.world", EvidenceTab::World),
        (
            "artifacts",
            "agent_workspace.artifacts",
            EvidenceTab::Artifacts,
        ),
        (
            "verification",
            "agent_workspace.verification",
            EvidenceTab::Verification,
        ),
        (
            "preview",
            "agent_workspace.preview_3d",
            EvidenceTab::Preview,
        ),
    ] {
        let mut item = button(
            &format!("agent.workspace.tab.{id}"),
            key,
            &format!("agent.workspace.tab.{id}"),
            &tokens,
        )
        .with_accessibility_selected(tab == value);
        item.layout = UiLayout {
            padding: UiSpacing::xy(4.0, 6.0),
            ..UiLayout::fixed(
                match value {
                    EvidenceTab::Changes => 66.0,
                    EvidenceTab::World => 56.0,
                    EvidenceTab::Artifacts => 74.0,
                    EvidenceTab::Verification => 94.0,
                    EvidenceTab::Preview => 34.0,
                },
                32.0,
            )
        };
        if value == EvidenceTab::Preview {
            item.text_key = None;
            item.icon = Some(UiIcon::new(UiIconId::ScenePreview).with_tint(tokens.text));
            item.tooltip_key = Some(key.into());
        }
        item.style.fill = if tab == value {
            tokens.surface_raised
        } else {
            [0; 4]
        };
        tabs = tabs.with_child(item);
    }
    root = root.with_child(tabs);
    if tab == EvidenceTab::World {
        let Some(project) = project.filter(|_| game) else {
            return root.with_child(
                label(
                    "agent.workspace.world.unavailable",
                    "agent_workspace.world_game_only",
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        };
        let model = WorldPanelModel::load(project, panel);
        return root.with_child(world_panel(palette, language, &model));
    }
    if tab == EvidenceTab::Preview {
        if !game {
            return root.with_child(
                label(
                    "agent.workspace.preview.unavailable",
                    "agent_workspace.preview_game_only",
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
        root.style.fill = [0; 4];
        root.layout.padding = UiSpacing::ZERO;
        root.children[0].layout = UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::absolute(UiRect::new(12.0, 12.0, rect.width - 24.0, 34.0))
        };
        for (id, area) in [
            ("top", UiRect::new(0.0, 0.0, rect.width, 86.0)),
            (
                "bottom",
                UiRect::new(0.0, rect.height - 46.0, rect.width, 46.0),
            ),
            ("left", UiRect::new(0.0, 0.0, 8.0, rect.height)),
            (
                "right",
                UiRect::new(rect.width - 8.0, 0.0, 8.0, rect.height),
            ),
        ] {
            root.children.insert(
                0,
                UiNode::new(
                    format!("agent.workspace.preview.background.{id}"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout::absolute(area))
                .with_style(UiStyle {
                    fill: tokens.surface,
                    ..UiStyle::transparent()
                }),
            );
        }
        let icon_button = |id: &str, key: &str, command: &str, icon| {
            let mut node = button(id, key, command, &tokens)
                .with_icon(UiIcon::new(icon).with_tint(tokens.text))
                .with_tooltip_key(key);
            node.text_key = None;
            node.layout = UiLayout::fixed(30.0, 28.0);
            node
        };
        return root
            .with_child(
                UiNode::new("agent.workspace.preview.controls", UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 8.0,
                        align_items: UiAlign::Center,
                        ..UiLayout::absolute(UiRect::new(12.0, 50.0, rect.width - 24.0, 28.0))
                    })
                    .with_child(
                        label(
                            "agent.workspace.preview.readonly",
                            "agent_workspace.preview_readonly",
                            &tokens,
                        )
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        }),
                    )
                    .with_child(icon_button(
                        "agent.workspace.preview.fit",
                        "agent_workspace.preview_fit",
                        "agent.workspace.preview.fit",
                        UiIconId::Focus,
                    ))
                    .with_child(icon_button(
                        "agent.workspace.preview.close",
                        "agent_workspace.preview_close",
                        "agent.workspace.preview.close",
                        UiIconId::Close,
                    )),
            )
            .with_child(
                UiNode::new("agent.workspace.preview.canvas", UiNodeKind::Canvas)
                    .with_layout(UiLayout::absolute(UiRect::new(
                        8.0,
                        86.0,
                        rect.width - 16.0,
                        (rect.height - 132.0).max(1.0),
                    )))
                    .with_style(UiStyle::transparent())
                    .with_accessibility_label_key("agent_workspace.preview_3d"),
            )
            .with_child(
                label(
                    "agent.workspace.preview.help",
                    "agent_workspace.preview_controls",
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::absolute(UiRect::new(
                    12.0,
                    rect.height - 40.0,
                    rect.width - 24.0,
                    36.0,
                ))),
            );
    }
    if tab == EvidenceTab::Artifacts {
        return root.with_child(evidence_gallery(palette, panel, language));
    }
    let mut list = UiNode::scroll_view("agent.workspace.evidence.scroll", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 12.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    let mut count = 0;
    for message in panel
        .runtime
        .messages
        .iter()
        .rev()
        .filter(|m| m.role == MessageRole::Tool)
        .take(24)
    {
        let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
            continue;
        };
        let relevant = match tab {
            EvidenceTab::Changes => result.diff.is_some() || result.changed,
            EvidenceTab::World => false,
            EvidenceTab::Artifacts => {
                result.data.get("artifact").is_some()
                    || result
                        .data
                        .pointer("/receipt/artifacts")
                        .and_then(|v| v.as_array())
                        .is_some_and(|artifacts| !artifacts.is_empty())
                    || result
                        .references
                        .iter()
                        .any(|r| r.contains("generated") || r.contains("agent_artifacts"))
            }
            EvidenceTab::Verification => result.verification.is_some(),
            EvidenceTab::Preview => false,
        };
        if !relevant {
            continue;
        }
        count += 1;
        let id = format!("agent.workspace.evidence.{}", message.id);
        let mut entry = UiNode::new(&id, UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 6.0,
                padding: UiSpacing::same(10.0),
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                radius: 8.0,
                ..UiStyle::transparent()
            })
            .with_child(
                UiNode::new(format!("{id}.summary"), UiNodeKind::Label)
                    .with_text_value(result.summary)
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .with_text_style(UiTextStyle::body(tokens.text))
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        if let Some(revision) = result.revision {
            entry = entry.with_child(
                UiNode::new(format!("{id}.revision"), UiNodeKind::Label)
                    .with_text_value(format!(
                        "{} {revision}",
                        t("agent_workspace.revision", language)
                    ))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            );
        }
        if result
            .data
            .pointer("/persistence/saved")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            entry = entry.with_child(label(
                &format!("{id}.saved"),
                "agent_workspace.autosave",
                &tokens,
            ));
        }
        let detail = match tab {
            EvidenceTab::Changes => result.diff.as_ref(),
            EvidenceTab::World => None,
            EvidenceTab::Artifacts => result
                .data
                .get("artifact")
                .or_else(|| result.data.pointer("/receipt/artifacts")),
            EvidenceTab::Verification => result.verification.as_ref(),
            EvidenceTab::Preview => None,
        };
        if let Some(detail) = detail {
            entry = entry.with_child(button(
                &format!("{id}.expand"),
                "agent_workspace.details",
                &format!("agent.tool.toggle:{}", message.id),
                &tokens,
            ));
            if panel.expanded_tools.contains(&message.id.to_string()) {
                let text = serde_json::to_string_pretty(detail)
                    .unwrap_or_default()
                    .chars()
                    .take(4096)
                    .collect::<String>();
                entry = entry.with_child(
                    UiNode::new(format!("{id}.detail"), UiNodeKind::Label)
                        .with_text_value(text)
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .selectable_text()
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
                );
            }
        }
        for (index, reference) in result.references.iter().take(12).enumerate() {
            entry = entry.with_child(
                UiNode::new(format!("{id}.reference.{index}"), UiNodeKind::Label)
                    .with_text_value(reference.clone())
                    .selectable_text()
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
        if let Some(verification) = result.verification {
            let status = verification
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let key = match status {
                "passed" | "Passed" => "agent_workspace.verified",
                "failed" | "Failed" => "agent_workspace.validation_failed",
                "preview" => "agent_workspace.preview",
                _ => "agent_workspace.unverified",
            };
            entry = entry.with_child(label(&format!("{id}.verification"), key, &tokens));
        }
        list = list.with_child(entry);
    }
    if count == 0 {
        list = list.with_child(
            label(
                "agent.workspace.evidence.empty",
                "agent_workspace.no_evidence",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    root.with_child(list)
}

fn evidence_image_node(
    tokens: &UiTokens,
    artifact: &EvidenceArtifact,
    id: &str,
    height: f32,
) -> UiNode {
    let mut image = UiNode::new(id, UiNodeKind::Image)
        .with_layout(UiLayout {
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, height],
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            radius: 6.0,
            ..UiStyle::transparent()
        });
    raf_ui::components::set_thumbnail_image(
        &mut image,
        evidence_image_key(&artifact.uri),
        raf_ui::UiImageFit::Cover,
        6.0,
    );
    image
}

fn evidence_card(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    artifact: &EvidenceArtifact,
    _index: usize,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let selected_index = panel
        .evidence_selected
        .iter()
        .position(|uri| uri == &artifact.uri);
    let selected = selected_index.is_some();
    let card_id = format!(
        "agent.workspace.evidence.card.{}",
        evidence_image_key(&artifact.uri).replace('.', "_")
    );
    let mut card = UiNode::new(&card_id, UiNodeKind::Button)
        .with_class("agent-evidence-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(7.0),
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, 188.0],
            grow: 1.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: if selected {
                tokens.surface_raised
            } else {
                tokens.surface_alt
            },
            border: if selected {
                tokens.accent_hot
            } else {
                tokens.border
            },
            border_width: if selected { 2.0 } else { 1.0 },
            radius: 8.0,
            ..UiStyle::transparent()
        })
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(artifact.kind.label_key())
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("agent.evidence.select:{}", artifact.uri),
        ))
        .with_child(evidence_image_node(
            &tokens,
            artifact,
            &format!("{card_id}.image"),
            104.0,
        ));
    let title = UiNode::new(format!("{card_id}.label"), UiNodeKind::Label)
        .with_text_value(artifact.label.clone())
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout {
            width_mode: UiSizeMode::Fill,
            ..UiLayout::fit_content()
        });
    card = card.with_child(title);
    if let Some(position) = selected_index {
        card = card.with_child(
            UiNode::new(format!("{card_id}.selected"), UiNodeKind::Label)
                .with_text_value(format!(
                    "{} {}",
                    position + 1,
                    t("agent_workspace.evidence_selected", panel.language)
                ))
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fit_content()),
        );
    }
    if panel.evidence_pinned.as_deref() == Some(artifact.uri.as_str()) {
        card = card.with_child(
            UiNode::new(format!("{card_id}.pinned"), UiNodeKind::Label)
                .with_text_key("agent_workspace.evidence_pinned")
                .with_text_style(UiTextStyle::button(tokens.positive))
                .with_layout(UiLayout::fit_content()),
        );
    }
    card = card.with_child(
        UiNode::new(format!("{card_id}.kind"), UiNodeKind::Label)
            .with_text_value(format!(
                "{} · {}",
                t(artifact.kind.label_key(), panel.language),
                artifact
                    .compare_lines
                    .first()
                    .cloned()
                    .unwrap_or_default()
            ))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout {
                width_mode: UiSizeMode::Fill,
                ..UiLayout::fit_content()
            }),
    );
    card.layout.basis[0] = 0.0;
    card
}

fn evidence_metadata(
    palette: StudioUiPalette,
    language: Language,
    artifact: &EvidenceArtifact,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut block = UiNode::new(
        format!("agent.workspace.evidence.meta.{}", evidence_image_key(&artifact.uri)),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 2.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    block = block.with_child(
        UiNode::new(
            format!("agent.workspace.evidence.meta.label.{}", evidence_image_key(&artifact.uri)),
            UiNodeKind::Label,
        )
        .with_text_value(artifact.label.clone())
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    );
    block = block.with_child(
        UiNode::new(
            format!("agent.workspace.evidence.meta.kind.{}", evidence_image_key(&artifact.uri)),
            UiNodeKind::Label,
        )
        .with_text_value(t(artifact.kind.label_key(), language))
        .with_text_style(UiTextStyle::body(tokens.accent_hot))
        .with_layout(UiLayout::fit_content()),
    );
    for (index, line) in artifact.compare_lines.iter().take(3).enumerate() {
        block = block.with_child(
            UiNode::new(
                format!(
                    "agent.workspace.evidence.meta.line.{}.{}",
                    evidence_image_key(&artifact.uri),
                    index
                ),
                UiNodeKind::Label,
            )
            .with_text_value(line.clone())
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    block
}

fn evidence_detail_panel(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    artifact: &EvidenceArtifact,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let key = evidence_image_key(&artifact.uri);
    let pinned = panel.evidence_pinned.as_deref() == Some(artifact.uri.as_str());
    let pin_label = if pinned {
        "agent_workspace.evidence_unpin"
    } else {
        "agent_workspace.evidence_pin"
    };
    UiNode::new(
        format!("agent.workspace.evidence.detail.{key}"),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        padding: UiSpacing::same(8.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_style(UiStyle {
        fill: tokens.surface_alt,
        border: tokens.border,
        border_width: 1.0,
        radius: 8.0,
        ..UiStyle::transparent()
    })
    .with_child(
        UiNode::new(
            format!("agent.workspace.evidence.detail.title.{key}"),
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_detail")
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_layout(UiLayout::fit_content()),
    )
    .with_child(evidence_image_node(
        &tokens,
        artifact,
        &format!("agent.workspace.evidence.detail.image.{key}"),
        300.0,
    ))
    .with_child(evidence_metadata(palette, panel.language, artifact))
    .with_child(
        UiNode::new(
            format!("agent.workspace.evidence.detail.summary.{key}"),
            UiNodeKind::Label,
        )
        .with_text_value(artifact.summary.clone())
        .with_text_style(UiTextStyle::body(tokens.text))
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    )
    .with_child(button(
        &format!("agent.workspace.evidence.detail.pin.{key}"),
        pin_label,
        &format!("agent.evidence.pin:{}", artifact.uri),
        &tokens,
    ))
}

fn evidence_compare_panel(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    selected: &[&EvidenceArtifact],
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new(
        "agent.workspace.evidence.compare",
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        padding: UiSpacing::same(8.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_style(UiStyle {
        fill: tokens.surface_alt,
        border: tokens.accent_hot,
        border_width: 1.0,
        radius: 8.0,
        ..UiStyle::transparent()
    })
    .with_child(
        UiNode::new(
            "agent.workspace.evidence.compare.title",
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_compare")
        .with_text_style(UiTextStyle::button(tokens.accent_hot))
        .with_layout(UiLayout::fit_content()),
    );
    let mut row = UiNode::new(
        "agent.workspace.evidence.compare.row",
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 8.0,
        align_items: UiAlign::Start,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    for artifact in selected.iter().take(2) {
        let key = evidence_image_key(&artifact.uri);
        let mut column = UiNode::new(
            format!("agent.workspace.evidence.compare.column.{key}"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            grow: 1.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(evidence_image_node(
            &tokens,
            artifact,
            &format!("agent.workspace.evidence.compare.image.{key}"),
            150.0,
        ))
        .with_child(evidence_metadata(palette, panel.language, artifact));
        column = column.with_child(
            UiNode::new(
                format!("agent.workspace.evidence.compare.summary.{key}"),
                UiNodeKind::Label,
            )
            .with_text_value(artifact.summary.clone())
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
        row = row.with_child(column);
    }
    root.with_child(row).with_child(
        UiNode::new(
            "agent.workspace.evidence.compare.hint",
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_compare_hint")
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    )
}

fn evidence_gallery(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    language: Language,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let artifacts = collect_evidence_artifacts(panel);
    let selected = panel
        .evidence_selected
        .iter()
        .filter_map(|uri| artifacts.iter().find(|artifact| &artifact.uri == uri))
        .collect::<Vec<_>>();
    let mut root = UiNode::new(
        "agent.workspace.evidence.gallery",
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 8.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    let mut header = UiNode::new(
        "agent.workspace.evidence.gallery.header",
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            "agent.workspace.evidence.gallery.title",
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_gallery")
        .with_text_style(UiTextStyle::panel_title(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            width_mode: UiSizeMode::Fill,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new(
            "agent.workspace.evidence.gallery.count",
            UiNodeKind::Label,
        )
        .with_text_value(format!("{}", artifacts.len()))
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_layout(UiLayout::fit_content()),
    );
    if !panel.evidence_selected.is_empty() {
        header = header.with_child(button(
            "agent.workspace.evidence.clear",
            "agent_workspace.evidence_clear",
            "agent.evidence.clear",
            &tokens,
        ));
    }
    header = header.with_child(button(
        "agent.workspace.evidence.compare.toggle",
        "agent_workspace.evidence_compare",
        "agent.evidence.compare",
        &tokens,
    ));
    root = root.with_child(header);
    if artifacts.is_empty() {
        return root.with_child(
            label(
                "agent.workspace.evidence.empty",
                "agent_workspace.no_evidence",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    if selected.len() == 2 && panel.evidence_compare {
        let pair: Vec<&EvidenceArtifact> = selected.iter().copied().take(2).collect();
        root = root.with_child(evidence_compare_panel(palette, panel, &pair));
    } else if let Some(first) = selected.first() {
        root = root.with_child(evidence_detail_panel(palette, panel, first));
    } else {
        root = root.with_child(
            label(
                "agent.workspace.evidence.select.hint",
                "agent_workspace.evidence_select_hint",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    let mut scroll = UiNode::scroll_view(
        "agent.workspace.evidence.gallery.scroll",
        UiScrollAxis::Vertical,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 8.0,
        grow: 1.0,
        ..UiLayout::fill(UiFlow::Column)
    });
    for (row, chunk) in artifacts.chunks(2).enumerate() {
        let mut line = UiNode::new(
            format!("agent.workspace.evidence.gallery.row.{row}"),
            UiNodeKind::Toolbar,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 8.0,
            align_items: UiAlign::Start,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
        for (column, artifact) in chunk.iter().enumerate() {
            line = line.with_child(evidence_card(
                palette,
                panel,
                artifact,
                row * 2 + column,
            ));
        }
        if chunk.len() == 1 {
            line = line.with_child(
                UiNode::new(
                    format!("agent.workspace.evidence.gallery.spacer.{row}"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
            );
        }
        scroll = scroll.with_child(line);
    }
    root.with_child(scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node<'a>(root: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if root.id == id {
            return Some(root);
        }
        root.children
            .iter()
            .find_map(|child| node(child, id))
    }

    fn has_prefix(root: &UiNode, prefix: &str) -> bool {
        count_prefix(root, prefix) > 0
    }

    fn count_prefix(root: &UiNode, prefix: &str) -> usize {
        usize::from(root.id.starts_with(prefix))
            + root
                .children
                .iter()
                .map(|child| count_prefix(child, prefix))
                .sum::<usize>()
    }

    fn count_card_roots(root: &UiNode, prefix: &str) -> usize {
        let is_card_root = root
            .id
            .strip_prefix(prefix)
            .is_some_and(|suffix| !suffix.contains('.'));
        usize::from(is_card_root)
            + root
                .children
                .iter()
                .map(|child| count_card_roots(child, prefix))
                .sum::<usize>()
    }

    #[test]
    fn evidence_gallery_renders_capture_map_selection_and_comparison() {
        let mut panel = crate::panels::ai_chat::AgentPanel::default();
        let capture_uri = ".aura_rafi/agent_artifacts/viewport-a.png";
        let map_uri = ".aura_rafi/agent_artifacts/top-down-b.png";
        let capture = AgentToolResult::success(
            "Viewport capture saved",
            serde_json::json!({
                "artifact": {
                    "id": "capture",
                    "kind": "image/png",
                    "uri": capture_uri,
                    "label": "Front capture"
                },
                "width": 960,
                "height": 540,
                "view": "front",
                "visual_evidence": [{"file": capture_uri, "width": 960, "height": 540}]
            }),
        );
        let map = AgentToolResult::success(
            "Top-down map saved",
            serde_json::json!({
                "mode": "top_down_map",
                "artifact": {
                    "id": "map",
                    "kind": "image/png",
                    "uri": map_uri,
                    "label": "District map",
                    "top_down_map": true
                },
                "width": 1024,
                "height": 1024,
                "entities": [{"name":"Dock"},{"name":"Crate"}],
                "colliders": [{"type":"aabb"}],
                "grid": {"spacing": 1.0},
                "overlap_count": 1
            }),
        );
        for result in [capture, map] {
            let mut message = raf_ai::chat::ChatMessage::assistant(
                &serde_json::to_string(&result).unwrap(),
            );
            message.role = MessageRole::Tool;
            panel.runtime.messages.push(message);
        }
        let artifacts = collect_evidence_artifacts(&panel);
        assert_eq!(artifacts.len(), 2);
        assert!(artifacts.iter().any(|artifact| artifact.kind == EvidenceKind::Capture));
        assert!(artifacts.iter().any(|artifact| artifact.kind == EvidenceKind::TopDownMap));
        assert!(artifacts
            .iter()
            .find(|artifact| artifact.kind == EvidenceKind::TopDownMap)
            .unwrap()
            .compare_lines
            .iter()
            .any(|line| line.contains("2 entities")));

        panel.evidence_selected = vec![capture_uri.to_string()];
        panel.evidence_pinned = Some(capture_uri.to_string());
        let detail = evidence_gallery(
            StudioUiPalette::IndustrialDark,
            &panel,
            Language::English,
        );
        assert!(node(&detail, "agent.workspace.evidence.gallery").is_some());
        assert!(has_prefix(&detail, "agent.workspace.evidence.card."));
        assert!(node(
            &detail,
            &format!(
                "agent.workspace.evidence.detail.pin.{}",
                evidence_image_key(capture_uri)
            )
        )
        .is_some());
        assert!(node(
            &detail,
            &format!(
                "agent.workspace.evidence.detail.{}",
                evidence_image_key(capture_uri)
            )
        )
        .is_some());

        panel.evidence_selected = vec![capture_uri.to_string(), map_uri.to_string()];
        panel.evidence_compare = true;
        let compare = evidence_gallery(
            StudioUiPalette::IndustrialDark,
            &panel,
            Language::English,
        );
        assert!(node(&compare, "agent.workspace.evidence.compare").is_some());
        assert!(has_prefix(
            &compare,
            "agent.workspace.evidence.compare.column."
        ));
        assert_eq!(
            count_card_roots(&compare, "agent.workspace.evidence.card."),
            2
        );
    }

    #[test]
    fn world_panel_shows_brief_plan_issues_evidence_and_checkpoints() {
        let root = std::env::temp_dir().join(format!(
            "raf-world-panel-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let project = raf_core::project::Project::create(
            "WorldPanelDemo",
            raf_core::project::ProjectType::Game,
            &root,
        )
        .unwrap();
        let world_dir = project.path.join(".aura_rafi/world");
        std::fs::create_dir_all(&world_dir).unwrap();
        std::fs::write(
            world_dir.join("brief.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "genre": "retail",
                "mood": "night",
                "constraints": ["keep aisles clear", "two exits"]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut panel = crate::panels::ai_chat::AgentPanel::default();
        let plan = AgentToolResult::success(
            "World plan ready",
            serde_json::json!({
                "plan_id": "plan_world_demo",
                "mode": "build",
                "groups": 12,
                "entities": 180,
                "design_audit": {"status": "passed"},
                "overlap_count": 1,
                "preview": {"chunks": 2}
            }),
        );
        let mut warning = AgentToolResult::success(
            "Scene check",
            serde_json::json!({"ok": true}),
        );
        warning
            .warnings
            .push("One aisle is below the requested width.".to_string());
        for result in [plan, warning] {
            let mut message = raf_ai::chat::ChatMessage::assistant(
                &serde_json::to_string(&result).unwrap(),
            );
            message.role = MessageRole::Tool;
            panel.runtime.messages.push(message);
        }
        let model = WorldPanelModel::load(&project, &panel);
        assert_eq!(model.project_name, "WorldPanelDemo");
        assert!(model.brief_path.is_some());
        assert!(model.brief_rows.iter().any(|(key, _)| key == "genre"));
        assert!(model.plan.is_some());
        assert_eq!(model.issues.len(), 1);

        let panel_node = world_panel(
            StudioUiPalette::IndustrialDark,
            Language::English,
            &model,
        );
        for id in [
            "agent.workspace.world",
            "agent.workspace.world.brief",
            "agent.workspace.world.brief.row.0",
            "agent.workspace.world.plan",
            "agent.workspace.world.plan.counts",
            "agent.workspace.world.issues",
            "agent.workspace.world.issue.0",
            "agent.workspace.world.evidence",
            "agent.workspace.world.evidence.open",
            "agent.workspace.world.checkpoints",
        ] {
            assert!(node(&panel_node, id).is_some(), "missing world panel node {id}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn agent_workspace_bulk_modals_block_background_and_fit_both_languages() {
        use raf_render::api_graphic_basic::ui_surface::{CpuUiSurfaceHost, UiSurfaceSession};
        for language in [Language::English, Language::Spanish] {
            for confirmation in [
                WorkspaceConfirmation::StartAll,
                WorkspaceConfirmation::CloseAll,
            ] {
                let settings = EngineSettings {
                    language,
                    ..Default::default()
                };
                let surface = with_confirmation(
                    build_workspace_surface(
                        StudioUiPalette::IndustrialDark,
                        &AgentService::default(),
                        &[],
                        &settings,
                        [900.0, 600.0],
                        0.0,
                        0,
                        EvidenceTab::Changes,
                        false,
                    ),
                    [900.0, 600.0],
                    Some(&confirmation),
                );
                fn focusable(node: &UiNode, result: &mut Vec<String>) {
                    if node.focusable && !node.disabled {
                        result.push(node.id.clone());
                    }
                    for child in &node.children {
                        focusable(child, result);
                    }
                }
                let mut controls = Vec::new();
                focusable(&surface.root, &mut controls);
                assert_eq!(
                    controls,
                    [
                        "agent.workspace.confirmation.cancel",
                        "agent.workspace.confirmation.accept"
                    ]
                );
                let mut session = UiSurfaceSession::default();
                let frame = session.build_frame_with_resolved_text(
                    &surface,
                    900,
                    600,
                    [9, 9, 10, 255],
                    |key| t(key, language),
                );
                let rect = |id: &str| frame.layout_boxes.iter().find(|b| b.id == id).unwrap().rect;
                let card = rect("agent.workspace.confirmation.card");
                let cancel = rect("agent.workspace.confirmation.cancel");
                let accept = rect("agent.workspace.confirmation.accept");
                assert!(
                    card.x >= 0.0
                        && card.y >= 0.0
                        && card.x + card.width <= 900.0
                        && card.y + card.height <= 600.0,
                    "{card:?}"
                );
                assert!(
                    cancel.x + cancel.width <= accept.x + 0.5,
                    "{cancel:?} {accept:?}"
                );
                assert!(accept.y + accept.height <= card.y + card.height);
                if let Some(directory) = std::env::var_os("RAF_AGENT_UI_QA_DIR") {
                    std::fs::create_dir_all(&directory).unwrap();
                    let name = match confirmation {
                        WorkspaceConfirmation::StartAll => "start",
                        _ => "close",
                    };
                    let mut host = CpuUiSurfaceHost::new(surface, [9, 9, 10, 255]);
                    let rendered = host.render([900, 600], |key| t(key, language));
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("agents-{name}-{language:?}.png")),
                        rendered.pixels,
                        900,
                        600,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }

    #[test]
    fn agent_workspace_copy_has_english_and_spanish_parity() {
        let en: serde_json::Value =
            serde_json::from_str(include_str!("../../raf_core/locales/en.json")).unwrap();
        let es: serde_json::Value =
            serde_json::from_str(include_str!("../../raf_core/locales/es.json")).unwrap();
        let en_keys: std::collections::BTreeSet<_> = en
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("agent_workspace."))
            .collect();
        let es_keys: std::collections::BTreeSet<_> = es
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("agent_workspace."))
            .collect();
        assert_eq!(en_keys, es_keys);
        for key in en_keys {
            assert_ne!(t(key, Language::English), *key);
            assert_ne!(t(key, Language::Spanish), *key);
        }
        let first = loading_ring(StudioUiPalette::IndustrialDark, 0);
        let next = loading_ring(StudioUiPalette::IndustrialDark, 1);
        assert_eq!(first.children.len(), 12);
        assert_ne!(first.children[0].style.fill, next.children[0].style.fill);
        assert!(first.accessibility_label_key.is_some());
    }
}
