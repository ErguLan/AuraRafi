use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per-surface values that must not be persisted into a UI document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiControlState {
    #[serde(default)]
    text_values: BTreeMap<String, String>,
    #[serde(default)]
    scroll_offsets: BTreeMap<String, [f32; 2]>,
}

impl UiControlState {
    pub fn text(&self, key: &str) -> &str {
        self.text_values.get(key).map(String::as_str).unwrap_or("")
    }

    pub fn set_text(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        max_length: usize,
    ) {
        let value = value.into().chars().take(max_length).collect::<String>();
        self.text_values.insert(key.into(), value);
    }

    pub fn append_text(&mut self, key: &str, value: &str, max_length: usize) -> bool {
        let current = self.text_values.entry(key.to_string()).or_default();
        let remaining = max_length.saturating_sub(current.chars().count());
        if remaining == 0 || value.is_empty() {
            return false;
        }
        let append = value.chars().take(remaining).collect::<String>();
        current.push_str(&append);
        !append.is_empty()
    }

    pub fn backspace(&mut self, key: &str) -> bool {
        let Some(value) = self.text_values.get_mut(key) else {
            return false;
        };
        value.pop().is_some()
    }

    pub fn scroll_offset(&self, id: &str) -> [f32; 2] {
        self.scroll_offsets.get(id).copied().unwrap_or([0.0, 0.0])
    }

    pub fn scroll_by(&mut self, id: impl Into<String>, delta: [f32; 2]) -> [f32; 2] {
        let offset = self.scroll_offsets.entry(id.into()).or_insert([0.0, 0.0]);
        // Bounds are supplied by layout once content metrics are known. The
        // finite interim clamp prevents malformed input from creating values
        // that would make an entire surface disappear.
        offset[0] = (offset[0] + delta[0]).clamp(0.0, 16_384.0);
        offset[1] = (offset[1] + delta[1]).clamp(0.0, 16_384.0);
        *offset
    }
}
