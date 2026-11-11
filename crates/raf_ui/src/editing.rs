//! Host-owned text drafts. Validation and durable writes remain domain work.
use crate::UiControlState;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTextBinding {
    pub key: String,
    pub value: String,
    pub max_length: usize,
}

impl UiTextBinding {
    pub fn new(key: impl Into<String>, value: impl Into<String>, max_length: usize) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            max_length,
        }
    }
}

/// Reconciles model values without overwriting unfinished edits. A new scope
/// is an explicit selection/document change and resets this bank's fields.
#[derive(Debug, Clone, Default)]
pub struct UiTextDraftBank {
    scope: Option<String>,
    bindings: BTreeMap<String, UiTextBinding>,
}

impl UiTextDraftBank {
    pub fn sync(
        &mut self,
        scope: impl Into<String>,
        bindings: impl IntoIterator<Item = UiTextBinding>,
        controls: &mut UiControlState,
    ) -> bool {
        let scope = scope.into();
        let switched = self.scope.as_ref() != Some(&scope);
        let mut seen = BTreeSet::new();
        let mut changed = switched;
        for mut binding in bindings {
            seen.insert(binding.key.clone());
            binding.value = binding.value.chars().take(binding.max_length).collect();
            let pristine = self
                .bindings
                .get(&binding.key)
                .is_some_and(|old| controls.text(&binding.key) == old.value);
            if switched || !controls.has_text(&binding.key) || pristine {
                changed |= controls.text(&binding.key) != binding.value
                    || !controls.has_text(&binding.key);
                if switched {
                    controls.remove_text(&binding.key);
                }
                controls.set_text(&binding.key, &binding.value, binding.max_length);
            }
            self.bindings.insert(binding.key.clone(), binding);
        }
        self.bindings.retain(|key, _| {
            if seen.contains(key) {
                true
            } else {
                changed |= controls.has_text(key);
                controls.remove_text(key);
                false
            }
        });
        self.scope = Some(scope);
        changed
    }

    pub fn is_dirty(&self, key: &str, controls: &UiControlState) -> bool {
        self.bindings
            .get(key)
            .is_some_and(|binding| controls.text(key) != binding.value)
    }

    /// Restores the latest model value, including updates received while a
    /// draft was dirty. A rejected commit never changes this baseline.
    pub fn cancel(&self, key: &str, controls: &mut UiControlState) -> bool {
        let Some(binding) = self.bindings.get(key) else {
            return false;
        };
        let changed = controls.text(key) != binding.value;
        controls.remove_text(key);
        controls.set_text(key, &binding.value, binding.max_length);
        changed
    }
}
