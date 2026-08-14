use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per-surface values that must not be persisted into a UI document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiControlState {
    #[serde(default)]
    text_values: BTreeMap<String, String>,
    #[serde(default)]
    scroll_offsets: BTreeMap<String, [f32; 2]>,
    #[serde(default)]
    scroll_max_offsets: BTreeMap<String, [f32; 2]>,
    #[serde(default)]
    text_edit: BTreeMap<String, UiTextEditState>,
}

/// Session-owned caret and selection state for a text control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UiTextEditState {
    pub cursor: usize,
    pub anchor: usize,
}

impl UiTextEditState {
    pub fn selection(self) -> std::ops::Range<usize> {
        let start = self.cursor.min(self.anchor);
        let end = self.cursor.max(self.anchor);
        start..end
    }

    pub fn has_selection(self) -> bool {
        self.cursor != self.anchor
    }
}

impl UiControlState {
    pub fn text(&self, key: &str) -> &str {
        self.text_values.get(key).map(String::as_str).unwrap_or("")
    }

    /// Returns whether a host has initialized this field in the current
    /// surface session. This distinction matters for editable settings: an
    /// empty field can be a deliberate user edit and must not be reseeded
    /// from the application model on the next frame.
    pub fn has_text(&self, key: &str) -> bool {
        self.text_values.contains_key(key)
    }

    pub fn set_text(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        max_length: usize,
    ) {
        let value = value.into().chars().take(max_length).collect::<String>();
        let key = key.into();
        let length = value.chars().count();
        let unchanged = self.text_values.get(&key).map(String::as_str) == Some(value.as_str());
        self.text_values.insert(key.clone(), value);
        let edit = self.text_edit.entry(key).or_default();
        if unchanged {
            edit.cursor = edit.cursor.min(length);
            edit.anchor = edit.anchor.min(length);
        } else {
            edit.cursor = length;
            edit.anchor = length;
        }
    }

    pub fn append_text(&mut self, key: &str, value: &str, max_length: usize) -> bool {
        let current = self.text_values.entry(key.to_string()).or_default();
        let edit = self
            .text_edit
            .entry(key.to_string())
            .or_insert_with(|| UiTextEditState {
                cursor: current.chars().count(),
                anchor: current.chars().count(),
            });
        let selection = edit.selection();
        let current_length = current.chars().count();
        let remaining = max_length.saturating_sub(current_length.saturating_sub(selection.len()));
        if remaining == 0 || value.is_empty() {
            return false;
        }
        let append = value.chars().take(remaining).collect::<String>();
        let mut chars = current.chars().collect::<Vec<_>>();
        chars.splice(selection.clone(), append.chars());
        *current = chars.into_iter().collect();
        let cursor = selection.start + append.chars().count();
        edit.cursor = cursor;
        edit.anchor = cursor;
        !append.is_empty()
    }

    pub fn backspace(&mut self, key: &str) -> bool {
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
            }
        });
        let range = if edit.has_selection() {
            edit.selection()
        } else if edit.cursor > 0 {
            (edit.cursor - 1)..edit.cursor
        } else {
            return false;
        };
        let mut chars = value.chars().collect::<Vec<_>>();
        chars.drain(range.clone());
        *value = chars.into_iter().collect();
        edit.cursor = range.start;
        edit.anchor = range.start;
        true
    }

    pub fn delete_forward(&mut self, key: &str) -> bool {
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
            }
        });
        let range = if edit.has_selection() {
            edit.selection()
        } else if edit.cursor < value.chars().count() {
            edit.cursor..edit.cursor + 1
        } else {
            return false;
        };
        let mut chars = value.chars().collect::<Vec<_>>();
        chars.drain(range.clone());
        *value = chars.into_iter().collect();
        edit.cursor = range.start;
        edit.anchor = range.start;
        true
    }

    pub fn move_cursor(&mut self, key: &str, direction: i32, extend: bool) {
        let value = self.text_values.get(key).map(String::as_str).unwrap_or("");
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
            }
        });
        let length = value.chars().count();
        let next = if direction < 0 {
            edit.cursor.saturating_sub(1)
        } else {
            (edit.cursor + 1).min(length)
        };
        edit.cursor = next;
        if !extend {
            edit.anchor = next;
        }
    }

    /// Places the caret at a character boundary. When `extend` is true the
    /// existing anchor is preserved so pointer/keyboard movement can extend
    /// the current selection.
    pub fn set_cursor(&mut self, key: &str, cursor: usize, extend: bool) {
        let length = self
            .text_values
            .get(key)
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.cursor = cursor.min(length);
        if !extend {
            edit.anchor = edit.cursor;
        }
    }

    /// Sets both ends of a text selection using character indices rather than
    /// byte offsets. This is used by word selection and keeps Unicode input
    /// safe for all retained UI hosts.
    pub fn set_selection(&mut self, key: &str, anchor: usize, cursor: usize) {
        let length = self
            .text_values
            .get(key)
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.anchor = anchor.min(length);
        edit.cursor = cursor.min(length);
    }

    pub fn move_cursor_to_edge(&mut self, key: &str, end: bool, extend: bool) {
        let length = self
            .text_values
            .get(key)
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.cursor = if end { length } else { 0 };
        if !extend {
            edit.anchor = edit.cursor;
        }
    }

    pub fn select_all(&mut self, key: &str) {
        let length = self
            .text_values
            .get(key)
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.anchor = 0;
        edit.cursor = length;
    }

    pub fn text_edit(&self, key: &str) -> UiTextEditState {
        self.text_edit.get(key).copied().unwrap_or_else(|| {
            let end = self.text(key).chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
            }
        })
    }

    pub fn scroll_offset(&self, id: &str) -> [f32; 2] {
        self.scroll_offsets.get(id).copied().unwrap_or([0.0, 0.0])
    }

    /// Starts a retained scroll surface at its origin after its content has
    /// been replaced by another session or page.
    pub fn reset_scroll(&mut self, id: &str) {
        self.scroll_offsets.insert(id.to_string(), [0.0, 0.0]);
        self.scroll_max_offsets.remove(id);
    }

    pub fn set_scroll_metrics(&mut self, id: impl Into<String>, max_offset: [f32; 2]) {
        let id = id.into();
        let max_offset = [max_offset[0].max(0.0), max_offset[1].max(0.0)];
        self.scroll_max_offsets.insert(id.clone(), max_offset);
        let offset = self.scroll_offsets.entry(id).or_insert([0.0, 0.0]);
        offset[0] = offset[0].clamp(0.0, max_offset[0]);
        offset[1] = offset[1].clamp(0.0, max_offset[1]);
    }

    /// Updates a provisional extent without clamping the current position.
    ///
    /// Text-fit surfaces measure once before they know the final wrapped line
    /// heights. Clamping against that first estimate can discard a valid
    /// offset before the intrinsic pass computes the real extent.
    pub fn set_scroll_metrics_preserving_offset(
        &mut self,
        id: impl Into<String>,
        max_offset: [f32; 2],
    ) {
        let id = id.into();
        self.scroll_max_offsets
            .insert(id.clone(), [max_offset[0].max(0.0), max_offset[1].max(0.0)]);
        let offset = self.scroll_offsets.entry(id).or_insert([0.0, 0.0]);
        offset[0] = offset[0].max(0.0);
        offset[1] = offset[1].max(0.0);
    }

    pub fn scroll_max_offset(&self, id: &str) -> [f32; 2] {
        self.scroll_max_offsets
            .get(id)
            .copied()
            .unwrap_or([0.0, 0.0])
    }

    pub fn scroll_by(&mut self, id: impl Into<String>, delta: [f32; 2]) -> [f32; 2] {
        let id = id.into();
        let delta = [
            if delta[0].is_finite() { delta[0] } else { 0.0 },
            if delta[1].is_finite() { delta[1] } else { 0.0 },
        ];
        let max_offset = self.scroll_max_offsets.get(&id).copied();
        let offset = self.scroll_offsets.entry(id).or_insert([0.0, 0.0]);
        if let Some(max_offset) = max_offset {
            offset[0] = (offset[0] + delta[0]).clamp(0.0, max_offset[0]);
            offset[1] = (offset[1] + delta[1]).clamp(0.0, max_offset[1]);
        } else {
            // Before the first layout pass there is no trustworthy content extent.
            // Preserve the input and let the next measured frame clamp it.
            offset[0] = (offset[0] + delta[0]).max(0.0);
            offset[1] = (offset[1] + delta[1]).max(0.0);
        }
        *offset
    }
}

/// Deterministic visible range helper for virtualized lists and trees.
/// Hosts can use it without constructing off-screen child nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiVirtualRange {
    pub start: usize,
    pub end: usize,
}

impl UiVirtualRange {
    pub fn for_vertical_list(
        item_count: usize,
        scroll_offset: f32,
        viewport_height: f32,
        item_height: f32,
        overscan: usize,
    ) -> Self {
        let item_height = item_height.max(1.0);
        let first = (scroll_offset.max(0.0) / item_height).floor() as usize;
        let visible = (viewport_height.max(0.0) / item_height).ceil() as usize + 1;
        let start = first.saturating_sub(overscan).min(item_count);
        let end = first
            .saturating_add(visible)
            .saturating_add(overscan)
            .min(item_count);
        Self { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_is_clamped_to_measured_content() {
        let mut state = UiControlState::default();
        state.set_scroll_metrics("list", [0.0, 120.0]);
        assert_eq!(state.scroll_by("list", [0.0, 500.0]), [0.0, 120.0]);
        assert_eq!(state.scroll_by("list", [0.0, -500.0]), [0.0, 0.0]);
    }

    #[test]
    fn provisional_scroll_metrics_do_not_discard_a_pending_intrinsic_offset() {
        let mut state = UiControlState::default();
        state.set_scroll_metrics("list", [0.0, 500.0]);
        state.scroll_by("list", [0.0, 420.0]);

        state.set_scroll_metrics_preserving_offset("list", [0.0, 24.0]);
        assert_eq!(state.scroll_offset("list"), [0.0, 420.0]);

        state.set_scroll_metrics("list", [0.0, 260.0]);
        assert_eq!(state.scroll_offset("list"), [0.0, 260.0]);
    }

    #[test]
    fn text_edit_supports_cursor_and_selection_delete() {
        let mut state = UiControlState::default();
        state.set_text("query", "abcd", 32);
        assert!(state.backspace("query"));
        assert_eq!(state.text("query"), "abc");
        state.move_cursor_to_edge("query", false, false);
        state.move_cursor("query", 1, true);
        assert!(state.delete_forward("query"));
        assert_eq!(state.text("query"), "bc");
        state.select_all("query");
        assert!(state.text_edit("query").has_selection());
    }

    #[test]
    fn text_edit_can_place_caret_and_set_unicode_safe_selection() {
        let mut state = UiControlState::default();
        state.set_text("query", "año raf", 32);
        state.set_cursor("query", 3, false);
        assert_eq!(state.text_edit("query").cursor, 3);
        state.set_selection("query", 0, 3);
        assert_eq!(state.text_edit("query").selection(), 0..3);
    }

    #[test]
    fn reseeding_the_same_text_preserves_the_caret() {
        let mut state = UiControlState::default();
        state.set_text("query", "abcd", 32);
        state.move_cursor_to_edge("query", false, false);
        state.set_text("query", "abcd", 32);

        assert_eq!(state.text_edit("query").cursor, 0);
        assert_eq!(state.text_edit("query").anchor, 0);
    }

    #[test]
    fn virtual_range_is_bounded_and_overscanned() {
        assert_eq!(
            UiVirtualRange::for_vertical_list(100, 100.0, 40.0, 20.0, 1),
            UiVirtualRange { start: 4, end: 9 }
        );
    }
}
