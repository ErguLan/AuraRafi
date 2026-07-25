use serde::{Deserialize, Serialize};

use crate::UiPointerButton;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UiFocusState {
    pub hovered: Option<String>,
    pub focused: Option<String>,
    pub active: Option<String>,
}

impl UiFocusState {
    pub fn request_focus(&mut self, id: impl Into<String>) {
        self.focused = Some(id.into());
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    pub fn set_hovered(&mut self, id: Option<String>) {
        self.hovered = id;
    }

    pub fn set_active(&mut self, id: Option<String>) {
        self.active = id;
    }

    pub fn has_focus(&self, id: &str) -> bool {
        self.focused.as_deref() == Some(id)
    }

    pub fn blur_if_removed<'a>(&mut self, live_ids: impl IntoIterator<Item = &'a str>) {
        let Some(focused) = self.focused.as_deref() else {
            return;
        };
        if !live_ids.into_iter().any(|id| id == focused) {
            self.clear_focus();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UiInputState {
    pub pointer_position: Option<[f32; 2]>,
    pub pointer_delta: [f32; 2],
    /// Monotonic host time in seconds. It is optional in spirit and defaults
    /// to zero for deterministic/headless callers.
    #[serde(default)]
    pub time_seconds: f64,
    /// Positive Y means the content should move down, matching the retained
    /// surface coordinate system rather than a particular platform event API.
    pub scroll_delta: [f32; 2],
    /// Legacy primary-button state retained for simple embedders.
    pub pointer_down: bool,
    pub pointer_buttons_down: Vec<UiPointerButton>,
    pub pointer_pressed_buttons: Vec<UiPointerButton>,
    pub pointer_released_buttons: Vec<UiPointerButton>,
    pub pressed_keys: Vec<String>,
    pub text_input: String,
}

impl UiInputState {
    pub fn key_pressed(&self, key: &str) -> bool {
        let lower = key.to_ascii_lowercase();
        self.pressed_keys
            .iter()
            .any(|pressed| pressed.eq_ignore_ascii_case(&lower))
    }

    pub fn button_down(&self, button: UiPointerButton) -> bool {
        (button == UiPointerButton::Primary && self.pointer_down)
            || self.pointer_buttons_down.contains(&button)
    }

    pub fn button_pressed(&self, button: UiPointerButton) -> bool {
        self.pointer_pressed_buttons.contains(&button)
    }

    pub fn button_released(&self, button: UiPointerButton) -> bool {
        self.pointer_released_buttons.contains(&button)
    }

    pub fn delta_seconds_since(&self, previous: f64) -> f32 {
        (self.time_seconds - previous).max(0.0).min(0.25) as f32
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiFocusPolicy {
    pub tab_order: Vec<String>,
    pub wrap: bool,
}

impl Default for UiFocusPolicy {
    fn default() -> Self {
        Self {
            tab_order: Vec::new(),
            wrap: true,
        }
    }
}

impl UiFocusPolicy {
    pub fn next_after(&self, current: Option<&str>) -> Option<&str> {
        if self.tab_order.is_empty() {
            return None;
        }

        let next_index = current
            .and_then(|id| self.tab_order.iter().position(|candidate| candidate == id))
            .map(|index| index + 1)
            .unwrap_or(0);

        if next_index < self.tab_order.len() {
            Some(self.tab_order[next_index].as_str())
        } else if self.wrap {
            Some(self.tab_order[0].as_str())
        } else {
            None
        }
    }

    pub fn from_focusable_ids(ids: impl IntoIterator<Item = String>) -> Self {
        Self {
            tab_order: ids.into_iter().collect(),
            wrap: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_policy_wraps_tab_order() {
        let policy = UiFocusPolicy {
            tab_order: vec!["a".to_string(), "b".to_string()],
            wrap: true,
        };

        assert_eq!(policy.next_after(None), Some("a"));
        assert_eq!(policy.next_after(Some("a")), Some("b"));
        assert_eq!(policy.next_after(Some("b")), Some("a"));
    }

    #[test]
    fn blur_if_removed_clears_missing_focus() {
        let mut state = UiFocusState::default();
        state.request_focus("gone");

        state.blur_if_removed(["live"].into_iter());

        assert_eq!(state.focused, None);
    }
}
