//! Transient numeric editing contracts for retained text controls.
//!
//! The editor keeps the user's raw draft while they type. Parsing and range
//! validation happen only when a host asks to commit; this module emits typed
//! intent and never writes application or domain state.

use std::collections::BTreeMap;

use crate::controls::{UiControl, UiRange};
use crate::UiNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiNumericUnit {
    None,
    Percent,
    Pixels,
    Degrees,
    Meters,
    Millimeters,
    Seconds,
    Milliseconds,
}

impl UiNumericUnit {
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Percent => "%",
            Self::Pixels => "px",
            Self::Degrees => "°",
            Self::Meters => "m",
            Self::Millimeters => "mm",
            Self::Seconds => "s",
            Self::Milliseconds => "ms",
        }
    }

    fn parse_suffix(suffix: &str) -> Option<Self> {
        match suffix.trim().to_ascii_lowercase().as_str() {
            "" => Some(Self::None),
            "%" | "percent" => Some(Self::Percent),
            "px" | "pixel" | "pixels" => Some(Self::Pixels),
            "°" | "deg" | "degree" | "degrees" => Some(Self::Degrees),
            "m" | "meter" | "meters" | "metre" | "metres" => Some(Self::Meters),
            "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => {
                Some(Self::Millimeters)
            }
            "s" | "sec" | "second" | "seconds" => Some(Self::Seconds),
            "ms" | "millisecond" | "milliseconds" => Some(Self::Milliseconds),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiNumericSpec {
    pub min: f32,
    pub max: f32,
    pub unit: UiNumericUnit,
}

impl UiNumericSpec {
    pub fn new(min: f32, max: f32, unit: UiNumericUnit) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min <= max).then_some(Self { min, max, unit })
    }

    pub fn validate(self, text: &str) -> UiNumericValidation {
        if !self.min.is_finite() || !self.max.is_finite() || self.min > self.max {
            return UiNumericValidation::Invalid;
        }
        let text = text.trim();
        if text.is_empty() {
            return UiNumericValidation::Incomplete;
        }

        let (number_text, supplied_unit) = match split_number_and_unit(text) {
            Ok(parts) => parts,
            Err(()) => return UiNumericValidation::Invalid,
        };
        if is_incomplete_number(number_text) {
            return UiNumericValidation::Incomplete;
        }
        if let Some(supplied_unit) = supplied_unit {
            if supplied_unit != self.unit {
                return UiNumericValidation::WrongUnit {
                    expected: self.unit,
                };
            }
        }

        let normalized = normalize_decimal_separator(number_text);
        let value = match normalized.parse::<f32>() {
            Ok(value) if value.is_finite() => value,
            _ => return UiNumericValidation::Invalid,
        };
        if value < self.min || value > self.max {
            return UiNumericValidation::OutOfRange {
                value,
                min: self.min,
                max: self.max,
            };
        }
        UiNumericValidation::Valid(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiNumericValidation {
    Incomplete,
    Invalid,
    WrongUnit { expected: UiNumericUnit },
    OutOfRange { value: f32, min: f32, max: f32 },
    Valid(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiNumericEditAction {
    Began {
        key: String,
        original_text: String,
    },
    DraftChanged {
        key: String,
        text: String,
        validation: UiNumericValidation,
    },
    Committed {
        key: String,
        value: f32,
    },
    Rejected {
        key: String,
        text: String,
        validation: UiNumericValidation,
    },
    Cancelled {
        key: String,
        value: f32,
        restored_text: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNumericEditSession {
    key: String,
    spec: UiNumericSpec,
    committed_value: f32,
    original_text: String,
    draft_text: String,
    editing: bool,
    validation: UiNumericValidation,
    rejected: bool,
}

impl UiNumericEditSession {
    pub fn new(
        key: impl Into<String>,
        spec: UiNumericSpec,
        committed_value: f32,
        initial_text: impl Into<String>,
    ) -> Self {
        let initial_text = initial_text.into();
        let validation = spec.validate(&initial_text);
        Self {
            key: key.into(),
            spec,
            committed_value,
            original_text: initial_text.clone(),
            draft_text: initial_text,
            editing: false,
            validation,
            rejected: false,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn spec(&self) -> UiNumericSpec {
        self.spec
    }

    pub fn committed_value(&self) -> f32 {
        self.committed_value
    }

    pub fn draft_text(&self) -> &str {
        &self.draft_text
    }

    pub fn validation(&self) -> UiNumericValidation {
        self.validation
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    pub fn is_rejected(&self) -> bool {
        self.rejected
    }

    fn update_spec(&mut self, spec: UiNumericSpec) {
        self.spec = spec;
        self.validation = self.spec.validate(&self.draft_text);
    }

    /// Starts an edit without replacing an already active draft.
    pub fn begin(&mut self) -> Option<UiNumericEditAction> {
        if self.editing {
            return None;
        }
        self.editing = true;
        self.original_text = self.draft_text.clone();
        Some(UiNumericEditAction::Began {
            key: self.key.clone(),
            original_text: self.original_text.clone(),
        })
    }

    /// Retains partial, invalid, and out-of-range text exactly as entered.
    pub fn edit(&mut self, text: impl Into<String>) -> UiNumericEditAction {
        if !self.editing {
            let _ = self.begin();
        }
        self.draft_text = text.into();
        self.validation = self.spec.validate(&self.draft_text);
        self.rejected = false;
        UiNumericEditAction::DraftChanged {
            key: self.key.clone(),
            text: self.draft_text.clone(),
            validation: self.validation,
        }
    }

    /// Commits only a finite value with the expected unit and range.
    /// Rejection leaves the edit active and preserves its raw text.
    pub fn commit(&mut self) -> UiNumericEditAction {
        self.validation = self.spec.validate(&self.draft_text);
        match self.validation {
            UiNumericValidation::Valid(value) => {
                self.committed_value = value;
                self.original_text = self.draft_text.clone();
                self.editing = false;
                self.rejected = false;
                UiNumericEditAction::Committed {
                    key: self.key.clone(),
                    value,
                }
            }
            validation => {
                self.rejected = true;
                UiNumericEditAction::Rejected {
                    key: self.key.clone(),
                    text: self.draft_text.clone(),
                    validation,
                }
            }
        }
    }

    /// Restores the value and text captured by `begin`.
    pub fn cancel(&mut self) -> UiNumericEditAction {
        self.draft_text = self.original_text.clone();
        self.validation = self.spec.validate(&self.draft_text);
        self.editing = false;
        self.rejected = false;
        UiNumericEditAction::Cancelled {
            key: self.key.clone(),
            value: self.committed_value,
            restored_text: self.draft_text.clone(),
        }
    }

    /// Synchronizes an authoritative host value when there is no active edit.
    /// Active partial edits are intentionally left untouched.
    pub fn sync_committed(&mut self, value: f32, text: impl Into<String>) -> bool {
        if self.editing || !value.is_finite() {
            return false;
        }
        let text = text.into();
        self.committed_value = value;
        self.original_text = text.clone();
        self.draft_text = text;
        self.validation = self.spec.validate(&self.draft_text);
        self.rejected = false;
        true
    }

    /// Synchronizes an authoritative value while preserving an in-progress
    /// raw draft. Cancel then restores the latest authoritative value.
    pub fn sync_authoritative(&mut self, value: f32, text: impl Into<String>) -> bool {
        if !value.is_finite() {
            return false;
        }
        let text = text.into();
        self.committed_value = value;
        self.original_text = text.clone();
        if !self.editing {
            self.draft_text = text;
            self.rejected = false;
        }
        self.validation = self.spec.validate(&self.draft_text);
        true
    }

    /// Resets the transient editor after the owning surface is cancelled or
    /// replaced. This deliberately discards any active partial edit.
    pub fn reset(&mut self, value: f32, text: impl Into<String>) -> bool {
        if !value.is_finite() {
            return false;
        }
        let text = text.into();
        self.committed_value = value;
        self.original_text = text.clone();
        self.draft_text = text;
        self.validation = self.spec.validate(&self.draft_text);
        self.editing = false;
        self.rejected = false;
        true
    }
}

/// A numeric text control paired with an authored range control.
#[derive(Debug, Clone, PartialEq)]
pub struct UiNumericBinding {
    /// The authoritative value key carried by `UiRange`.
    pub key: String,
    /// The transient text key carried by `UiTextInput`.
    pub text_key: String,
    /// The text input node that should receive invalid state.
    pub node_id: String,
    pub submit_command: Option<String>,
    pub spec: UiNumericSpec,
    pub step: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNumericEditValue {
    pub key: String,
    pub text_key: String,
    pub node_id: String,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNumericDraftIssue {
    pub key: String,
    pub text_key: String,
    pub node_id: String,
    pub validation: UiNumericValidation,
}

/// Host-owned transient sessions inferred from paired `UiRange` and
/// `UiTextInput` controls. Repeated calls to `merge_tree` retain entries from
/// previously visited tabs while synchronizing values and authored ranges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiNumericEditBank {
    bindings: BTreeMap<String, UiNumericBinding>,
    sessions: BTreeMap<String, UiNumericEditSession>,
    unit_overrides: BTreeMap<String, UiNumericUnit>,
}

impl UiNumericEditBank {
    /// Merges all numeric range/text pairs found under this tree. A text input
    /// is paired by its submit command first, then by `value_key` or the
    /// conventional `<range-key>.text` key.
    pub fn merge_tree(&mut self, root: &UiNode) {
        let mut ranges = Vec::new();
        let mut inputs = Vec::new();
        collect_numeric_controls(root, true, &mut ranges, &mut inputs);

        for (range, range_enabled) in ranges {
            let Some((node_id, input, input_enabled)) = inputs
                .iter()
                .filter(|(_, input, _)| input_matches_range(input, &range.value_key))
                .max_by_key(|(_, input, _)| {
                    usize::from(command_targets_key(
                        input.submit_command.as_deref(),
                        &range.value_key,
                    ))
                })
            else {
                continue;
            };
            let key = range.value_key.clone();
            let unit = self
                .unit_overrides
                .get(&key)
                .copied()
                .or_else(|| self.bindings.get(&key).map(|binding| binding.spec.unit))
                .unwrap_or(UiNumericUnit::None);
            let Some(spec) = UiNumericSpec::new(range.min, range.max, unit) else {
                continue;
            };
            let initial_text = format_numeric_value(range.value, range.step);
            let binding = UiNumericBinding {
                key: key.clone(),
                text_key: input.value_key.clone(),
                node_id: node_id.clone(),
                submit_command: input.submit_command.clone(),
                spec,
                step: range.step,
                enabled: range_enabled && *input_enabled,
            };
            self.bindings.insert(key.clone(), binding);
            if let Some(session) = self.sessions.get_mut(&key) {
                session.update_spec(spec);
                session.sync_authoritative(range.value, initial_text);
            } else {
                self.sessions.insert(
                    key.clone(),
                    UiNumericEditSession::new(key, spec, range.value, initial_text),
                );
            }
        }
    }

    /// Explicit unit metadata for controls whose surface contract does not
    /// encode a unit. The authored range continues to supply min, max, value,
    /// and step.
    pub fn set_unit(&mut self, key: &str, unit: UiNumericUnit) -> bool {
        self.unit_overrides.insert(key.to_string(), unit);
        let Some(binding) = self.bindings.get_mut(key) else {
            return false;
        };
        binding.spec.unit = unit;
        if let Some(session) = self.sessions.get_mut(key) {
            session.update_spec(binding.spec);
        }
        true
    }

    pub fn bindings(&self) -> impl Iterator<Item = &UiNumericBinding> {
        self.bindings.values()
    }

    pub fn binding_for_text_key(&self, text_key: &str) -> Option<&UiNumericBinding> {
        self.bindings
            .values()
            .find(|binding| binding.text_key == text_key)
    }

    pub fn binding_for_submit_command(&self, command: &str) -> Option<&UiNumericBinding> {
        self.bindings
            .values()
            .find(|binding| binding.submit_command.as_deref() == Some(command))
    }

    pub fn binding_for_node(&self, node_id: &str) -> Option<&UiNumericBinding> {
        self.bindings
            .values()
            .find(|binding| binding.node_id == node_id)
    }

    pub fn edit_text(
        &mut self,
        text_key: &str,
        text: impl Into<String>,
    ) -> Option<UiNumericEditAction> {
        let binding = self.binding_for_text_key(text_key)?;
        if !binding.enabled {
            return None;
        }
        let key = binding.key.clone();
        self.sessions
            .get_mut(&key)
            .map(|session| session.edit(text))
    }

    pub fn commit_for_submit(&mut self, command: &str) -> Option<UiNumericEditAction> {
        let binding = self.binding_for_submit_command(command)?;
        if !binding.enabled {
            return None;
        }
        let key = binding.key.clone();
        self.sessions
            .get_mut(&key)
            .map(UiNumericEditSession::commit)
    }

    pub fn cancel(&mut self, key: &str) -> Option<UiNumericEditAction> {
        let session = self.sessions.get_mut(key)?;
        session.is_editing().then(|| session.cancel())
    }

    pub fn cancel_all(&mut self) -> Vec<UiNumericEditAction> {
        self.sessions
            .values_mut()
            .filter(|session| session.is_editing())
            .map(UiNumericEditSession::cancel)
            .collect()
    }

    pub fn draft_text(&self, key: &str) -> Option<&str> {
        self.sessions.get(key).map(UiNumericEditSession::draft_text)
    }

    pub fn validation(&self, key: &str) -> Option<UiNumericValidation> {
        self.sessions.get(key).map(UiNumericEditSession::validation)
    }

    pub fn is_invalid(&self, key: &str) -> bool {
        let Some(session) = self.sessions.get(key) else {
            return false;
        };
        session.is_rejected()
            || matches!(
                session.validation(),
                UiNumericValidation::Invalid
                    | UiNumericValidation::WrongUnit { .. }
                    | UiNumericValidation::OutOfRange { .. }
            )
    }

    pub fn mark_rejected(&mut self, key: &str) -> bool {
        let Some(session) = self.sessions.get_mut(key) else {
            return false;
        };
        session.rejected = true;
        true
    }

    pub fn format_value(&self, key: &str, value: f32) -> Option<String> {
        self.bindings
            .get(key)
            .map(|binding| format_numeric_value(value, binding.step))
    }

    pub fn pending_values(&self) -> Result<Vec<UiNumericEditValue>, Vec<UiNumericDraftIssue>> {
        let mut values = Vec::new();
        let mut issues = Vec::new();
        for (key, binding) in &self.bindings {
            if !binding.enabled {
                continue;
            }
            let Some(session) = self
                .sessions
                .get(key)
                .filter(|session| session.is_editing())
            else {
                continue;
            };
            match binding.spec.validate(session.draft_text()) {
                UiNumericValidation::Valid(value) => values.push(UiNumericEditValue {
                    key: key.clone(),
                    text_key: binding.text_key.clone(),
                    node_id: binding.node_id.clone(),
                    value,
                }),
                validation => issues.push(UiNumericDraftIssue {
                    key: key.clone(),
                    text_key: binding.text_key.clone(),
                    node_id: binding.node_id.clone(),
                    validation,
                }),
            }
        }
        if issues.is_empty() {
            Ok(values)
        } else {
            Err(issues)
        }
    }

    /// Commits all enabled pending sessions together. If any draft is invalid,
    /// no valid session is committed and every invalid session is marked.
    pub fn commit_pending(&mut self) -> Result<Vec<UiNumericEditValue>, Vec<UiNumericDraftIssue>> {
        let values = match self.pending_values() {
            Ok(values) => values,
            Err(issues) => {
                for issue in &issues {
                    self.mark_rejected(&issue.key);
                }
                return Err(issues);
            }
        };

        let mut next_sessions = self.sessions.clone();
        for value in &values {
            let Some(session) = next_sessions.get_mut(&value.key) else {
                continue;
            };
            if !matches!(session.commit(), UiNumericEditAction::Committed { .. }) {
                return Err(vec![UiNumericDraftIssue {
                    key: value.key.clone(),
                    text_key: value.text_key.clone(),
                    node_id: value.node_id.clone(),
                    validation: UiNumericValidation::Invalid,
                }]);
            }
        }
        self.sessions = next_sessions;
        Ok(values)
    }

    /// Applies an authoritative value while keeping the raw draft if the
    /// field is currently being edited.
    pub fn sync_committed(&mut self, key: &str, value: f32, text: impl Into<String>) -> bool {
        let Some(session) = self.sessions.get_mut(key) else {
            return false;
        };
        session.sync_authoritative(value, text)
    }
}

fn collect_numeric_controls(
    node: &UiNode,
    ancestors_enabled: bool,
    ranges: &mut Vec<(UiRange, bool)>,
    inputs: &mut Vec<(String, crate::controls::UiTextInput, bool)>,
) {
    let enabled = ancestors_enabled && !node.disabled && !node.hidden;
    match &node.control {
        UiControl::Range(range) => ranges.push((range.clone(), enabled)),
        UiControl::TextInput(input) => {
            inputs.push((node.id.clone(), input.clone(), enabled));
        }
        _ => {}
    }
    for child in &node.children {
        collect_numeric_controls(child, enabled, ranges, inputs);
    }
}

fn input_matches_range(input: &crate::controls::UiTextInput, key: &str) -> bool {
    command_targets_key(input.submit_command.as_deref(), key)
        || input.value_key == key
        || input.value_key.strip_suffix(".text") == Some(key)
}

fn command_targets_key(command: Option<&str>, key: &str) -> bool {
    command.and_then(|command| command.rsplit_once(':').map(|(_, target)| target)) == Some(key)
}

fn format_numeric_value(value: f32, step: f32) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let step = if step.is_finite() && step > 0.0 {
        step
    } else {
        1.0
    };
    let decimals = (0_usize..=6)
        .find(|decimals| {
            let scaled = step * 10_f32.powi(*decimals as i32);
            (scaled - scaled.round()).abs() <= 0.0001 * scaled.abs().max(1.0)
        })
        .unwrap_or(6);
    format!("{value:.decimals$}")
}

fn split_number_and_unit(text: &str) -> Result<(&str, Option<UiNumericUnit>), ()> {
    let trimmed = text.trim();
    if let Some(number) = trimmed.strip_suffix('%') {
        return Ok((number.trim(), Some(UiNumericUnit::Percent)));
    }
    if let Some(number) = trimmed.strip_suffix('°') {
        return Ok((number.trim(), Some(UiNumericUnit::Degrees)));
    }
    if normalize_decimal_separator(trimmed).parse::<f32>().is_ok() {
        return Ok((trimmed, None));
    }

    const SUFFIXES: [&str; 25] = [
        "milliseconds",
        "millisecond",
        "millimeters",
        "millimetres",
        "millimeter",
        "millimetre",
        "percent",
        "degrees",
        "degree",
        "pixels",
        "pixel",
        "seconds",
        "second",
        "meters",
        "metres",
        "meter",
        "metre",
        "deg",
        "sec",
        "mm",
        "ms",
        "px",
        "m",
        "s",
        "°",
    ];
    let lower = trimmed.to_ascii_lowercase();
    for suffix in SUFFIXES {
        let Some(number) = lower.strip_suffix(suffix) else {
            continue;
        };
        let number = number.trim();
        if normalize_decimal_separator(number).parse::<f32>().is_ok()
            || is_incomplete_number(number)
        {
            let byte_length = number.len();
            let unit = UiNumericUnit::parse_suffix(suffix).ok_or(())?;
            return Ok((&trimmed[..byte_length], Some(unit)));
        }
    }
    if trimmed
        .chars()
        .last()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return Err(());
    }
    Ok((trimmed, None))
}

fn is_incomplete_number(text: &str) -> bool {
    let mut chars = text.trim().chars().peekable();
    if matches!(chars.peek(), Some('+' | '-')) {
        chars.next();
    }

    let mut has_mantissa_digit = false;
    let mut has_decimal = false;
    while let Some(character) = chars.peek().copied() {
        match character {
            '0'..='9' => {
                has_mantissa_digit = true;
                chars.next();
            }
            '.' | ',' if !has_decimal => {
                has_decimal = true;
                chars.next();
            }
            _ => break,
        }
    }

    if !has_mantissa_digit && chars.peek().is_none() {
        return true;
    }
    if has_decimal
        && chars.peek().is_none()
        && matches!(text.trim().chars().last(), Some('.' | ','))
    {
        return true;
    }
    if matches!(chars.peek(), Some('e' | 'E')) {
        chars.next();
        if matches!(chars.peek(), Some('+' | '-')) {
            chars.next();
        }
        return chars.peek().is_none();
    }
    false
}

fn normalize_decimal_separator(text: &str) -> String {
    if !text.contains('.') && text.matches(',').count() == 1 {
        text.replace(',', ".")
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::UiTextInput;
    use crate::UiNodeKind;

    fn millimeter_spec() -> UiNumericSpec {
        UiNumericSpec::new(5.0, 100.0, UiNumericUnit::Millimeters).expect("valid spec")
    }

    #[test]
    fn validates_units_range_and_decimal_comma() {
        assert_eq!(
            millimeter_spec().validate("25,5 mm"),
            UiNumericValidation::Valid(25.5)
        );
        assert!(matches!(
            millimeter_spec().validate("25 px"),
            UiNumericValidation::WrongUnit { .. }
        ));
        assert!(matches!(
            millimeter_spec().validate("101 mm"),
            UiNumericValidation::OutOfRange { .. }
        ));
        assert_eq!(
            millimeter_spec().validate("NaN mm"),
            UiNumericValidation::Invalid
        );
        assert_eq!(
            millimeter_spec().validate("25, mm"),
            UiNumericValidation::Incomplete
        );
    }

    #[test]
    fn incomplete_draft_survives_rejected_commit_and_cancel_restores_original() {
        let mut edit = UiNumericEditSession::new("grid.step", millimeter_spec(), 25.0, "25");
        assert!(matches!(
            edit.begin(),
            Some(UiNumericEditAction::Began { .. })
        ));
        edit.edit("-");
        assert!(matches!(
            edit.commit(),
            UiNumericEditAction::Rejected {
                validation: UiNumericValidation::Incomplete,
                ..
            }
        ));
        assert_eq!(edit.draft_text(), "-");
        assert!(matches!(
            edit.cancel(),
            UiNumericEditAction::Cancelled {
                restored_text,
                value: 25.0,
                ..
            } if restored_text == "25"
        ));
    }

    #[test]
    fn host_sync_does_not_clobber_an_active_partial_edit() {
        let mut edit = UiNumericEditSession::new("grid.step", millimeter_spec(), 25.0, "25");
        edit.begin();
        edit.edit("2");

        assert!(!edit.sync_committed(30.0, "30"));
        assert_eq!(edit.draft_text(), "2");
        edit.cancel();
        assert!(edit.sync_committed(30.0, "30"));
        assert_eq!(edit.draft_text(), "30");
    }

    fn numeric_tree(
        key: &str,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        command_prefix: &str,
    ) -> UiNode {
        UiNode::new(format!("{key}.row"), UiNodeKind::Panel).with_children([
            UiNode::range(
                format!("{key}.range"),
                UiRange::new(key, value, min, max, step),
            ),
            UiNode::text_input(
                format!("{key}.input"),
                UiTextInput {
                    value_key: format!("{key}.text"),
                    placeholder_key: None,
                    max_length: 32,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("{command_prefix}:{key}")),
                },
            ),
        ])
    }

    #[test]
    fn bank_infers_range_binding_and_retains_drafts_across_filtered_trees() {
        let key = "project.runtime.limit";
        let mut bank = UiNumericEditBank::default();
        bank.merge_tree(&numeric_tree(
            key,
            4.0,
            1.0,
            9.0,
            0.5,
            "project-settings.commit_numeric",
        ));

        let binding = bank
            .binding_for_submit_command("project-settings.commit_numeric:project.runtime.limit")
            .expect("paired numeric control");
        assert_eq!(binding.key, key);
        assert_eq!(binding.text_key, "project.runtime.limit.text");
        assert_eq!(binding.node_id, "project.runtime.limit.input");
        assert_eq!(binding.spec.min, 1.0);
        assert_eq!(binding.spec.max, 9.0);
        assert_eq!(binding.step, 0.5);
        assert_eq!(bank.draft_text(key), Some("4.0"));

        bank.edit_text("project.runtime.limit.text", "-");
        bank.merge_tree(&numeric_tree(
            "project.other.value",
            2.0,
            0.0,
            8.0,
            1.0,
            "project-settings.commit_numeric",
        ));

        assert_eq!(bank.draft_text(key), Some("-"));
        assert!(matches!(
            bank.commit_for_submit("project-settings.commit_numeric:project.runtime.limit"),
            Some(UiNumericEditAction::Rejected {
                validation: UiNumericValidation::Incomplete,
                ..
            })
        ));
        assert!(bank.is_invalid(key));
    }

    #[test]
    fn bank_commit_is_transactional_and_cancel_uses_latest_authoritative_value() {
        let mut bank = UiNumericEditBank::default();
        bank.merge_tree(&UiNode::new("root", UiNodeKind::Root).with_children([
            numeric_tree(
                "settings.first",
                2.0,
                0.0,
                10.0,
                1.0,
                "settings.commit_numeric",
            ),
            numeric_tree(
                "settings.second",
                3.0,
                0.0,
                10.0,
                1.0,
                "settings.commit_numeric",
            ),
        ]));
        bank.edit_text("settings.first.text", "5");
        bank.edit_text("settings.second.text", "invalid");

        let issues = bank.commit_pending().expect_err("invalid pending field");
        assert_eq!(issues.len(), 1);
        assert!(bank
            .validation("settings.first")
            .is_some_and(|validation| validation == UiNumericValidation::Valid(5.0)));
        assert_eq!(bank.draft_text("settings.first"), Some("5"));

        bank.merge_tree(&numeric_tree(
            "settings.first",
            7.0,
            0.0,
            10.0,
            1.0,
            "settings.commit_numeric",
        ));
        assert_eq!(bank.draft_text("settings.first"), Some("5"));
        assert!(matches!(
            bank.cancel("settings.first"),
            Some(UiNumericEditAction::Cancelled {
                value: 7.0,
                restored_text,
                ..
            }) if restored_text == "7"
        ));
    }
}
