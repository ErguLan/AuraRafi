use super::super::composer::*;
use super::super::evidence::*;
use super::super::history::*;
use super::super::layout::*;
use super::super::messages::*;
use super::super::models::*;
use super::super::settings::*;
use super::super::tools::*;
use super::super::*;
use super::*;

use crate::panels::ai_chat::{AgentPanel, AgentReadiness};
use raf_ai::agent_runtime::AgentStatus;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurfaceSession};

#[test]
fn dock_shows_the_review_chip_only_when_there_is_room_for_it() {
    let panel = AgentPanel::default();
    let settings = EngineSettings::default();

    let wide = dock_frame(&panel, &settings, 900.0, 520.0);
    assert!(wide
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "agent.review.chip"));

    let narrow = dock_frame(&panel, &settings, 540.0, 520.0);
    assert!(narrow
        .layout_boxes
        .iter()
        .all(|layout| layout.id != "agent.review.chip"));
}

#[test]
fn active_permissions_hide_review_and_approval_controls_in_workspace_and_dock() {
    let mut panel = AgentPanel::default();
    panel.runtime.status = AgentStatus::AwaitingApproval;
    panel
        .runtime
        .pending_calls
        .push(raf_ai::agent_runtime::PendingToolCall::new(
            "legacy-review",
            "scene_create",
            serde_json::json!({}),
        ));
    for language in [Language::English, Language::Spanish] {
        let settings = EngineSettings {
            agent_mode: AgentMode::Active,
            language,
            ..Default::default()
        };
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            for variant in [AgentSurfaceVariant::Embedded, AgentSurfaceVariant::Dock] {
                for width in [540u32, 900] {
                    let surface = build_agent_surface_with_variant(
                        palette,
                        &panel,
                        &settings,
                        AgentReadiness::Ready,
                        ProjectType::Game,
                        [width as f32, 640.0],
                        0.0,
                        0.0,
                        variant,
                        None,
                    );
                    let mut session = UiSurfaceSession::default();
                    let frame = session.build_frame_with_resolved_text(
                        &surface,
                        width,
                        640,
                        [0; 4],
                        |key| t(key, language),
                    );
                    assert!(frame
                        .layout_boxes
                        .iter()
                        .any(|item| item.id == "agent.input"));
                    assert!(frame
                        .layout_boxes
                        .iter()
                        .all(|item| !item.id.starts_with("agent.approval")
                            && !item.id.starts_with("agent.review")));
                }
            }
        }
    }
}

#[test]
fn embedded_header_never_shows_the_dock_review_chip() {
    let panel = AgentPanel::default();
    let settings = EngineSettings::default();
    let surface = build_agent_surface(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        AgentReadiness::ProviderDisabled,
        ProjectType::Game,
        [1100.0, 640.0],
        0.0,
        1.0,
    );
    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 1100, 640, [0, 0, 0, 255], |key| {
            key.to_string()
        });
    assert!(frame
        .layout_boxes
        .iter()
        .all(|layout| layout.id != "agent.review.chip"));
}

#[test]
fn dock_replaces_the_composer_with_the_approval_composer_when_requested() {
    let mut panel = AgentPanel::default();
    panel.runtime.status = AgentStatus::AwaitingApproval;
    panel
        .runtime
        .pending_calls
        .push(raf_ai::agent_runtime::PendingToolCall::new(
            "op-1",
            "scene.create_node",
            serde_json::json!({ "name": "Cube" }),
        ));
    let settings = EngineSettings::default();

    let frame = dock_frame(&panel, &settings, 720.0, 520.0);
    let composer = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.composer")
        .expect("dock approval composer");
    let input = frame
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "agent.input");
    assert!(composer.rect.height >= 180.0 && !input);

    let embedded_surface = build_agent_surface(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        AgentReadiness::ProviderDisabled,
        ProjectType::Game,
        [820.0, 560.0],
        0.0,
        1.0,
    );
    let mut session = UiSurfaceSession::default();
    let embedded = session.build_frame_with_resolved_text(
        &embedded_surface,
        820,
        560,
        [0, 0, 0, 255],
        |key| key.to_string(),
    );
    let embedded_keeps_live_composer = embedded
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "agent.input");
    assert!(embedded_keeps_live_composer);
    assert!(embedded
        .layout_boxes
        .iter()
        .any(|layout| layout.id == "agent.approval.operation"));
}
