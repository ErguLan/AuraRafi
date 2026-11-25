use super::evidence::evidence_gallery;
use super::tools::{world_panel, WorldPanelModel};
use super::*;

fn node<'a>(root: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| node(child, id))
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
        let mut message =
            raf_ai::chat::ChatMessage::assistant(&serde_json::to_string(&result).unwrap());
        message.role = MessageRole::Tool;
        panel.runtime.messages.push(message);
    }
    let artifacts = collect_evidence_artifacts(&panel);
    assert_eq!(artifacts.len(), 2);
    assert!(artifacts
        .iter()
        .any(|artifact| artifact.kind == EvidenceKind::Capture));
    assert!(artifacts
        .iter()
        .any(|artifact| artifact.kind == EvidenceKind::TopDownMap));
    assert!(artifacts
        .iter()
        .find(|artifact| artifact.kind == EvidenceKind::TopDownMap)
        .unwrap()
        .compare_lines
        .iter()
        .any(|line| line.contains("2 entities")));

    panel.evidence_selected = vec![capture_uri.to_string()];
    panel.evidence_pinned = Some(capture_uri.to_string());
    let detail = evidence_gallery(StudioUiPalette::IndustrialDark, &panel, Language::English);
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
    let compare = evidence_gallery(StudioUiPalette::IndustrialDark, &panel, Language::English);
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
    let root =
        std::env::temp_dir().join(format!("raf-world-panel-{}", uuid::Uuid::new_v4().simple()));
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
    let mut warning = AgentToolResult::success("Scene check", serde_json::json!({"ok": true}));
    warning
        .warnings
        .push("One aisle is below the requested width.".to_string());
    for result in [plan, warning] {
        let mut message =
            raf_ai::chat::ChatMessage::assistant(&serde_json::to_string(&result).unwrap());
        message.role = MessageRole::Tool;
        panel.runtime.messages.push(message);
    }
    let model = WorldPanelModel::load(&project, &panel);
    assert_eq!(model.project_name, "WorldPanelDemo");
    assert!(model.brief_path.is_some());
    assert!(model.brief_rows.iter().any(|(key, _)| key == "genre"));
    assert!(model.plan.is_some());
    assert_eq!(model.issues.len(), 1);

    let panel_node = world_panel(StudioUiPalette::IndustrialDark, Language::English, &model);
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
        assert!(
            node(&panel_node, id).is_some(),
            "missing world panel node {id}"
        );
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
        serde_json::from_str(include_str!("../../../raf_core/locales/en.json")).unwrap();
    let es: serde_json::Value =
        serde_json::from_str(include_str!("../../../raf_core/locales/es.json")).unwrap();
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
