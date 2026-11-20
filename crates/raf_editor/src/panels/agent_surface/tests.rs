mod layout;

use super::composer::*;
use super::evidence::*;
use super::history::*;
use super::layout::*;
use super::messages::*;
use super::models::*;
use super::settings::*;
use super::tools::*;
use super::*;

use chrono::Utc;
use raf_ai::agent_runtime::{AgentStatus, AgentToolResult};
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::ai::AgentMode;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    CpuUiSurfaceHost, StudioUiPalette, UiAction, UiFlow, UiInputState, UiLayout, UiNode,
    UiNodeKind, UiSurface, UiSurfaceSession,
};
use uuid::Uuid;

use crate::panels::ai_chat::{AgentPanel, AgentReadiness};

fn message(content: &str) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        role: MessageRole::Assistant,
        content: content.to_string(),
        images: Vec::new(),
        reasoning: None,
        timestamp: Utc::now(),
        tool_calls: None,
    }
}

#[test]
fn plan_review_card_exposes_counts_tree_validations_and_actions() {
    let result = AgentToolResult::success(
        "World plan ready",
        serde_json::json!({
            "status": "planned",
            "plan_id": "plan_demo_1234567890",
            "plan_hash": "abc123",
            "mode": "build",
            "groups": 18,
            "entities": 240,
            "design_audit": {"status": "passed"},
            "overlap_count": 2,
            "pipeline": {
                "status": "warning",
                "gates": [
                    {"gate": "budget", "status": "passed", "detail": {}},
                    {"gate": "navigation", "status": "passed", "detail": {}}
                ]
            },
            "preview": {
                "chunks": 3,
                "group_sample": [{"name":"Test District"},{"name":"Retail Row"}],
                "entity_sample": [{"name":"Floor"},{"name":"Shelf_A"},{"name":"Checkout"}]
            }
        }),
    );
    let message = tool_message(&serde_json::to_string(&result).unwrap());
    let review = plan_review_from_message(&message).expect("recognized plan result");
    assert_eq!(review.plan_id, "plan_demo_1234567890");
    assert_eq!(review.groups, 18);
    assert_eq!(review.entities, 240);
    assert_eq!(review.chunks, 3);
    assert_eq!(review.design_status, "passed");
    assert_eq!(review.overlap_count, 2);

    let panel = AgentPanel::default();
    let card = plan_review_card(
        StudioUiPalette::IndustrialDark,
        &panel,
        &review,
        760.0,
        0,
    );
    for id in [
        "agent.plan.review.0",
        "agent.plan.metric.groups.0",
        "agent.plan.metric.entities.0",
        "agent.plan.metric.chunks.0",
        "agent.plan.validation.design.0",
        "agent.plan.validation.overlaps.0",
        "agent.plan.validation.budget.0",
        "agent.plan.validation.navigation.0",
        "agent.plan.review.0.tree",
        "agent.plan.review.0.group.0",
        "agent.plan.review.0.entity.0",
        "agent.plan.apply",
        "agent.plan.cancel",
    ] {
        assert!(node(&card, id).is_some(), "missing plan card node {id}");
    }
    let apply = node(&card, "agent.plan.apply").unwrap();
    assert!(apply.focusable);
    assert!(apply.event_handlers.iter().any(|binding| {
        matches!(&binding.action, UiAction::Command { name } if name == "agent.plan.apply:plan_demo_1234567890")
    }));
    let cancel = node(&card, "agent.plan.cancel").unwrap();
    assert!(cancel.event_handlers.iter().any(|binding| {
        matches!(&binding.action, UiAction::Command { name } if name == "agent.plan.cancel:plan_demo_1234567890")
    }));
}

#[test]
fn plan_review_card_is_wired_into_history_and_cancel_hides_it() {
    let mut user = message("Build a test district");
    user.role = MessageRole::User;
    let result = AgentToolResult::success(
        "World plan ready",
        serde_json::json!({
            "status": "planned",
            "plan_id": "plan_history_demo",
            "mode": "build",
            "groups": 4,
            "entities": 40,
            "design_audit": {"status": "passed"},
            "overlap_count": 0,
            "preview": {
                "chunks": 1,
                "group_sample": [{"name":"District"}],
                "entity_sample": [{"name":"Dock"}]
            }
        }),
    );
    let plan = tool_message(&serde_json::to_string(&result).unwrap());
    let messages = vec![&user, &plan];
    let entries = history_entries(&messages, &std::collections::HashSet::new());
    assert!(entries.iter().any(|entry| matches!(entry, HistoryEntry::Plan(1))));

    let mut panel = AgentPanel::default();
    let surface = build_history(
        StudioUiPalette::IndustrialDark,
        &panel,
        ProjectType::Game,
        &messages,
        760.0,
        0.0,
        AgentSurfaceVariant::Embedded,
        |_| false,
    );
    assert!(node(&surface, "agent.plan.review.1").is_some());

    panel.dismissed_plans.insert("plan_history_demo".to_string());
    let surface = build_history(
        StudioUiPalette::IndustrialDark,
        &panel,
        ProjectType::Game,
        &messages,
        760.0,
        0.0,
        AgentSurfaceVariant::Embedded,
        |_| false,
    );
    assert!(node(&surface, "agent.plan.review.1").is_none());
}

#[test]
fn harness_turn_groups_keep_errors_and_the_final_answer_outside_details() {
    let mut user = message("Build a world");
    user.role = MessageRole::User;
    let comment = message("Inspecting first");
    let mut success = message("Created objects");
    success.role = MessageRole::Tool;
    let mut failure = message("Error: layout check failed");
    failure.role = MessageRole::Tool;
    let answer = message("The layout needs one correction.");
    let messages = [&user, &comment, &success, &failure, &answer];
    let mut expanded = std::collections::HashSet::new();
    assert_eq!(
        history_entries(&messages, &expanded),
        vec![
            HistoryEntry::Message(0),
            HistoryEntry::Activity {
                first: 1,
                count: 2,
                expanded: false
            },
            HistoryEntry::Message(3),
            HistoryEntry::Message(4)
        ]
    );
    expanded.insert(format!("group:{}", comment.id));
    let entries = history_entries(&messages, &expanded);
    assert_eq!(entries.len(), 6);
    assert_eq!(entries.last(), Some(&HistoryEntry::Message(4)));
}

#[test]
fn harness_reasoning_visibility_and_expansion_follow_settings() {
    let mut panel = AgentPanel::default();
    let mut answer = message("Done");
    answer.reasoning = Some(raf_ai::chat::ProviderReasoning {
        endpoint: "https://openrouter.ai/api/v1".into(), model: "test/model".into(),
        transport: String::new(), text: "Provider reasoning".into(), details: Vec::new(),
    });
    let id = format!("agent.reasoning.{}", answer.id);
    let render = |panel: &AgentPanel| build_history(StudioUiPalette::IndustrialDark, panel,
        ProjectType::Game, &[&answer], 600.0, 0.0, AgentSurfaceVariant::Embedded, |_| false);
    let collapsed = render(&panel);
    assert!(node(&collapsed, &format!("{id}.toggle")).is_some());
    assert!(node(&collapsed, &format!("{id}.text")).is_none());
    panel.expanded_tools.insert(format!("reasoning:{}", answer.id));
    assert!(node(&render(&panel), &format!("{id}.text")).is_some());
    panel.show_reasoning = false;
    assert!(node(&render(&panel), &format!("{id}.toggle")).is_none());
}

#[test]
fn harness_markdown_preserves_code_and_user_literals() {
    let source = "## Summary\nDone\n```rust\n  let snake_case = \"**literal**\";\n```";
    let tiles = message_text_tiles(source, MessageRole::Assistant);
    assert_eq!(tiles[0], ("Summary".into(), MessageTileKind::Heading));
    assert_eq!(
        tiles.last().unwrap(),
        &(
            "  let snake_case = \"**literal**\";".into(),
            MessageTileKind::Code
        )
    );
    assert_eq!(
        message_text_tiles("**literal**", MessageRole::User)[0].0,
        "**literal**"
    );
}

#[test]
fn harness_composer_exposes_removable_images_and_automatic_output() {
    let mut panel = AgentPanel::default();
    panel.history.start_session("Images");
    panel
        .history
        .active_session_mut()
        .unwrap()
        .draft_images
        .push(raf_ai::chat_media::ChatImage {
            file: "fixture.png".into(),
            width: 320,
            height: 240,
        });
    let settings = EngineSettings::default();
    let composer = build_composer(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        AgentReadiness::Ready,
        720.0,
        0,
        AgentSurfaceVariant::Embedded,
    );
    assert!(node(&composer, "agent.image.paste").is_some());
    assert!(node(&composer, "agent.image.0.thumb").is_some());
    assert!(node(&composer, "agent.image.0.remove").is_some());
    assert!(node(&composer, "agent.reasoning.cycle").is_some());
    assert_eq!(composer_media_height(&panel), 30.0);
    assert!(metrics_text(&panel, Language::English, 0, 0).contains("Automatic"));
}

#[test]
fn harness_cpu_layout_keeps_grouped_activity_and_composer_inside_the_panel() {
    // UI fixture, not evidence of a model run or of native window input.
    let mut panel = AgentPanel::default();
    panel.language = Language::Spanish;
    panel.document_name = "Vista de prueba".into();
    panel.sidebar_open = false;
    let mut user = message("Revisa la estructura de esta escena.");
    user.role = MessageRole::User;
    let comment = message("Consulto la escena y reviso sus objetos.");
    let mut tool = message(
        &serde_json::to_string(&AgentToolResult::success(
            "Consulta de escena",
            serde_json::json!({}),
        ))
        .unwrap(),
    );
    tool.role = MessageRole::Tool;
    panel.runtime.messages = vec![user, comment, tool.clone(), {let mut second=tool;second.id=Uuid::new_v4();second},
        message("## Resumen\nRespuesta de prueba con actividad agrupada.\n```rust\nlet scene_name = \"World\";\n```")];
    let mut settings = EngineSettings::default();
    settings.language = Language::Spanish;
    settings.agent_mode = AgentMode::Inspect;
    let surface = build_dock_agent_surface(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        AgentReadiness::Ready,
        ProjectType::Game,
        [760.0, 640.0],
        0.0,
        0.0,
    );
    let mut host = CpuUiSurfaceHost::new(surface, [9, 9, 10, 255]);
    let rendered = host.render([760, 640], |key| t(key, Language::Spanish));
    let rect = |id: &str| {
        rendered
            .frame
            .layout_boxes
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .rect
    };
    let composer = rect("agent.composer");
    let input = rect("agent.input");
    let reasoning = rect("agent.reasoning.cycle");
    assert!(input.y >= composer.y && input.bottom() <= composer.bottom() + 1.0);
    assert!(reasoning.x >= composer.x && reasoning.right() <= composer.right() + 1.0);
    assert!(composer.bottom() <= 641.0);
    assert!(rendered.frame.text_requests.iter().any(|request| request
        .text_key
        .contains("Vista de prueba \u{00b7}")));
    assert!(rendered
        .frame
        .layout_boxes
        .iter()
        .any(|item| item.id.starts_with("agent.activity.")));
    assert!(!rendered
        .frame
        .layout_boxes
        .iter()
        .any(|item| item.id == "agent.message.2.row"));
    let path = std::env::temp_dir().join(format!("raf-harness-ui-{}.png", Uuid::new_v4()));
    image::save_buffer(&path, rendered.pixels, 760, 640, image::ColorType::Rgba8).unwrap();
    eprintln!("Harness CPU fixture: {}", path.display());
}

#[test]
fn harness_workspace_tools_are_compact_and_failures_remain_visible() {
    let mut tool = message(
        &serde_json::to_string(&AgentToolResult::success(
            "Created objects",
            serde_json::json!({}),
        ))
        .unwrap(),
    );
    tool.role = MessageRole::Tool;
    let height = estimated_message_height_with_variant(
        &tool,
        700.0,
        AgentSurfaceVariant::Embedded,
        false,
    );
    assert!(height < 70.0);
    let card = message_card_with_variant(
        StudioUiPalette::IndustrialDark,
        &tool,
        700.0,
        0,
        MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
        AgentSurfaceVariant::Embedded,
        false,
    );
    assert!(node(&card, "agent.message.0.row.toggle").is_some());
    assert!(node(&card, "agent.message.0.copy").is_none());
    tool.content = "Error: Verification failed".into();
    assert!(tool_needs_attention(&tool));
    assert!(
        estimated_message_height_with_variant(
            &tool,
            700.0,
            AgentSurfaceVariant::Embedded,
            false
        ) > height
    );
}

fn node<'a>(root: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| node(child, id))
}

#[test]
fn assistant_messages_expose_selectable_text_and_a_copy_action() {
    let message = message("response from the agent");
    let card = message_card_with_variant(
        StudioUiPalette::IndustrialDark,
        &message,
        700.0,
        0,
        MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
        AgentSurfaceVariant::Embedded,
        false,
    );
    let text = node(&card, "agent.message.0.body.text").expect("message text");
    let copy = node(&card, "agent.message.0.copy").expect("copy button");

    assert!(text.text_selectable);
    assert!(text.focusable);
    assert!(copy.focusable);
    assert!(copy.event_handlers.iter().any(|binding| matches!(
        &binding.action,
        UiAction::SetClipboard { text } if text == "response from the agent"
    )));
}

#[test]
fn virtual_range_keeps_small_overscan_for_long_history() {
    let messages = (0..200)
        .map(|index| message(&format!("message {index}")))
        .collect::<Vec<_>>();
    let heights = messages
        .iter()
        .map(|message| {
            estimated_message_height_with_variant(
                message,
                700.0,
                AgentSurfaceVariant::Embedded,
                false,
            )
        })
        .collect::<Vec<_>>();
    let range = visible_message_range(&heights, 5_000.0, 360.0);
    assert!(range.end - range.start < 30);
    assert!(range.start > 0);
}

#[test]
fn virtual_history_spacers_do_not_add_a_trailing_gap() {
    let heights = [100.0, 200.0, 300.0];

    assert_eq!(estimated_prefix_height(&heights, 2), 308.0);
    assert_eq!(estimated_suffix_height(&heights, 1), 508.0);
}

#[test]
fn long_message_height_estimate_preserves_its_full_scroll_extent() {
    let message = message(&"Long Agent line ".repeat(1_000));
    let height = estimated_message_height_with_variant(
        &message,
        700.0,
        AgentSurfaceVariant::Embedded,
        false,
    );

    assert!(height > 1_000.0);
}

#[test]
fn message_text_is_bounded_without_splitting_unicode() {
    let source = "\u{00e9}".repeat(MAX_VISIBLE_MESSAGE_CHARS + 10);
    let bounded = bounded_text(&source, MAX_VISIBLE_MESSAGE_CHARS);
    assert_eq!(bounded.chars().count(), MAX_VISIBLE_MESSAGE_CHARS + 1);
    assert!(bounded.ends_with('\u{2026}'));
}

#[test]
fn lightweight_markdown_keeps_readable_text_without_parser_cost() {
    let text = lightweight_markdown_text("### **Scene**\n- `Create` a cube\n\nReady");

    assert_eq!(text, "Scene\n\u{2022} Create a cube\n\nReady");
}

#[test]
fn long_message_text_is_split_into_bounded_atlas_tiles() {
    let source = "Long Agent response ".repeat(180);
    let chunks = message_text_chunks(&source);

    assert!(chunks.len() > 1);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.chars().count() <= MESSAGE_TEXT_CHUNK_CHARS));
    assert_eq!(chunks.concat(), source);
}

#[test]
fn maximum_agent_message_fits_the_bounded_text_atlas_as_tiles() {
    let source = "Agent output ".repeat(1_500);
    let mut message = message(&source);
    message.role = MessageRole::Tool;
    let surface = UiSurface::new(
        "agent-long-message-atlas",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::Column))
            .with_child(message_card_with_variant(
                StudioUiPalette::IndustrialDark,
                &message,
                700.0,
                0,
                MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                AgentSurfaceVariant::Embedded,
                true,
            ))
            .with_child(
                UiNode::new("status.fps", UiNodeKind::Label)
                    .with_text_value("FPS: 60")
                    .with_layout(UiLayout::fixed(76.0, 18.0)),
            ),
    );
    let mut session = UiSurfaceSession::default();
    let frame = session.build_frame_with_resolved_text_at_scale(
        &surface,
        760,
        7_000,
        [0, 0, 0, 255],
        1.5,
        |key| key.to_string(),
    );
    let body_requests = frame
        .text_requests
        .iter()
        .filter(|request| request.node_id.starts_with("agent.message.0.body.text"))
        .collect::<Vec<_>>();
    let fps_request = frame
        .text_requests
        .iter()
        .find(|request| request.node_id == "status.fps")
        .expect("status text remains in the frame after a long message");

    assert!((2..=8).contains(&body_requests.len()));
    assert!(body_requests.iter().all(|request| session
        .text_atlas
        .slot_for(request, &request.text_key)
        .is_some()));
    assert!(session
        .text_atlas
        .slot_for(fps_request, &fps_request.text_key)
        .is_some());
}

#[test]
fn deep_scroll_keeps_only_a_bounded_window_of_one_large_message() {
    let source = "Tool output row with several values and paths\n".repeat(500);
    let mut message = message(&source);
    message.role = MessageRole::Tool;
    let surface = UiSurface::new(
        "agent-deep-message-window",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::Column))
            .with_child(message_card_with_variant(
                StudioUiPalette::IndustrialDark,
                &message,
                700.0,
                0,
                MessageViewport {
                    top: 2_500.0,
                    bottom: 2_960.0,
                },
                AgentSurfaceVariant::Embedded,
                true,
            )),
    );
    let mut session = UiSurfaceSession::default();
    let frame = session.build_frame_with_resolved_text_at_scale(
        &surface,
        760,
        7_000,
        [0, 0, 0, 255],
        1.5,
        |key| key.to_string(),
    );
    let body_requests = frame
        .text_requests
        .iter()
        .filter(|request| request.node_id.starts_with("agent.message.0.body.text"))
        .collect::<Vec<_>>();
    let body = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.body")
        .expect("virtualized message body");

    assert!(!body_requests.is_empty());
    assert!(body_requests.len() <= 8);
    assert!(body_requests
        .iter()
        .all(|request| request.node_id != "agent.message.0.body.text"));
    assert!(body.rect.height > 3_000.0);
}

#[test]
fn session_rows_keep_distinct_vertical_tracks() {
    let mut panel = AgentPanel::default();
    panel.history.start_session("First chat");
    panel.history.start_session("Second chat");
    let surface = UiSurface::new(
        "agent-sessions-layout",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_child(build_sidebar(
                StudioUiPalette::IndustrialDark,
                &panel,
                280.0,
                180.0,
                AgentSurfaceVariant::Embedded,
            )),
    );
    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 280, 180, [0, 0, 0, 255], |key| {
            key.to_string()
        });
    let first = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.session.row.0")
        .expect("first session row");
    let second = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.session.row.1")
        .expect("second session row");

    assert_eq!(first.rect.height, 36.0);
    assert_eq!(second.rect.height, 36.0);
    assert!(second.rect.y >= first.rect.bottom());
}

#[test]
fn message_rows_reserve_height_instead_of_overlapping() {
    let first = message("first");
    let second = message("second");
    let surface = UiSurface::new(
        "agent-history-layout",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::Column))
            .with_child(message_card_with_variant(
                StudioUiPalette::IndustrialDark,
                &first,
                360.0,
                0,
                MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                AgentSurfaceVariant::Embedded,
                false,
            ))
            .with_child(message_card_with_variant(
                StudioUiPalette::IndustrialDark,
                &second,
                360.0,
                1,
                MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                AgentSurfaceVariant::Embedded,
                false,
            )),
    );
    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 400, 240, [0, 0, 0, 255], |key| {
            key.to_string()
        });
    let first_row = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.row")
        .expect("first message row");
    let second_row = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.1.row")
        .expect("second message row");

    assert!(second_row.rect.y >= first_row.rect.bottom());
}

#[test]
fn wrapped_message_rows_advance_by_their_measured_height() {
    let first = message(
        "A long assistant response that must wrap across several lines inside the message card. \
         It must keep the following message below it instead of painting over it.",
    );
    let second = message("second");
    let surface = UiSurface::new(
        "agent-wrapped-history-layout",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::Column))
            .with_child(message_card_with_variant(
                StudioUiPalette::IndustrialDark,
                &first,
                260.0,
                0,
                MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                AgentSurfaceVariant::Embedded,
                false,
            ))
            .with_child(message_card_with_variant(
                StudioUiPalette::IndustrialDark,
                &second,
                260.0,
                1,
                MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                AgentSurfaceVariant::Embedded,
                false,
            )),
    );
    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 300, 240, [0, 0, 0, 255], |key| {
            match key {
                "app.agent_label" => "Agent".to_string(),
                _ => key.to_string(),
            }
        });
    let first_row = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.row")
        .expect("first wrapped message row");
    let second_row = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.1.row")
        .expect("second message row");
    let first_body = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.body.text")
        .expect("first body text");
    assert!(first_body.rect.height > 18.0);
    assert!(second_row.rect.y >= first_row.rect.bottom());
}

#[test]
fn dropdown_menus_have_vertical_flow_and_visible_bounds() {
    let panel = AgentPanel::default();
    let settings = EngineSettings::default();
    let model_menu = build_model_menu(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        720.0,
        AgentSurfaceVariant::Embedded,
    );
    let mode_menu = build_mode_menu(
        StudioUiPalette::IndustrialDark,
        &settings,
        720.0,
        AgentSurfaceVariant::Embedded,
    );

    assert_eq!(model_menu.layout.flow, UiFlow::Column);
    assert_eq!(mode_menu.layout.flow, UiFlow::Column);
    assert!(model_menu.layout.rect.is_some_and(|rect| rect.height > 0.0));
    assert!(mode_menu.layout.rect.is_some_and(|rect| rect.height > 0.0));
}

#[test]
fn dock_mode_select_survives_the_real_input_protocol() {
    let size = [760u32, 520];
    let mut panel = AgentPanel::default();
    let mut settings = EngineSettings::default();
    settings.agent_mode = AgentMode::Active;
    let resolve = |key: &str| key.to_string();
    let idle = UiInputState::default();
    let mut host =
        CpuUiSurfaceHost::new(dock_wrapper_surface(&panel, &settings), [10, 10, 20, 255]);
    let _ = host.process_input(size, resolve, &idle);
    let trigger = host
        .layout_rect("agent.mode.trigger")
        .expect("mode trigger rect");
    let trigger_point = [
        trigger.x + trigger.width * 0.5,
        trigger.y + trigger.height * 0.5,
    ];
    let open_click = UiInputState {
        pointer_position: Some(trigger_point),
        pointer_down: true,
        pointer_buttons_down: vec![raf_ui::UiPointerButton::Primary],
        pointer_pressed_buttons: vec![raf_ui::UiPointerButton::Primary],
        pointer_released_buttons: vec![raf_ui::UiPointerButton::Primary],
        ..UiInputState::default()
    };
    let opened = host.process_input(size, resolve, &open_click);
    assert!(opened.iter().any(|dispatched| matches!(
        &dispatched.action,
        raf_ui::UiAction::SetSelectOpen {
            id,
            open: true
        } if id == "agent.mode.trigger"
    )));
    panel.mode_menu_open = true;
    host.set_surface(dock_wrapper_surface(&panel, &settings));
    let _ = host.process_input(size, resolve, &idle);
    let option = host
        .layout_rect("agent.mode.option.plan")
        .expect("plan option rect");
    let option_point = [
        option.x + option.width * 0.5,
        option.y + option.height * 0.5,
    ];
    let press = UiInputState {
        pointer_position: Some(option_point),
        pointer_down: true,
        pointer_buttons_down: vec![raf_ui::UiPointerButton::Primary],
        pointer_pressed_buttons: vec![raf_ui::UiPointerButton::Primary],
        ..UiInputState::default()
    };
    let pressed = host.process_input(size, resolve, &press);
    assert!(pressed.iter().all(|dispatched| !matches!(
        &dispatched.action,
        raf_ui::UiAction::SetSelectOpen { open: false, .. }
    )));
    let release = UiInputState {
        pointer_position: Some(option_point),
        pointer_released_buttons: vec![raf_ui::UiPointerButton::Primary],
        ..UiInputState::default()
    };
    let released = host.process_input(size, resolve, &release);
    assert!(released.iter().any(|dispatched| matches!(
        &dispatched.action,
        raf_ui::UiAction::SetSelect {
            key,
            value,
            ..
        } if key == "agent.mode" && value == "plan"
    )));
    assert!(released.iter().any(|dispatched| matches!(
        &dispatched.action,
        raf_ui::UiAction::SetSelectOpen {
            id,
            open: false
        } if id == "agent.mode.trigger"
    )));
}

#[test]
fn selector_click_dispatches_the_shared_select_contract() {
    let panel = AgentPanel::default();
    let settings = EngineSettings::default();
    let surface = build_agent_surface(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        AgentReadiness::ProviderDisabled,
        ProjectType::Game,
        [720.0, 400.0],
        0.0,
        1.0,
    );
    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 720, 400, [0, 0, 0, 255], |key| {
            key.to_string()
        });
    let trigger = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.model.trigger")
        .expect("model selector");
    let point = [trigger.rect.x + 12.0, trigger.rect.y + 12.0];
    let _ = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some(point),
            pointer_down: true,
            time_seconds: 0.1,
            ..UiInputState::default()
        },
    );
    let actions = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some(point),
            time_seconds: 0.2,
            ..UiInputState::default()
        },
    );

    assert!(actions.iter().any(|action| {
        matches!(
            action.action,
            raf_ui::UiAction::SetSelectOpen { ref id, open: true }
                if id == "agent.model.trigger"
        )
    }));
}

#[test]
fn model_menu_custom_id_action_is_reachable_and_compact_selectors_keep_their_width() {
    let mut panel = AgentPanel::default();
    panel.model_menu_open = true;
    let settings = EngineSettings::default();
    let surface = build_agent_surface(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        AgentReadiness::ProviderDisabled,
        ProjectType::Game,
        [720.0, 400.0],
        0.0,
        1.0,
    );
    let mut session = UiSurfaceSession::default();
    let frame =
        session.build_frame_with_resolved_text(&surface, 720, 400, [0, 0, 0, 255], |key| {
            key.to_string()
        });
    // The single-field custom model entry lives inline in the picker: the
    // old separate add-model dialog is gone.
    let confirm = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.model.confirm")
        .expect("custom model confirmation");
    assert!(confirm.interactive);

    let point = [confirm.rect.x + 12.0, confirm.rect.y + 12.0];
    let _ = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some(point),
            pointer_down: true,
            time_seconds: 0.1,
            ..UiInputState::default()
        },
    );
    let actions = session.process_input(
        &surface,
        &frame,
        &UiInputState {
            pointer_position: Some(point),
            time_seconds: 0.2,
            ..UiInputState::default()
        },
    );
    assert!(actions.iter().any(|action| matches!(
        action.action,
        raf_ui::UiAction::Command { ref name } if name == "agent.model.confirm"
    )));

    let narrow_header = build_header(
        StudioUiPalette::IndustrialDark,
        &panel,
        &settings,
        "OpenRouter".to_string(),
        "Default".to_string(),
        "app.agent_mode_plan",
        340.0,
        AgentSurfaceVariant::Embedded,
    );
    let narrow_surface = UiSurface::new(
        "agent-narrow-header",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root).with_child(narrow_header),
    );
    let narrow_frame = narrow_surface.build_frame(340, 400, [0, 0, 0, 255]);
    let model = narrow_frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.model.trigger")
        .expect("narrow model selector");
    let mode = narrow_frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.mode.trigger")
        .expect("narrow mode selector");
    assert!(model.rect.width >= 100.0);
    assert!(mode.rect.width >= 80.0);
    assert!(mode.rect.x >= model.rect.right());
}

fn dock_wrapper_surface(
    panel: &AgentPanel,
    settings: &EngineSettings,
) -> raf_render::api_graphic_basic::ui_surface::UiSurface {
    build_dock_agent_surface(
        StudioUiPalette::IndustrialDark,
        panel,
        settings,
        AgentReadiness::ProviderDisabled,
        ProjectType::Game,
        [760.0, 520.0],
        0.0,
        1.0,
    )
}

fn tool_message(content: &str) -> ChatMessage {
    let mut message = message(content);
    message.role = MessageRole::Tool;
    message
}

fn dock_frame(
    panel: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
    height: f32,
) -> raf_render::api_graphic_basic::ui_surface::UiSurfaceFrame {
    let surface = build_dock_agent_surface(
        StudioUiPalette::IndustrialDark,
        panel,
        settings,
        AgentReadiness::ProviderDisabled,
        ProjectType::Game,
        [width, height],
        0.0,
        1.0,
    );
    let mut session = UiSurfaceSession::default();
    session.build_frame_with_resolved_text(&surface, 800, 800, [0, 0, 0, 255], |key| {
        key.to_string()
    })
}

#[test]
fn dock_collapses_tool_results_into_rows_until_expanded() {
    let mut panel = AgentPanel::default();
    panel
        .runtime
        .messages
        .push(tool_message("Tool result ... Revision 7 ..."));

    let frame = dock_frame(&panel, &EngineSettings::default(), 720.0, 520.0);
    let row = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.row")
        .expect("collapsed tool row in dock");
    assert!(row.rect.height <= 40.0);
    assert!(row.rect.height.min(1.0).gt(&0.0));
    assert!(frame
        .layout_boxes
        .iter()
        .all(|layout| layout.id != "agent.message.0.body"));
}

#[test]
fn dock_expands_tool_results_after_the_panel_marks_them_expanded() {
    let mut panel = AgentPanel::default();
    panel
        .runtime
        .messages
        .push(tool_message("Tool result ... Revision 7 ..."));
    let expanded_id = panel.runtime.messages[0].id.to_string();
    panel.expanded_tools.insert(expanded_id);

    let frame = dock_frame(&panel, &EngineSettings::default(), 720.0, 520.0);
    let body = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.body")
        .expect("expanded tool body in dock");
    let row = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.row")
        .expect("tool toggle row stays visible while expanded");
    assert!(row.rect.height <= 40.0);
    assert!(body.rect.y >= row.rect.bottom() - 1.0);
}

#[test]
fn dock_keeps_assistant_messages_fully_visible_inside_the_tool_viewport() {
    let mut panel = AgentPanel::default();
    panel
        .runtime
        .messages
        .push(message("assistant answer body"));

    let frame = dock_frame(&panel, &EngineSettings::default(), 720.0, 520.0);
    let body = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.body.text")
        .expect("assistant body text in dock");
    let card = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == "agent.message.0.card")
        .expect("assistant card in dock");
    assert!(body.rect.bottom() <= card.rect.bottom() + 1.0);
}
