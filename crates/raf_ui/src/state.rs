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
    #[serde(default)]
    text_selection: BTreeMap<String, UiTextEditState>,
    #[serde(default)]
    ime_preedit: BTreeMap<String, String>,
    #[serde(skip)]
    text_history: BTreeMap<String, UiTextHistory>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UiTextHistory {
    undo: Vec<String>,
    redo: Vec<String>,
}

/// Session-owned caret and selection state for a text control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UiTextEditState {
    pub cursor: usize,
    pub anchor: usize,
    /// Character column to preserve while moving vertically through visual
    /// lines. This is transient editor state, not part of the authored UI.
    #[serde(default)]
    pub preferred_column: Option<usize>,
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
    /// Returns the portion of control state that changes retained layout.
    ///
    /// Measured scroll extents are outputs of layout, not layout inputs. They
    /// must stay out of renderer cache keys or an intrinsic text pass will
    /// immediately invalidate its own measured frame and leave hit-testing on
    /// the provisional scroll range.
    pub fn layout_snapshot(&self) -> Self {
        let mut snapshot = self.clone();
        snapshot.scroll_max_offsets.clear();
        snapshot
            .scroll_offsets
            .retain(|_, offset| offset.iter().any(|value| *value != 0.0));
        // Caret, selection, and IME preedit affect the rendered text pass and
        // therefore belong in the layout cache key. Dropping them here makes
        // a focused field keep painting the previous caret after typing.
        snapshot.text_history.clear();
        snapshot
    }

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

    pub fn set_ime_preedit(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if value.is_empty() {
            self.ime_preedit.remove(&key);
        } else {
            self.ime_preedit.insert(key, value);
        }
    }

    pub fn ime_preedit(&self, key: &str) -> &str {
        self.ime_preedit.get(key).map(String::as_str).unwrap_or("")
    }

    fn record_text_change(&mut self, key: &str, before: String) {
        let after = self.text(key).to_string();
        if before == after {
            return;
        }
        let history = self.text_history.entry(key.to_string()).or_default();
        history.undo.push(before);
        if history.undo.len() > 100 {
            history.undo.remove(0);
        }
        history.redo.clear();
    }

    pub fn undo(&mut self, key: &str) -> bool {
        let Some(previous) = self
            .text_history
            .get_mut(key)
            .and_then(|history| history.undo.pop())
        else {
            return false;
        };
        let current = self.text(key).to_string();
        self.text_history
            .entry(key.to_string())
            .or_default()
            .redo
            .push(current);
        let length = previous.chars().count();
        self.text_values.insert(key.to_string(), previous);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.cursor = length;
        edit.anchor = length;
        edit.preferred_column = None;
        true
    }

    pub fn redo(&mut self, key: &str) -> bool {
        let Some(next) = self
            .text_history
            .get_mut(key)
            .and_then(|history| history.redo.pop())
        else {
            return false;
        };
        let current = self.text(key).to_string();
        self.text_history
            .entry(key.to_string())
            .or_default()
            .undo
            .push(current);
        let length = next.chars().count();
        self.text_values.insert(key.to_string(), next);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.cursor = length;
        edit.anchor = length;
        edit.preferred_column = None;
        true
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
        let edit = self.text_edit.entry(key.clone()).or_default();
        if unchanged {
            edit.cursor = edit.cursor.min(length);
            edit.anchor = edit.anchor.min(length);
        } else {
            edit.cursor = length;
            edit.anchor = length;
            edit.preferred_column = None;
            self.ime_preedit.remove(&key);
        }
    }

    pub fn append_text(&mut self, key: &str, value: &str, max_length: usize) -> bool {
        let before = self.text(key).to_string();
        let current = self.text_values.entry(key.to_string()).or_default();
        let edit = self
            .text_edit
            .entry(key.to_string())
            .or_insert_with(|| UiTextEditState {
                cursor: current.chars().count(),
                anchor: current.chars().count(),
                preferred_column: None,
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
        edit.preferred_column = None;
        let changed = !append.is_empty();
        if changed {
            self.record_text_change(key, before);
        }
        changed
    }

    pub fn backspace(&mut self, key: &str) -> bool {
        let before = self.text(key).to_string();
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
                preferred_column: None,
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
        edit.preferred_column = None;
        self.record_text_change(key, before);
        true
    }

    /// Deletes the word immediately before the caret. Whitespace is treated
    /// as a separator, matching the behavior users expect from native text
    /// fields when pressing Ctrl/Command+Backspace.
    pub fn delete_backward_word(&mut self, key: &str) -> bool {
        if self.text_edit(key).has_selection() {
            return self.backspace(key);
        }
        let before = self.text(key).to_string();
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
                preferred_column: None,
            }
        });
        if edit.cursor == 0 {
            return false;
        }
        let chars = value.chars().collect::<Vec<_>>();
        let mut start = edit.cursor.min(chars.len());
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let range = start..edit.cursor.min(chars.len());
        let mut chars = chars;
        chars.drain(range.clone());
        *value = chars.into_iter().collect();
        edit.cursor = range.start;
        edit.anchor = range.start;
        edit.preferred_column = None;
        self.record_text_change(key, before);
        true
    }

    pub fn delete_forward(&mut self, key: &str) -> bool {
        let before = self.text(key).to_string();
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
                preferred_column: None,
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
        edit.preferred_column = None;
        self.record_text_change(key, before);
        true
    }

    pub fn delete_forward_word(&mut self, key: &str) -> bool {
        if self.text_edit(key).has_selection() {
            return self.delete_forward(key);
        }
        let before = self.text(key).to_string();
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
                preferred_column: None,
            }
        });
        let chars = value.chars().collect::<Vec<_>>();
        let mut end = edit.cursor.min(chars.len());
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }
        if end == edit.cursor.min(chars.len()) {
            return false;
        }
        let range = edit.cursor.min(chars.len())..end;
        let mut chars = chars;
        chars.drain(range.clone());
        *value = chars.into_iter().collect();
        edit.cursor = range.start;
        edit.anchor = range.start;
        edit.preferred_column = None;
        self.record_text_change(key, before);
        true
    }

    pub fn selected_text(&self, key: &str) -> String {
        let Some(value) = self.text_values.get(key) else {
            return String::new();
        };
        let edit = self.text_edit(key);
        if !edit.has_selection() {
            return String::new();
        }
        value
            .chars()
            .skip(edit.selection().start)
            .take(edit.selection().len())
            .collect()
    }

    pub fn move_cursor(&mut self, key: &str, direction: i32, extend: bool) {
        let value = self.text_values.get(key).map(String::as_str).unwrap_or("");
        let edit = self.text_edit.entry(key.to_string()).or_insert_with(|| {
            let end = value.chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
                preferred_column: None,
            }
        });
        let length = value.chars().count();
        let next = if direction < 0 {
            edit.cursor.saturating_sub(1)
        } else {
            (edit.cursor + 1).min(length)
        };
        edit.cursor = next;
        edit.preferred_column = None;
        if !extend {
            edit.anchor = next;
        }
    }

    /// Moves by semantic words for native-feeling Ctrl/Command navigation.
    pub fn move_cursor_by_word(&mut self, key: &str, direction: i32, extend: bool) {
        let chars = self.text(key).chars().collect::<Vec<_>>();
        let mut cursor = self.text_edit(key).cursor.min(chars.len());
        if direction < 0 {
            while cursor > 0 && chars[cursor - 1].is_whitespace() {
                cursor -= 1;
            }
            while cursor > 0 && !chars[cursor - 1].is_whitespace() {
                cursor -= 1;
            }
        } else {
            while cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            while cursor < chars.len() && !chars[cursor].is_whitespace() {
                cursor += 1;
            }
        }
        self.set_cursor(key, cursor, extend);
    }

    /// Moves to the beginning/end of the current line. Newline characters are
    /// treated as boundaries and remain outside the editable line content.
    pub fn move_cursor_to_line_edge(&mut self, key: &str, end: bool, extend: bool) {
        let chars = self.text(key).chars().collect::<Vec<_>>();
        let cursor = self.text_edit(key).cursor.min(chars.len());
        let mut line_start = cursor;
        while line_start > 0 && chars[line_start - 1] != '\n' {
            line_start -= 1;
        }
        let mut line_end = cursor;
        while line_end < chars.len() && chars[line_end] != '\n' {
            line_end += 1;
        }
        self.set_cursor(key, if end { line_end } else { line_start }, extend);
    }

    /// Moves vertically through explicit newline-delimited lines.
    pub fn move_cursor_vertical(&mut self, key: &str, direction: i32, extend: bool) {
        let chars = self.text(key).chars().collect::<Vec<_>>();
        let mut lines = Vec::new();
        let mut start = 0;
        for (index, character) in chars.iter().enumerate() {
            if *character == '\n' {
                lines.push((start, index));
                start = index + 1;
            }
        }
        lines.push((start, chars.len()));
        self.move_cursor_vertical_with_lines(key, direction, extend, &lines);
    }

    /// Moves vertically through the visual line ranges produced by the text
    /// renderer. The ranges use character indices and may come from either
    /// explicit newlines or soft wrapping.
    pub fn move_cursor_vertical_with_lines(
        &mut self,
        key: &str,
        direction: i32,
        extend: bool,
        lines: &[(usize, usize)],
    ) {
        if lines.is_empty() {
            return;
        }
        let length = self.text(key).chars().count();
        let edit = self.text_edit.entry(key.to_string()).or_default();
        let cursor = edit.cursor.min(length);
        let current_index = lines
            .iter()
            .enumerate()
            .position(|(index, (start, end))| {
                cursor >= *start
                    && (cursor < *end
                        || cursor == *end
                            && lines
                                .get(index + 1)
                                .is_none_or(|(next_start, _)| *next_start != cursor))
            })
            .unwrap_or_else(|| lines.len().saturating_sub(1));
        let target_index = if direction < 0 {
            current_index.checked_sub(1)
        } else {
            (current_index + 1 < lines.len()).then_some(current_index + 1)
        };
        let Some(target_index) = target_index else {
            return;
        };
        let (current_start, current_end) = lines[current_index];
        let column = edit
            .preferred_column
            .unwrap_or_else(|| cursor.saturating_sub(current_start.min(current_end)));
        let (target_start, target_end) = lines[target_index];
        let target = (target_start + column).min(target_end);
        edit.cursor = target.min(length);
        edit.preferred_column = Some(column);
        if !extend {
            edit.anchor = edit.cursor;
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
        edit.preferred_column = None;
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
        edit.preferred_column = None;
    }

    /// Returns the selection state for a read-only selectable text node. The
    /// text itself stays in the retained node; only the transient character
    /// boundaries live in this per-surface state.
    pub fn selectable_text_edit(&self, key: &str, text: &str) -> UiTextEditState {
        let length = text.chars().count();
        self.text_selection
            .get(key)
            .copied()
            .map(|mut edit| {
                edit.cursor = edit.cursor.min(length);
                edit.anchor = edit.anchor.min(length);
                edit
            })
            .unwrap_or(UiTextEditState {
                cursor: length,
                anchor: length,
                preferred_column: None,
            })
    }

    pub fn set_selectable_cursor(&mut self, key: &str, text: &str, cursor: usize, extend: bool) {
        let length = text.chars().count();
        let edit = self
            .text_selection
            .entry(key.to_string())
            .or_insert(UiTextEditState {
                cursor: length,
                anchor: length,
                preferred_column: None,
            });
        edit.cursor = cursor.min(length);
        edit.preferred_column = None;
        if !extend {
            edit.anchor = edit.cursor;
        }
    }

    pub fn move_selectable_cursor(&mut self, key: &str, text: &str, direction: i32, extend: bool) {
        let edit = self.selectable_text_edit(key, text);
        let next = if direction < 0 {
            edit.cursor.saturating_sub(1)
        } else {
            (edit.cursor + 1).min(text.chars().count())
        };
        self.set_selectable_cursor(key, text, next, extend);
    }

    pub fn move_selectable_cursor_to_edge(
        &mut self,
        key: &str,
        text: &str,
        end: bool,
        extend: bool,
    ) {
        self.set_selectable_cursor(
            key,
            text,
            if end { text.chars().count() } else { 0 },
            extend,
        );
    }

    pub fn select_all_selectable(&mut self, key: &str, text: &str) {
        let length = text.chars().count();
        self.text_selection.insert(
            key.to_string(),
            UiTextEditState {
                cursor: length,
                anchor: 0,
                preferred_column: None,
            },
        );
    }

    pub fn selected_selectable_text(&self, key: &str, text: &str) -> String {
        let edit = self.selectable_text_edit(key, text);
        if !edit.has_selection() {
            return String::new();
        }
        text.chars()
            .skip(edit.selection().start)
            .take(edit.selection().len())
            .collect()
    }

    pub fn move_cursor_to_edge(&mut self, key: &str, end: bool, extend: bool) {
        let length = self
            .text_values
            .get(key)
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let edit = self.text_edit.entry(key.to_string()).or_default();
        edit.cursor = if end { length } else { 0 };
        edit.preferred_column = None;
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
        edit.preferred_column = None;
    }

    pub fn text_edit(&self, key: &str) -> UiTextEditState {
        self.text_edit.get(key).copied().unwrap_or_else(|| {
            let end = self.text(key).chars().count();
            UiTextEditState {
                cursor: end,
                anchor: end,
                preferred_column: None,
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
    /// offset before the intrinsic pass computes the real extent. Keep a
    /// previously measured larger extent until that authoritative pass runs.
    pub fn set_scroll_metrics_preserving_offset(
        &mut self,
        id: impl Into<String>,
        max_offset: [f32; 2],
    ) {
        let id = id.into();
        let previous = self
            .scroll_max_offsets
            .get(&id)
            .copied()
            .unwrap_or([0.0; 2]);
        self.scroll_max_offsets.insert(
            id.clone(),
            [
                max_offset[0].max(previous[0]).max(0.0),
                max_offset[1].max(previous[1]).max(0.0),
            ],
        );
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

    /// Sets a scroll position from a direct-manipulation control such as the
    /// retained scrollbar thumb. The same clamping rules as wheel scrolling
    /// apply, so hosts cannot leave a scroll view outside its measured extent.
    pub fn set_scroll_offset(&mut self, id: impl Into<String>, offset: [f32; 2]) -> [f32; 2] {
        let id = id.into();
        let offset = [
            if offset[0].is_finite() {
                offset[0]
            } else {
                0.0
            },
            if offset[1].is_finite() {
                offset[1]
            } else {
                0.0
            },
        ];
        let max_offset = self
            .scroll_max_offsets
            .get(&id)
            .copied()
            .unwrap_or([0.0, 0.0]);
        let current = self.scroll_offsets.entry(id).or_insert([0.0, 0.0]);
        current[0] = offset[0].clamp(0.0, max_offset[0]);
        current[1] = offset[1].clamp(0.0, max_offset[1]);
        *current
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
    fn direct_scroll_offset_is_clamped_for_manual_scrollbar_drags() {
        let mut state = UiControlState::default();
        state.set_scroll_metrics("list", [0.0, 240.0]);
        assert_eq!(state.set_scroll_offset("list", [0.0, 160.0]), [0.0, 160.0]);
        assert_eq!(state.set_scroll_offset("list", [0.0, 999.0]), [0.0, 240.0]);
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
    fn text_edit_supports_word_delete_and_selected_text() {
        let mut state = UiControlState::default();
        state.set_text("query", "hello world", 32);
        assert_eq!(state.selected_text("query"), "");
        assert!(state.delete_backward_word("query"));
        assert_eq!(state.text("query"), "hello ");
        state.set_cursor("query", 0, false);
        assert!(state.delete_forward_word("query"));
        assert_eq!(state.text("query"), " ");
    }

    #[test]
    fn text_edit_preserves_spaces_at_the_caret() {
        let mut state = UiControlState::default();
        state.set_text("query", "dsad1adssd", 32);
        state.set_cursor("query", 4, false);
        assert!(state.append_text("query", " ", 32));

        assert_eq!(state.text("query"), "dsad 1adssd");
        assert_eq!(state.text_edit("query").cursor, 5);

        assert!(state.append_text("query", "  ", 32));
        assert_eq!(state.text("query"), "dsad   1adssd");
    }

    #[test]
    fn vertical_navigation_keeps_the_preferred_visual_column() {
        let mut state = UiControlState::default();
        state.set_text("query", "abcd1234", 32);
        state.set_cursor("query", 3, false);
        state.move_cursor_vertical_with_lines("query", 1, false, &[(0, 4), (4, 8)]);

        assert_eq!(state.text_edit("query").cursor, 7);
        assert_eq!(state.text_edit("query").preferred_column, Some(3));
    }

    #[test]
    fn selectable_text_keeps_read_only_selection_and_returns_selected_content() {
        let mut state = UiControlState::default();
        let text = "hello agent";
        state.set_selectable_cursor("message", text, 0, false);
        state.set_selectable_cursor("message", text, 5, true);

        assert_eq!(state.selected_selectable_text("message", text), "hello");
        assert_eq!(
            state.selectable_text_edit("message", text).selection(),
            0..5
        );

        state.select_all_selectable("message", text);
        assert_eq!(
            state.selected_selectable_text("message", text),
            "hello agent"
        );
    }

    #[test]
    fn virtual_range_is_bounded_and_overscanned() {
        assert_eq!(
            UiVirtualRange::for_vertical_list(100, 100.0, 40.0, 20.0, 1),
            UiVirtualRange { start: 4, end: 9 }
        );
    }
}
