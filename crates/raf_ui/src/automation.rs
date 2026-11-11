//! Bounded semantic input. Platform providers never mutate retained documents.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crate::{UiAction, UiDispatchedAction, UiEventKind, UiInteractionState, UiNode};

const MAX_PENDING: usize = 32;
const ACTION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub enum UiAutomationAction { Invoke, Focus, SetText(String), SetRange(f64), Expand(bool) }
pub struct UiAutomationRequest {
    pub target: String,
    pub action: UiAutomationAction,
    expires: Instant,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<Result<(), ()>>,
}
#[derive(Clone, Default)]
pub struct UiAutomationInbox(Arc<Mutex<VecDeque<UiAutomationRequest>>>);
impl UiAutomationInbox {
    pub fn same_queue(&self, other: &Self) -> bool { Arc::ptr_eq(&self.0, &other.0) }
    pub fn submit(&self, target: String, action: UiAutomationAction) -> Result<UiAutomationTicket, ()> {
        let (tx, rx) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut queue = self.0.lock().map_err(|_| ())?;
        if queue.len() >= MAX_PENDING { return Err(()); }
        queue.push_back(UiAutomationRequest { target, action, expires: Instant::now() + ACTION_TIMEOUT,
            cancelled: cancelled.clone(), reply: tx });
        Ok(UiAutomationTicket { rx, cancelled, cancel_on_drop: true })
    }
    pub fn drain(&self, root: &UiNode, interaction: &mut UiInteractionState) -> Vec<UiDispatchedAction> {
        let pending = self.0.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default();
        let mut actions = Vec::new();
        for request in pending {
            if request.cancelled.load(Ordering::Acquire) || request.expires < Instant::now() { continue; }
            let result = apply(root, interaction, &request.target, request.action, &mut actions);
            let _ = request.reply.send(result);
        }
        actions
    }
    pub fn cancel(&self) {
        if let Ok(mut queue) = self.0.lock() { for request in queue.drain(..) { let _ = request.reply.send(Err(())); } }
    }
}
pub struct UiAutomationTicket { rx: mpsc::Receiver<Result<(), ()>>, cancelled: Arc<AtomicBool>, cancel_on_drop: bool }
impl UiAutomationTicket {
    pub fn wait(self) -> Result<(), ()> { self.rx.recv_timeout(ACTION_TIMEOUT).map_err(|_| ())? }
    /// Window-thread callbacks enqueue work without pumping or waiting on the same thread.
    pub fn detach(mut self) { self.cancel_on_drop = false; }
}
impl Drop for UiAutomationTicket {
    fn drop(&mut self) { if self.cancel_on_drop { self.cancelled.store(true, Ordering::Release); } }
}

fn live<'a>(root: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if root.hidden || root.disabled { return None; }
    if root.id == id { return Some(root); }
    root.children.iter().find_map(|child| live(child, id))
}
fn apply(root: &UiNode, interaction: &mut UiInteractionState, id: &str, action: UiAutomationAction,
    out: &mut Vec<UiDispatchedAction>) -> Result<(), ()> {
    let node = live(root, id).ok_or(())?;
    let mut push = |action, event| out.push(UiDispatchedAction { target_id: id.into(), event, action });
    match action {
        UiAutomationAction::Focus if node.focusable || node.control.text_input().is_some() || node.control.range().is_some() || node.control.toggle().is_some() || node.control.select().is_some() => { interaction.focus.request_focus(id); }
        UiAutomationAction::SetText(value) => {
            let input = node.control.text_input().ok_or(())?;
            if input.password || input.value_key.contains("api_key") || value.chars().count() > input.max_length { return Err(()); }
            interaction.focus.request_focus(id);
            interaction.controls.set_text(&input.value_key, &value, input.max_length);
            push(UiAction::SetText { key: input.value_key.clone(), value: value.clone() }, UiEventKind::TextInput(value));
            // A semantic replacement supplies a complete value, including fields
            // whose physical keyboard path commits only on Enter.
            if let Some(command) = &input.submit_command {
                push(UiAction::Command { name: command.clone() }, UiEventKind::KeyPress("Enter".into()));
            }
        }
        UiAutomationAction::SetRange(value) => {
            let range = node.control.range().ok_or(())?;
            if !value.is_finite() || value < range.min as f64 || value > range.max as f64 { return Err(()); }
            push(UiAction::SetRange { key: range.value_key.clone(), value: value as f32 }, UiEventKind::Click);
        }
        UiAutomationAction::Expand(open) => {
            let select = node.control.select().ok_or(())?;
            if select.open != open { push(UiAction::SetSelectOpen { id: id.into(), open }, UiEventKind::Click); }
        }
        UiAutomationAction::Invoke => {
            if let Some(toggle) = node.control.toggle() {
                push(UiAction::SetToggle { key: toggle.value_key.clone(), value: !toggle.value }, UiEventKind::Click);
            } else if let Some(select) = node.control.select() {
                push(UiAction::SetSelectOpen { id: id.into(), open: !select.open }, UiEventKind::Click);
            } else {
                let mut found = false;
                for binding in &node.event_handlers {
                    if binding.event == UiEventKind::Click { found = true; push(binding.action.clone(), UiEventKind::Click); }
                }
                if !found { return Err(()); }
            }
            if node.focusable { interaction.focus.request_focus(id); }
        }
        _ => return Err(()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiNodeKind, UiTextInput, UiEventBinding};
    #[test]
    fn native_automation_commits_complete_values_and_focuses_implicit_inputs() {
        let mut input = UiTextInput::new("numeric");
        input.submit_command = Some("numeric.commit".into());
        let root = UiNode::text_input("input", input);
        let mut state = UiInteractionState::default();
        let mut actions = Vec::new();
        apply(&root, &mut state, "input", UiAutomationAction::Focus, &mut actions).unwrap();
        assert!(state.focus.has_focus("input"));
        apply(&root, &mut state, "input", UiAutomationAction::SetText("8.5".into()), &mut actions).unwrap();
        assert_eq!(state.controls.text("numeric"), "8.5");
        assert!(matches!(&actions[1].action, UiAction::Command { name } if name == "numeric.commit"));
    }
    #[test]
    fn native_automation_cancellation_and_hidden_ancestors_block_actions() {
        let button = UiNode::new("button", UiNodeKind::Button).with_event(UiEventBinding::command(UiEventKind::Click, "test.action"));
        let mut root = UiNode::new("root", UiNodeKind::Root).with_child(button);
        let inbox = UiAutomationInbox::default();
        let ticket = inbox.submit("button".into(), UiAutomationAction::Invoke).unwrap();
        drop(ticket);
        assert!(inbox.drain(&root, &mut UiInteractionState::default()).is_empty());
        root.hidden = true;
        let ticket = inbox.submit("button".into(), UiAutomationAction::Invoke).unwrap();
        assert!(inbox.drain(&root, &mut UiInteractionState::default()).is_empty());
        assert!(ticket.wait().is_err());
    }
    #[test]
    fn native_automation_uses_existing_actions_and_protects_passwords() {
        let mut field = UiTextInput::new("text");
        let mut root = UiNode::text_input("input", field.clone()).focusable();
        let mut state = UiInteractionState::default();
        let mut actions = Vec::new();
        apply(&root, &mut state, "input", UiAutomationAction::SetText("hello".into()), &mut actions).unwrap();
        assert_eq!(state.controls.text("text"), "hello");
        assert!(matches!(&actions[0].action, UiAction::SetText { value, .. } if value == "hello"));
        field.password = true;
        root = UiNode::text_input("input", field);
        assert!(apply(&root, &mut state, "input", UiAutomationAction::SetText("secret".into()), &mut actions).is_err());
    }
}
