use super::pointer::word_range_at;
use super::UiInteractionState;
use crate::events::{UiAction, UiEventBinding, UiEventKind, UiPointerButton};
use crate::focus::{UiFocusPolicy, UiInputState, UiModifiers};
use crate::geometry::UiRect;
use crate::hit_test::UiHitRegion;
use crate::layout::UiLayout;
use crate::node::{UiNode, UiNodeKind};

fn button(id: &str, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn region(id: &str, x: f32) -> UiHitRegion {
    UiHitRegion {
        id: id.to_string(),
        kind: UiNodeKind::Button,
        rect: UiRect::new(x, 0.0, 80.0, 30.0),
        clip_rect: UiRect::new(x, 0.0, 80.0, 30.0),
        z_index: 1,
        interactive: true,
        focusable: true,
        disabled: false,
    }
}

fn text_region(id: &str, width: f32) -> UiHitRegion {
    UiHitRegion {
        id: id.to_string(),
        kind: UiNodeKind::TextInput,
        rect: UiRect::new(0.0, 0.0, width, 30.0),
        clip_rect: UiRect::new(0.0, 0.0, width, 30.0),
        z_index: 1,
        interactive: true,
        focusable: true,
        disabled: false,
    }
}

#[test]
fn click_dispatches_only_after_pointer_release_on_same_target() {
    let root = UiNode::new("root", UiNodeKind::Root)
        .with_layout(UiLayout::default())
        .with_child(button("save", "file.save"));
    let regions = vec![region("save", 0.0)];
    let policy = UiFocusPolicy {
        tab_order: vec!["save".to_string()],
        wrap: true,
    };
    let mut state = UiInteractionState::default();

    let press = UiInputState {
        pointer_position: Some([12.0, 12.0]),
        pointer_down: true,
        ..UiInputState::default()
    };
    assert!(state.update(&root, &regions, &press, &policy).is_empty());
    assert!(state.focus.has_focus("save"));

    let release = UiInputState {
        pointer_position: Some([12.0, 12.0]),
        ..UiInputState::default()
    };
    let actions = state.update(&root, &regions, &release, &policy);

    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        UiAction::Command {
            name: "file.save".to_string()
        }
    );
}

#[test]
fn coalesced_press_and_release_still_dispatches_a_click() {
    let root = UiNode::new("root", UiNodeKind::Root)
        .with_layout(UiLayout::default())
        .with_child(button("save", "file.save"));
    let regions = vec![region("save", 0.0)];
    let mut state = UiInteractionState::default();

    let actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            pointer_pressed_buttons: vec![UiPointerButton::Primary],
            pointer_released_buttons: vec![UiPointerButton::Primary],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert!(actions.iter().any(|action| {
        action.action
            == UiAction::Command {
                name: "file.save".to_string(),
            }
    }));
    assert!(!state.has_pointer_capture());
}

#[test]
fn repeated_pointer_release_dispatches_generic_double_click() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(
        button("node", "node.select").with_event(UiEventBinding {
            event: UiEventKind::DoubleClick,
            action: UiAction::Command {
                name: "node.rename".to_string(),
            },
        }),
    );
    let regions = vec![region("node", 0.0)];
    let policy = UiFocusPolicy::default();
    let mut state = UiInteractionState::default();

    for time in [1.0_f64, 1.2_f64] {
        state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                pointer_down: true,
                time_seconds: time,
                ..UiInputState::default()
            },
            &policy,
        );
        let actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                time_seconds: time + 0.01,
                ..UiInputState::default()
            },
            &policy,
        );
        if time > 1.0 {
            assert!(actions.iter().any(|action| {
                action.action
                    == UiAction::Command {
                        name: "node.rename".to_string(),
                    }
            }));
        }
    }
}

#[test]
fn tab_uses_declared_focus_order() {
    let root = UiNode::new("root", UiNodeKind::Root)
        .with_child(button("first", "first"))
        .with_child(button("second", "second"));
    let policy = UiFocusPolicy {
        tab_order: vec!["first".to_string(), "second".to_string()],
        wrap: true,
    };
    let mut state = UiInteractionState::default();

    state.update(
        &root,
        &[],
        &UiInputState {
            pressed_keys: vec!["Tab".to_string()],
            ..UiInputState::default()
        },
        &policy,
    );
    assert!(state.focus.has_focus("first"));

    state.update(
        &root,
        &[],
        &UiInputState {
            pressed_keys: vec!["Tab".to_string()],
            ..UiInputState::default()
        },
        &policy,
    );
    assert!(state.focus.has_focus("second"));
}

#[test]
fn secondary_click_dispatches_context_menu_and_text_targets_focus() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(
        button("input", "input.click")
            .with_event(UiEventBinding::command(
                UiEventKind::ContextMenu,
                "input.menu",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::TextInput("hello".to_string()),
                "input.text",
            )),
    );
    let regions = vec![region("input", 0.0)];
    let mut state = UiInteractionState::default();

    let menu_actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            pointer_pressed_buttons: vec![UiPointerButton::Secondary],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(menu_actions.iter().any(|action| matches!(
        action.action,
        UiAction::Command { ref name } if name == "input.menu"
    )));

    let text_actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            text_input: "hello".to_string(),
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(text_actions.iter().any(|action| matches!(
        action.action,
        UiAction::Command { ref name } if name == "input.text"
    )));
}

#[test]
fn text_input_updates_transient_value_and_emits_set_text() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("hub.query"),
    ));
    let mut state = UiInteractionState::default();
    state.focus.request_focus("query");

    let actions = state.update(
        &root,
        &[region("query", 0.0)],
        &UiInputState {
            text_input: "raf".to_string(),
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(state.controls.text("hub.query"), "raf");
    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::SetText { ref key, ref value } if key == "hub.query" && value == "raf"
    )));
}

#[test]
fn ctrl_a_selects_all_text_in_the_focused_input() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let mut state = UiInteractionState::default();
    state.controls.set_text("query.value", "select me", 64);
    state.focus.request_focus("query");

    state.update(
        &root,
        &[text_region("query", 160.0)],
        &UiInputState {
            pressed_keys: vec!["A".to_string()],
            modifiers: crate::UiModifiers {
                control: true,
                ..crate::UiModifiers::default()
            },
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(state.controls.text_edit("query.value").selection(), 0..9);
}

#[test]
fn dragging_text_input_selects_between_pointer_positions() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let regions = [text_region("query", 120.0)];
    let mut state = UiInteractionState::default();
    state.controls.set_text("query.value", "hello world", 64);

    state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([0.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([110.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert!(state.controls.text_edit("query.value").has_selection());
    assert_eq!(state.controls.text_edit("query.value").selection(), 0..10);
}

#[test]
fn dragging_selectable_text_selects_it_and_ctrl_c_emits_clipboard_action() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(
        UiNode::new("message", UiNodeKind::Label)
            .with_text_value("hello agent")
            .selectable_text(),
    );
    let regions = [UiHitRegion {
        id: "message".to_string(),
        kind: UiNodeKind::Label,
        rect: UiRect::new(0.0, 0.0, 120.0, 30.0),
        clip_rect: UiRect::new(0.0, 0.0, 120.0, 30.0),
        z_index: 1,
        interactive: true,
        focusable: true,
        disabled: false,
    }];
    let mut state = UiInteractionState::default();

    state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([0.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([55.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(
        state
            .controls
            .selected_selectable_text("message", "hello agent"),
        "hello"
    );

    let actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pressed_keys: vec!["C".to_string()],
            modifiers: UiModifiers {
                control: true,
                ..UiModifiers::default()
            },
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(actions.iter().any(|action| matches!(
        &action.action,
        UiAction::SetClipboard { text } if text == "hello"
    )));
}

#[test]
fn double_click_selects_the_word_under_the_pointer() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let regions = [text_region("query", 120.0)];
    let mut state = UiInteractionState::default();
    state.controls.set_text("query.value", "hello world", 64);

    for (time_seconds, down) in [(0.0, true), (0.1, false), (0.25, true)] {
        state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                pointer_down: down,
                time_seconds,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
    }

    assert_eq!(state.controls.text_edit("query.value").selection(), 0..5);
}

#[test]
fn held_delete_repeats_until_release() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let regions = [text_region("query", 120.0)];
    let mut state = UiInteractionState::default();
    state.controls.set_text("query.value", "abcd", 64);
    state.controls.set_cursor("query.value", 0, false);
    state.focus.request_focus("query");

    state.update(
        &root,
        &regions,
        &UiInputState {
            pressed_keys: vec!["delete".to_string()],
            keys_down: vec!["delete".to_string()],
            time_seconds: 0.0,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert_eq!(state.controls.text("query.value"), "bcd");

    state.update(
        &root,
        &regions,
        &UiInputState {
            keys_down: vec!["delete".to_string()],
            time_seconds: 0.46,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert_eq!(state.controls.text("query.value"), "cd");

    state.update(
        &root,
        &regions,
        &UiInputState {
            time_seconds: 0.5,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(!state.has_active_text_repeat());
}

#[test]
fn delete_consumes_coalesced_press_edges_and_keeps_repeating() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let regions = [text_region("query", 120.0)];
    let mut state = UiInteractionState::default();
    state.controls.set_text("query.value", "abcdef", 64);
    state.controls.set_cursor("query.value", 0, false);
    state.focus.request_focus("query");

    state.update(
        &root,
        &regions,
        &UiInputState {
            // Two press/release cycles can arrive before one redraw. A
            // boolean key query must not collapse them into one edit.
            pressed_keys: vec!["delete".to_string(), "delete".to_string()],
            keys_down: vec!["delete".to_string()],
            time_seconds: 0.0,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert_eq!(state.controls.text("query.value"), "cdef");

    state.update(
        &root,
        &regions,
        &UiInputState {
            keys_down: vec!["delete".to_string()],
            time_seconds: 0.46,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert_eq!(state.controls.text("query.value"), "def");

    state.update(
        &root,
        &regions,
        &UiInputState {
            keys_down: vec!["delete".to_string()],
            time_seconds: 0.50,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert_eq!(state.controls.text("query.value"), "ef");

    state.update(
        &root,
        &regions,
        &UiInputState {
            time_seconds: 0.51,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(!state.has_active_text_repeat());
}

#[test]
fn backspace_consumes_coalesced_press_edges() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let regions = [text_region("query", 120.0)];
    let mut state = UiInteractionState::default();
    state.controls.set_text("query.value", "abcdef", 64);
    state.controls.set_cursor("query.value", 6, false);
    state.focus.request_focus("query");

    state.update(
        &root,
        &regions,
        &UiInputState {
            pressed_keys: vec!["backspace".to_string(), "backspace".to_string()],
            keys_down: vec!["backspace".to_string()],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(state.controls.text("query.value"), "abcd");
}

#[test]
fn double_click_keeps_punctuation_separate_from_words_and_spaces() {
    assert_eq!(word_range_at("hello,  world", 1), 0..5);
    assert_eq!(word_range_at("hello,  world", 5), 5..6);
    assert_eq!(word_range_at("hello,  world", 6), 6..8);
}

#[test]
fn cursor_hint_distinguishes_textboxes_from_actionable_controls() {
    let text_root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let mut text_state = UiInteractionState::default();
    text_state.focus.set_hovered(Some("query".to_string()));
    assert_eq!(
        text_state.cursor_hint(&text_root),
        crate::UiCursorIcon::Text
    );

    let resize_root = UiNode::new("root", UiNodeKind::Root).with_child(
        UiNode::new("resize", UiNodeKind::Button).with_class("settings-modal-resize-handle"),
    );
    let mut resize_state = UiInteractionState::default();
    resize_state.focus.set_hovered(Some("resize".to_string()));
    assert_eq!(
        resize_state.cursor_hint(&resize_root),
        crate::UiCursorIcon::ResizeNorthWestSouthEast
    );

    let button_root = UiNode::new("root", UiNodeKind::Root).with_child(button("save", "save"));
    let mut button_state = UiInteractionState::default();
    button_state.focus.set_hovered(Some("save".to_string()));
    assert_eq!(
        button_state.cursor_hint(&button_root),
        crate::UiCursorIcon::PointingHand
    );

    let panel_root = UiNode::new("root", UiNodeKind::Root)
        .with_child(UiNode::new("settings", UiNodeKind::Panel).focusable());
    let mut panel_state = UiInteractionState::default();
    panel_state.focus.set_hovered(Some("settings".to_string()));
    assert_eq!(
        panel_state.cursor_hint(&panel_root),
        crate::UiCursorIcon::Default
    );
}

#[test]
fn focused_retained_control_captures_editor_keyboard_input() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let mut state = UiInteractionState::default();
    state.focus.request_focus("query");

    assert!(state.captures_keyboard_input(&root));
    state.focus.clear_focus();
    assert!(!state.captures_keyboard_input(&root));
}

#[test]
fn focused_button_captures_keyboard_input_for_activation() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(button("save", "file.save"));
    let mut state = UiInteractionState::default();
    state.focus.request_focus("save");

    assert!(state.captures_keyboard_input(&root));

    let actions = state.update(
        &root,
        &[],
        &UiInputState {
            pressed_keys: vec!["enter".to_string()],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::Command { ref name } if name == "file.save"
    )));
}

#[test]
fn focused_select_emits_navigation_and_toggle_actions() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::select(
        "quality",
        crate::UiSelect::new(
            "settings.quality",
            vec![
                crate::UiSelectOption::new("low", "quality.low"),
                crate::UiSelectOption::new("high", "quality.high"),
            ],
            0,
        ),
    ));
    let mut state = UiInteractionState::default();
    state.focus.request_focus("quality");

    let down = state.update(
        &root,
        &[],
        &UiInputState {
            pressed_keys: vec!["arrowdown".to_string()],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(down.iter().any(|action| matches!(
        action.action,
        UiAction::SetSelect { ref key, ref value, index: 1 }
            if key == "settings.quality" && value == "high"
    )));

    let enter = state.update(
        &root,
        &[],
        &UiInputState {
            pressed_keys: vec!["enter".to_string()],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(enter.iter().any(|action| matches!(
        action.action,
        UiAction::SetSelectOpen { ref id, open: true } if id == "quality"
    )));
}

#[test]
fn focused_select_supports_typeahead_without_requiring_a_second_widget() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::select(
        "quality",
        crate::UiSelect::new(
            "settings.quality",
            vec![
                crate::UiSelectOption::new("low", "quality.low"),
                crate::UiSelectOption::new("high", "quality.high"),
            ],
            0,
        ),
    ));
    let mut state = UiInteractionState::default();
    state.focus.request_focus("quality");
    let actions = state.update(
        &root,
        &[],
        &UiInputState {
            text_input: "h".to_string(),
            time_seconds: 1.0,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::SetSelect { ref key, ref value, index: 1 }
            if key == "settings.quality" && value == "high"
    )));
}

#[test]
fn outside_primary_press_releases_retained_keyboard_focus() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
        "query",
        crate::UiTextInput::new("query.value"),
    ));
    let mut state = UiInteractionState::default();
    state.focus.request_focus("query");

    state.update(
        &root,
        &[],
        &UiInputState {
            pointer_pressed_outside: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert!(!state.captures_keyboard_input(&root));
}

#[test]
fn drag_end_survives_when_the_source_node_is_rebuilt_away() {
    let drag_root = UiNode::new("root", UiNodeKind::Root).with_child(
        button("tab", "tab.click")
            .with_event(UiEventBinding::command(UiEventKind::DragStart, "tab.start"))
            .with_event(UiEventBinding::command(UiEventKind::DragMove, "tab.move"))
            .with_event(UiEventBinding::command(UiEventKind::DragEnd, "tab.end")),
    );
    let empty_root = UiNode::new("root", UiNodeKind::Root);
    let mut state = UiInteractionState::default();

    state.update(
        &drag_root,
        &[region("tab", 0.0)],
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    let moving = state.update(
        &empty_root,
        &[],
        &UiInputState {
            pointer_position: Some([24.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(moving.iter().any(|action| matches!(
        action.action,
        UiAction::Command { ref name } if name == "tab.start"
    )));
    assert!(moving.iter().any(|action| matches!(
        action.action,
        UiAction::Command { ref name } if name == "tab.move"
    )));

    let released = state.update(
        &empty_root,
        &[],
        &UiInputState {
            pointer_position: Some([24.0, 12.0]),
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    assert!(released.iter().any(|action| matches!(
        action.action,
        UiAction::Command { ref name } if name == "tab.end"
    )));
}

#[test]
fn scroll_view_moves_state_from_a_descendant_hit() {
    let root = UiNode::scroll_view("list", crate::UiScrollAxis::Vertical)
        .with_child(UiNode::new("list.item", UiNodeKind::Panel).interactive());
    let mut state = UiInteractionState::default();

    let actions = state.update(
        &root,
        &[region("list.item", 0.0)],
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            scroll_delta: [0.0, 24.0],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(state.controls.scroll_offset("list"), [0.0, 24.0]);
    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::ScrollTo { ref id, offset } if id == "list" && offset == [0.0, 24.0]
    )));
}

#[test]
fn scroll_view_accepts_wheel_over_passive_content() {
    let root = UiNode::scroll_view("list", crate::UiScrollAxis::Vertical)
        .with_child(UiNode::new("list.message", UiNodeKind::Panel));
    let regions = vec![
        UiHitRegion {
            id: "list".to_string(),
            kind: UiNodeKind::ScrollView,
            rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
            clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
            z_index: 0,
            interactive: false,
            focusable: false,
            disabled: false,
        },
        UiHitRegion {
            id: "list.message".to_string(),
            kind: UiNodeKind::Panel,
            rect: UiRect::new(0.0, 0.0, 240.0, 300.0),
            clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
            z_index: -1,
            interactive: false,
            focusable: false,
            disabled: false,
        },
    ];
    let mut state = UiInteractionState::default();
    state.controls.set_scroll_metrics("list", [0.0, 180.0]);

    let actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([120.0, 80.0]),
            scroll_delta: [0.0, 48.0],
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(state.controls.scroll_offset("list"), [0.0, 48.0]);
    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::ScrollTo { ref id, offset } if id == "list" && offset == [0.0, 48.0]
    )));
}

#[test]
fn manual_scrollbar_thumb_drag_updates_the_retained_scroll_offset() {
    let root = UiNode::scroll_view("list", crate::UiScrollAxis::Vertical);
    let regions = vec![
        UiHitRegion {
            id: "__rafui.scrollbar.list.track".to_string(),
            kind: UiNodeKind::Panel,
            rect: UiRect::new(230.0, 0.0, 10.0, 120.0),
            clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
            z_index: 4,
            interactive: true,
            focusable: false,
            disabled: false,
        },
        UiHitRegion {
            id: "__rafui.scrollbar.list.thumb".to_string(),
            kind: UiNodeKind::Panel,
            rect: UiRect::new(230.0, 0.0, 10.0, 30.0),
            clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
            z_index: 5,
            interactive: true,
            focusable: false,
            disabled: false,
        },
    ];
    let mut state = UiInteractionState::default();
    state.controls.set_scroll_metrics("list", [0.0, 180.0]);

    state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([235.0, 10.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    let actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([235.0, 80.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert_eq!(state.controls.scroll_offset("list"), [0.0, 140.0]);
    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::ScrollTo { ref id, offset } if id == "list" && offset == [0.0, 140.0]
    )));
}

#[test]
fn toggle_emits_the_next_boolean_value() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::toggle(
        "enabled",
        crate::UiToggle::new("settings.enabled", false),
    ));
    let mut state = UiInteractionState::default();
    let regions = vec![region("enabled", 0.0)];

    state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );
    let actions = state.update(
        &root,
        &regions,
        &UiInputState {
            pointer_position: Some([12.0, 12.0]),
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::SetToggle { ref key, value } if key == "settings.enabled" && value
    )));
}

#[test]
fn range_maps_pointer_position_to_a_stepped_value() {
    let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::range(
        "scale",
        crate::UiRange::new("settings.scale", 0.0, 0.0, 100.0, 5.0),
    ));
    let mut state = UiInteractionState::default();
    let actions = state.update(
        &root,
        &[region("scale", 0.0)],
        &UiInputState {
            pointer_position: Some([62.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        },
        &UiFocusPolicy::default(),
    );

    assert!(actions.iter().any(|action| matches!(
        action.action,
        UiAction::SetRange { ref key, value } if key == "settings.scale" && value == 80.0
    )));
}
