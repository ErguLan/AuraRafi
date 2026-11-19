//! Custom component authoring state machine.
//!
//! This module owns the draft, validation and persistence hand-off for the
//! Electronics component creator. It is a child of the controller so it can
//! reuse the live library, project root and UI invalidation without exposing a
//! second document model. Placement remains the existing library flow.

use std::path::{Path, PathBuf};

use glam::Vec2;
use raf_electronics::component::{
    ComponentAppearance, ElectronicComponent, Pin, PinDirection, SimModel, MAX_COMPONENT_CATEGORY,
    MAX_COMPONENT_DESIGNATOR, MAX_COMPONENT_FOOTPRINT, MAX_COMPONENT_NAME, MAX_COMPONENT_PINS,
    MAX_COMPONENT_VALUE,
};
use raf_electronics::library::{template_slug, ComponentLibrary};
use raf_electronics::symbol::{SymbolDefinition, SymbolPreset};
use uuid::Uuid;

use crate::electronics_controller::NativeElectronicsEditor;

/// Command that opens the component creator.
pub const COMPONENT_CREATOR_OPEN_COMMAND: &str = "electronics.component.create";
/// Command that closes the component creator without saving.
pub const COMPONENT_CREATOR_CANCEL_COMMAND: &str = "electronics.component.create.cancel";
/// Command that persists the current draft.
pub const COMPONENT_CREATOR_SAVE_COMMAND: &str = "electronics.component.create.save";
/// Prefix for a preset selection command.
pub const COMPONENT_CREATOR_PRESET_PREFIX: &str = "electronics.component.create.preset:";
/// Prefix for a field update command. Values are hex encoded bytes.
pub const COMPONENT_CREATOR_FIELD_PREFIX: &str = "electronics.component.create.field:";

/// Retained text-control key for the component display name.
pub const COMPONENT_CREATOR_NAME_VALUE_KEY: &str = "electronics.component_creator.value.name";
/// Retained text-control key for the category.
pub const COMPONENT_CREATOR_CATEGORY_VALUE_KEY: &str =
    "electronics.component_creator.value.category";
/// Retained text-control key for the designator prefix.
pub const COMPONENT_CREATOR_DESIGNATOR_VALUE_KEY: &str =
    "electronics.component_creator.value.designator";
/// Retained text-control key for the value.
pub const COMPONENT_CREATOR_VALUE_VALUE_KEY: &str = "electronics.component_creator.value.value";
/// Retained text-control key for the pin count.
pub const COMPONENT_CREATOR_PINS_VALUE_KEY: &str = "electronics.component_creator.value.pins";
/// Retained text-control key for the footprint.
pub const COMPONENT_CREATOR_FOOTPRINT_VALUE_KEY: &str =
    "electronics.component_creator.value.footprint";

/// Editable field of the component draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCreatorField {
    Name,
    Category,
    Designator,
    Value,
    Pins,
    Footprint,
}

impl ComponentCreatorField {
    /// Resolves the command/UI slug used by the host.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "name" => Some(Self::Name),
            "category" => Some(Self::Category),
            "designator" => Some(Self::Designator),
            "value" => Some(Self::Value),
            "pins" => Some(Self::Pins),
            "footprint" => Some(Self::Footprint),
            _ => None,
        }
    }

    /// Stable command slug for this field.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Category => "category",
            Self::Designator => "designator",
            Self::Value => "value",
            Self::Pins => "pins",
            Self::Footprint => "footprint",
        }
    }

    /// Retained text-control key seeded and read by the host.
    pub fn value_key(self) -> &'static str {
        match self {
            Self::Name => COMPONENT_CREATOR_NAME_VALUE_KEY,
            Self::Category => COMPONENT_CREATOR_CATEGORY_VALUE_KEY,
            Self::Designator => COMPONENT_CREATOR_DESIGNATOR_VALUE_KEY,
            Self::Value => COMPONENT_CREATOR_VALUE_VALUE_KEY,
            Self::Pins => COMPONENT_CREATOR_PINS_VALUE_KEY,
            Self::Footprint => COMPONENT_CREATOR_FOOTPRINT_VALUE_KEY,
        }
    }

    /// Resolves the field behind a retained text-control key.
    pub fn from_value_key(key: &str) -> Option<Self> {
        match key {
            "electronics.component_creator.value.name" => Some(Self::Name),
            "electronics.component_creator.value.category" => Some(Self::Category),
            "electronics.component_creator.value.designator" => Some(Self::Designator),
            "electronics.component_creator.value.value" => Some(Self::Value),
            "electronics.component_creator.value.pins" => Some(Self::Pins),
            "electronics.component_creator.value.footprint" => Some(Self::Footprint),
            _ => None,
        }
    }
}

/// Validation or persistence failure of the authoring flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCreatorError {
    NameRequired,
    Designator,
    Value,
    Pins,
    Invalid,
    Exists,
    ProjectMissing,
    SaveFailed,
    NotEditing,
}

impl ComponentCreatorError {
    /// Localized message key registered in both locale catalogs.
    pub fn message_key(self) -> &'static str {
        match self {
            Self::NameRequired => "electronics.component_creator.error.name_required",
            Self::Designator => "electronics.component_creator.error.designator",
            Self::Value => "electronics.component_creator.error.value",
            Self::Pins => "electronics.component_creator.error.pins",
            Self::Invalid => "electronics.component_creator.error.invalid",
            Self::Exists => "electronics.component_creator.error.exists",
            Self::ProjectMissing => "electronics.component_creator.error.project",
            Self::SaveFailed => "electronics.component_creator.error.save",
            Self::NotEditing => "electronics.component_creator.error.invalid",
        }
    }

    fn from_validation(message: &str) -> Self {
        if message.contains("designator") {
            Self::Designator
        } else if message.contains("value") {
            Self::Value
        } else if message.contains("pin") {
            Self::Pins
        } else {
            Self::Invalid
        }
    }
}

/// Creator lifecycle. `Saving` is observable only while the write runs; the
/// success path returns to `Editing` with the saved confirmation visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComponentCreatorPhase {
    #[default]
    Closed,
    Editing,
    Saving,
    Error,
}

/// Editable draft of one custom component.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentCreatorDraft {
    pub name: String,
    pub category: String,
    pub designator_prefix: String,
    pub value: String,
    pub pin_count: usize,
    pub preset: SymbolPreset,
    pub footprint: String,
}

impl Default for ComponentCreatorDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            category: String::new(),
            designator_prefix: "U".to_string(),
            value: String::new(),
            pin_count: 2,
            preset: SymbolPreset::Passive,
            footprint: String::new(),
        }
    }
}

impl ComponentCreatorDraft {
    /// Pin count the generated symbol and component will use.
    pub fn effective_pin_count(&self) -> usize {
        if self.preset == SymbolPreset::Ground {
            1
        } else {
            self.pin_count.clamp(1, MAX_COMPONENT_PINS)
        }
    }

    /// Authored symbol generated from the current preset and pin count.
    pub fn symbol(&self) -> SymbolDefinition {
        self.preset.symbol_for(self.effective_pin_count())
    }

    /// Pin offsets in schematic grid cells.
    pub fn pin_offsets(&self) -> Vec<Vec2> {
        self.preset.pin_offsets(self.effective_pin_count())
    }

    /// Applies one text field, truncating by characters rather than bytes.
    pub fn set_text(&mut self, field: ComponentCreatorField, value: &str) -> bool {
        let limit = match field {
            ComponentCreatorField::Name => MAX_COMPONENT_NAME,
            ComponentCreatorField::Category => MAX_COMPONENT_CATEGORY,
            ComponentCreatorField::Designator => MAX_COMPONENT_DESIGNATOR,
            ComponentCreatorField::Value => MAX_COMPONENT_VALUE,
            ComponentCreatorField::Footprint => MAX_COMPONENT_FOOTPRINT,
            ComponentCreatorField::Pins => return false,
        };
        let next = value.chars().take(limit).collect::<String>();
        let target = match field {
            ComponentCreatorField::Name => &mut self.name,
            ComponentCreatorField::Category => &mut self.category,
            ComponentCreatorField::Designator => &mut self.designator_prefix,
            ComponentCreatorField::Value => &mut self.value,
            ComponentCreatorField::Footprint => &mut self.footprint,
            ComponentCreatorField::Pins => return false,
        };
        if *target == next {
            return false;
        }
        *target = next;
        true
    }

    /// Applies the pin count field. Invalid input leaves the draft unchanged.
    pub fn set_pin_count(&mut self, value: &str) -> bool {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return false;
        }
        let Ok(count) = trimmed.parse::<usize>() else {
            return false;
        };
        let count = if self.preset == SymbolPreset::Ground {
            1
        } else {
            count
        };
        if !(1..=MAX_COMPONENT_PINS).contains(&count) {
            return false;
        }
        if self.pin_count == count {
            return false;
        }
        self.pin_count = count;
        true
    }

    /// Selects a preset. The ground reference only ever owns one pin.
    pub fn set_preset(&mut self, preset: SymbolPreset) -> bool {
        if self.preset == preset {
            return false;
        }
        let leaving_ground = self.preset == SymbolPreset::Ground;
        self.preset = preset;
        if preset == SymbolPreset::Ground {
            self.pin_count = 1;
        } else if leaving_ground && self.pin_count == 1 {
            // A ground reference owns one terminal. Restoring the default
            // two-terminal count keeps the editable field truthful when the
            // author switches back to a passive/box preset.
            self.pin_count = 2;
        }
        true
    }

    /// Builds the component that will be persisted as a template.
    pub fn build_component(&self) -> Result<ElectronicComponent, ComponentCreatorError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ComponentCreatorError::NameRequired);
        }
        let designator = self.designator_prefix.trim().to_ascii_uppercase();
        if designator.is_empty()
            || designator.chars().count() > MAX_COMPONENT_DESIGNATOR
            || !designator
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(ComponentCreatorError::Designator);
        }
        if self.value.chars().count() > MAX_COMPONENT_VALUE {
            return Err(ComponentCreatorError::Value);
        }
        if self.category.chars().count() > MAX_COMPONENT_CATEGORY {
            return Err(ComponentCreatorError::Invalid);
        }
        if self.footprint.chars().count() > MAX_COMPONENT_FOOTPRINT {
            return Err(ComponentCreatorError::Invalid);
        }

        let pin_count = self.effective_pin_count();
        if !(1..=MAX_COMPONENT_PINS).contains(&pin_count) {
            return Err(ComponentCreatorError::Pins);
        }
        let offsets = self.preset.pin_offsets(pin_count);
        if offsets.len() != pin_count {
            return Err(ComponentCreatorError::Pins);
        }
        let direction = if self.preset == SymbolPreset::Ground {
            PinDirection::Ground
        } else {
            PinDirection::Bidirectional
        };
        let pins = offsets
            .into_iter()
            .enumerate()
            .map(|(index, offset)| Pin {
                id: Uuid::new_v4(),
                name: (index + 1).to_string(),
                direction,
                offset,
                net: String::new(),
            })
            .collect::<Vec<_>>();

        let value = if self.value.trim().is_empty() {
            name.to_string()
        } else {
            self.value.trim().to_string()
        };
        let category = if self.category.trim().is_empty() {
            "Custom".to_string()
        } else {
            self.category.trim().to_string()
        };
        let component = ElectronicComponent {
            id: Uuid::new_v4(),
            designator,
            value,
            category,
            pins,
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: self.footprint.trim().to_string(),
            footprint_pin_map: Default::default(),
            symbol: Some(self.symbol()),
            library_name: name.to_string(),
            sim_model: SimModel::Unmodeled,
            datasheet: None,
            locked: false,
            visible: true,
            appearance: ComponentAppearance::default(),
        };
        component
            .validate_custom()
            .map_err(|message| ComponentCreatorError::from_validation(&message))?;
        Ok(component)
    }
}

/// Observable state of the creator panel.
#[derive(Debug, Clone, Default)]
pub struct ComponentCreatorState {
    pub phase: ComponentCreatorPhase,
    pub draft: ComponentCreatorDraft,
    pub error_key: Option<&'static str>,
    pub saved: bool,
    pub saved_path: Option<PathBuf>,
}

impl ComponentCreatorState {
    pub fn is_open(&self) -> bool {
        self.phase != ComponentCreatorPhase::Closed
    }

    pub fn is_saving(&self) -> bool {
        self.phase == ComponentCreatorPhase::Saving
    }

    /// Save is available while the draft is editable and has not just been
    /// persisted. Any edit clears the saved confirmation.
    pub fn can_save(&self) -> bool {
        matches!(
            self.phase,
            ComponentCreatorPhase::Editing | ComponentCreatorPhase::Error
        ) && !self.saved
            && self.error_key != Some("electronics.component_creator.error.pins")
    }

    /// Current draft values keyed by their retained text-control keys.
    ///
    /// The host writes these into `UiInteractionState` controls when the modal
    /// opens and after a programmatic draft update, so the fields always show
    /// the authoritative draft instead of a stale transient string.
    pub fn control_values(&self) -> [(&'static str, String); 6] {
        [
            (COMPONENT_CREATOR_NAME_VALUE_KEY, self.draft.name.clone()),
            (
                COMPONENT_CREATOR_CATEGORY_VALUE_KEY,
                self.draft.category.clone(),
            ),
            (
                COMPONENT_CREATOR_DESIGNATOR_VALUE_KEY,
                self.draft.designator_prefix.clone(),
            ),
            (COMPONENT_CREATOR_VALUE_VALUE_KEY, self.draft.value.clone()),
            (
                COMPONENT_CREATOR_PINS_VALUE_KEY,
                self.draft.effective_pin_count().to_string(),
            ),
            (
                COMPONENT_CREATOR_FOOTPRINT_VALUE_KEY,
                self.draft.footprint.clone(),
            ),
        ]
    }
}

impl NativeElectronicsEditor {
    /// Current creator state, used by the host and by the retained surface.
    pub fn component_creator(&self) -> &ComponentCreatorState {
        &self.component_creator
    }

    /// Opens the creator with a fresh draft. Reopening an open editor keeps the
    /// user's work instead of discarding it.
    pub fn begin_component_creation(&mut self) -> bool {
        if self.component_creator.is_open() {
            return false;
        }
        self.component_creator = ComponentCreatorState {
            phase: ComponentCreatorPhase::Editing,
            ..ComponentCreatorState::default()
        };
        self.touch_ui();
        true
    }

    /// Applies one text field of the draft.
    pub fn update_component_draft(&mut self, field: ComponentCreatorField, value: &str) -> bool {
        if !matches!(
            self.component_creator.phase,
            ComponentCreatorPhase::Editing | ComponentCreatorPhase::Error
        ) {
            return false;
        }
        let changed = if field == ComponentCreatorField::Pins {
            if self.component_creator.draft.preset == SymbolPreset::Ground {
                return false;
            }
            self.component_creator.draft.set_pin_count(value)
        } else {
            self.component_creator.draft.set_text(field, value)
        };
        if !changed && field == ComponentCreatorField::Pins {
            let trimmed = value.trim();
            let invalid = !trimmed.is_empty()
                && trimmed
                    .parse::<usize>()
                    .map_or(true, |count| !(1..=MAX_COMPONENT_PINS).contains(&count));
            if invalid {
                self.component_creator.phase = ComponentCreatorPhase::Error;
                self.component_creator.error_key = Some("electronics.component_creator.error.pins");
                self.component_creator.saved = false;
                self.touch_ui();
                return true;
            }
        }
        if changed {
            self.component_creator.phase = ComponentCreatorPhase::Editing;
            self.component_creator.error_key = None;
            self.component_creator.saved = false;
            self.touch_ui();
        }
        changed
    }

    /// Selects one of the generated symbol presets.
    pub fn set_component_draft_preset(&mut self, preset: SymbolPreset) -> bool {
        if !matches!(
            self.component_creator.phase,
            ComponentCreatorPhase::Editing | ComponentCreatorPhase::Error
        ) {
            return false;
        }
        if !self.component_creator.draft.set_preset(preset) {
            return false;
        }
        self.component_creator.phase = ComponentCreatorPhase::Editing;
        self.component_creator.error_key = None;
        self.component_creator.saved = false;
        self.touch_ui();
        true
    }

    /// Persists the draft into `<project_root>/ElectricalAssets/` and refreshes
    /// the library from disk. A successful save keeps the panel open with a
    /// visible confirmation; the next edit returns it to a normal draft.
    pub fn save_component_draft(&mut self, project_root: &Path) -> Result<PathBuf, String> {
        if !matches!(
            self.component_creator.phase,
            ComponentCreatorPhase::Editing | ComponentCreatorPhase::Error
        ) {
            return Err(ComponentCreatorError::NotEditing.message_key().to_string());
        }
        if self.component_creator.saved {
            if let Some(path) = self.component_creator.saved_path.clone() {
                return Ok(path);
            }
        }
        if self.component_creator.error_key == Some("electronics.component_creator.error.pins") {
            return Err(ComponentCreatorError::Pins.message_key().to_string());
        }
        if !project_root.is_dir() {
            self.component_creator.phase = ComponentCreatorPhase::Error;
            self.component_creator.error_key =
                Some(ComponentCreatorError::ProjectMissing.message_key());
            self.touch_ui();
            return Err(ComponentCreatorError::ProjectMissing
                .message_key()
                .to_string());
        }
        self.component_creator.phase = ComponentCreatorPhase::Saving;
        self.component_creator.error_key = None;
        self.touch_ui();

        let component = match self.component_creator.draft.build_component() {
            Ok(component) => component,
            Err(error) => {
                self.component_creator.phase = ComponentCreatorPhase::Error;
                self.component_creator.error_key = Some(error.message_key());
                self.component_creator.saved = false;
                self.touch_ui();
                return Err(error.message_key().to_string());
            }
        };

        match self.library.save_custom_template(
            project_root,
            &component,
            self.component_creator
                .saved_path
                .as_ref()
                .is_some_and(|path| {
                    let display_name = if !component.library_name.trim().is_empty() {
                        component.library_name.trim()
                    } else if !component.value.trim().is_empty() {
                        component.value.trim()
                    } else {
                        component.designator.trim()
                    };
                    let slug = template_slug(display_name);
                    path.file_stem().and_then(|stem| stem.to_str()) == slug.as_deref()
                }),
        ) {
            Ok(path) => {
                // A full rebuild from the built-in library removes stale
                // project entries and guarantees a refresh never stacks a
                // second copy of the same template.
                self.library = ComponentLibrary::default_library();
                self.library.load_external_assets_from(project_root);
                self.placement_template = None;
                let expected_name = component.library_name.trim();
                let reloaded = !expected_name.is_empty()
                    && self.library.components.iter().any(|template| {
                        template.source == raf_electronics::library::TemplateSource::Project
                            && template.name == expected_name
                    });
                if !reloaded {
                    self.component_creator.phase = ComponentCreatorPhase::Error;
                    self.component_creator.error_key =
                        Some(ComponentCreatorError::SaveFailed.message_key());
                    self.component_creator.saved = false;
                    self.touch_ui();
                    return Err("the component file could not be loaded back".to_string());
                }
                self.component_creator.phase = ComponentCreatorPhase::Editing;
                self.component_creator.error_key = None;
                self.component_creator.saved = true;
                self.component_creator.saved_path = Some(path.clone());
                self.touch_ui();
                Ok(path)
            }
            Err(message) => {
                let error = if message.contains("already exists")
                    || message.contains("collides with a built-in")
                {
                    ComponentCreatorError::Exists
                } else if message.contains("file name")
                    || message.contains("invalid component")
                    || message.contains("not valid")
                {
                    ComponentCreatorError::Invalid
                } else {
                    ComponentCreatorError::SaveFailed
                };
                self.component_creator.phase = ComponentCreatorPhase::Error;
                self.component_creator.error_key = Some(error.message_key());
                self.component_creator.saved = false;
                self.touch_ui();
                Err(message)
            }
        }
    }

    /// Saves against the project root the editor was mounted with.
    pub fn save_component_draft_in_project(&mut self) -> Result<PathBuf, String> {
        let Some(root) = self.footprint_project_root.clone() else {
            self.component_creator.phase = ComponentCreatorPhase::Error;
            self.component_creator.error_key =
                Some(ComponentCreatorError::ProjectMissing.message_key());
            self.touch_ui();
            return Err(ComponentCreatorError::ProjectMissing
                .message_key()
                .to_string());
        };
        self.save_component_draft(&root)
    }

    /// Closes the creator and discards the draft.
    pub fn cancel_component_creation(&mut self) -> bool {
        if !self.component_creator.is_open() {
            return false;
        }
        self.component_creator = ComponentCreatorState::default();
        self.touch_ui();
        true
    }

    /// Handles the semantic commands emitted by the creator surface and by the
    /// library header action. Returns true when the command belongs here.
    pub(crate) fn apply_component_creator_command(&mut self, command: &str) -> bool {
        if command == COMPONENT_CREATOR_OPEN_COMMAND {
            if self.component_creator.is_open() {
                return true;
            }
            return self.begin_component_creation();
        }
        if command == COMPONENT_CREATOR_CANCEL_COMMAND {
            if !self.component_creator.is_open() {
                return false;
            }
            self.cancel_component_creation();
            return true;
        }
        if command == COMPONENT_CREATOR_SAVE_COMMAND {
            if !self.component_creator.is_open() {
                return false;
            }
            if self.component_creator.can_save() {
                let _ = self.save_component_draft_in_project();
            }
            return true;
        }
        if let Some(slug) = command.strip_prefix(COMPONENT_CREATOR_PRESET_PREFIX) {
            let Some(preset) = preset_from_slug(slug) else {
                return false;
            };
            if !self.component_creator.is_open() {
                return false;
            }
            self.set_component_draft_preset(preset);
            return true;
        }
        if let Some(rest) = command.strip_prefix(COMPONENT_CREATOR_FIELD_PREFIX) {
            let Some((field, encoded)) = rest.split_once(':') else {
                return false;
            };
            let Some(field) = ComponentCreatorField::from_slug(field) else {
                return false;
            };
            if !self.component_creator.is_open() {
                return false;
            }
            let value = decode_hex(encoded).unwrap_or_else(|| encoded.to_string());
            self.update_component_draft(field, &value);
            return true;
        }
        false
    }
}

/// Symbol preset slug used by the segmented selector.
pub fn preset_slug(preset: SymbolPreset) -> &'static str {
    match preset {
        SymbolPreset::Passive => "passive",
        SymbolPreset::Box => "box",
        SymbolPreset::Ic => "ic",
        SymbolPreset::Connector => "connector",
        SymbolPreset::Ground => "ground",
    }
}

/// Resolves a preset slug emitted by the selector.
pub fn preset_from_slug(slug: &str) -> Option<SymbolPreset> {
    match slug {
        "passive" => Some(SymbolPreset::Passive),
        "box" => Some(SymbolPreset::Box),
        "ic" => Some(SymbolPreset::Ic),
        "connector" => Some(SymbolPreset::Connector),
        "ground" => Some(SymbolPreset::Ground),
        _ => None,
    }
}

fn decode_hex(value: &str) -> Option<String> {
    if value.is_empty() || value.len() % 2 != 0 || !value.is_ascii() {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let characters = value.as_bytes();
    let mut index = 0;
    while index < characters.len() {
        let high = (characters[index] as char).to_digit(16)?;
        let low = (characters[index + 1] as char).to_digit(16)?;
        bytes.push(((high << 4) | low) as u8);
        index += 2;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electronics_controller::NativeElectronicsEditor as CreatorEditor;
    use raf_electronics::Schematic;
    use std::env;

    fn creator() -> CreatorEditor {
        CreatorEditor::empty("Creator")
    }

    fn unique_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = env::temp_dir().join(format!(
            "rafi_component_creator_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&path).expect("temp project");
        path
    }

    #[test]
    fn draft_builds_a_bounded_unmodeled_component() {
        let mut draft = ComponentCreatorDraft::default();
        draft.name = "Op Amp".to_string();
        draft.category = "Analog".to_string();
        draft.designator_prefix = "U".to_string();
        draft.value = "LM358".to_string();
        draft.pin_count = 8;
        draft.preset = SymbolPreset::Ic;
        draft.footprint = "DIP-8".to_string();

        let component = draft.build_component().expect("valid custom component");
        assert_eq!(component.pins.len(), 8);
        assert_eq!(component.pins[0].name, "1");
        assert!(component.is_unmodeled());
        assert_eq!(component.library_name, "Op Amp");
        assert!(component.symbol.is_some());
        component.validate_custom().expect("custom contract");
        let _ = draft.symbol();
    }

    #[test]
    fn creator_survives_save_reload_and_schematic_round_trip() {
        let base = unique_dir("roundtrip");
        let mut editor = creator();
        assert!(editor.begin_component_creation());
        assert!(editor.update_component_draft(ComponentCreatorField::Name, "Op Amp"));
        assert_eq!(editor.component_creator().draft.designator_prefix, "U");
        assert!(editor.update_component_draft(ComponentCreatorField::Value, "LM358"));
        assert!(editor.update_component_draft(ComponentCreatorField::Pins, "8"));
        assert!(editor.set_component_draft_preset(SymbolPreset::Ic));
        assert!(editor.update_component_draft(ComponentCreatorField::Footprint, "DIP-8"));
        let seed = editor.component_creator().control_values();
        assert!(seed.iter().any(|(key, value)| {
            *key == COMPONENT_CREATOR_PINS_VALUE_KEY && value.as_str() == "8"
        }));

        let path = editor.save_component_draft(&base).expect("component saved");
        assert!(path.is_file());
        assert!(editor.component_creator().saved);
        let matches = editor
            .library()
            .components
            .iter()
            .filter(|template| template.name == "Op Amp")
            .count();
        assert_eq!(matches, 1);

        // Editing the same draft updates the file it just saved instead of
        // colliding with its own previous version.
        assert!(editor.update_component_draft(ComponentCreatorField::Value, "LM358N"));
        let updated = editor
            .save_component_draft(&base)
            .expect("saved component updated");
        assert_eq!(updated, path);
        let value = editor
            .library()
            .components
            .iter()
            .find(|template| template.name == "Op Amp")
            .map(|template| template.template.value.as_str());
        assert_eq!(value, Some("LM358N"));

        editor.refresh_library(&base);
        let matches = editor
            .library()
            .components
            .iter()
            .filter(|template| template.name == "Op Amp")
            .count();
        assert_eq!(matches, 1, "refresh must not duplicate project templates");

        let template = editor
            .library()
            .components
            .iter()
            .find(|template| template.name == "Op Amp")
            .expect("custom template");
        let mut schematic = Schematic::new("Round trip");
        schematic.add_component(template.instantiate());
        let serialized = ron::ser::to_string_pretty(&schematic, ron::ser::PrettyConfig::default())
            .expect("serialize schematic");
        let loaded = ron::from_str::<Schematic>(&serialized).expect("load schematic");
        let component = &loaded.components[0];
        assert_eq!(component.designator, "U1");
        assert!(component.symbol.is_some());
        assert!(component.is_unmodeled());
        component.validate_custom().expect("loaded custom part");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_built_in_name_is_reported_as_an_existing_component() {
        let base = unique_dir("exists");
        let mut editor = creator();
        assert!(editor.begin_component_creation());
        assert!(editor.update_component_draft(ComponentCreatorField::Name, "Resistor"));
        assert!(editor.save_component_draft(&base).is_err());
        assert_eq!(
            editor.component_creator().error_key,
            Some("electronics.component_creator.error.exists")
        );
        assert_eq!(
            editor.component_creator().phase,
            ComponentCreatorPhase::Error
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn text_and_pin_updates_clear_the_error_and_saved_state() {
        let base = unique_dir("state");
        let mut editor = creator();
        assert!(editor.begin_component_creation());
        assert!(editor.update_component_draft(ComponentCreatorField::Pins, "999"));
        assert_eq!(editor.component_creator().draft.pin_count, 2);
        assert_eq!(
            editor.component_creator().error_key,
            Some("electronics.component_creator.error.pins")
        );
        assert!(editor.update_component_draft(ComponentCreatorField::Pins, "256"));
        assert_eq!(
            editor.component_creator().draft.pin_count,
            MAX_COMPONENT_PINS
        );
        assert!(editor.update_component_draft(ComponentCreatorField::Pins, "not a number"));
        assert_eq!(
            editor.component_creator().error_key,
            Some("electronics.component_creator.error.pins")
        );
        assert!(editor.set_component_draft_preset(SymbolPreset::Ground));
        assert_eq!(editor.component_creator().draft.effective_pin_count(), 1);
        assert!(editor.update_component_draft(ComponentCreatorField::Name, "Ref"));
        assert!(editor.save_component_draft(&base).is_ok());
        assert!(editor.component_creator().saved);
        assert!(editor.update_component_draft(ComponentCreatorField::Value, "0V"));
        assert!(!editor.component_creator().saved);
        assert_eq!(
            editor.component_creator().phase,
            ComponentCreatorPhase::Editing
        );
        assert!(editor.cancel_component_creation());
        assert!(!editor.component_creator().is_open());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn saving_without_a_project_reports_a_project_error() {
        let missing = unique_dir("missing_project");
        std::fs::remove_dir_all(&missing).ok();
        let mut editor = creator();
        assert!(editor.begin_component_creation());
        assert!(editor.update_component_draft(ComponentCreatorField::Name, "Filter"));
        assert!(editor.save_component_draft(&missing).is_err());
        assert_eq!(
            editor.component_creator().error_key,
            Some("electronics.component_creator.error.project")
        );
        assert_eq!(
            editor.component_creator().phase,
            ComponentCreatorPhase::Error
        );
    }

    #[test]
    fn preset_slugs_round_trip() {
        for preset in [
            SymbolPreset::Passive,
            SymbolPreset::Box,
            SymbolPreset::Ic,
            SymbolPreset::Connector,
            SymbolPreset::Ground,
        ] {
            assert_eq!(preset_from_slug(preset_slug(preset)), Some(preset));
        }
        for field in [
            ComponentCreatorField::Name,
            ComponentCreatorField::Category,
            ComponentCreatorField::Designator,
            ComponentCreatorField::Value,
            ComponentCreatorField::Pins,
            ComponentCreatorField::Footprint,
        ] {
            assert_eq!(
                ComponentCreatorField::from_value_key(field.value_key()),
                Some(field)
            );
        }
    }

    #[test]
    fn hex_encoded_field_commands_update_the_draft() {
        let mut editor = creator();
        assert!(editor.apply_component_creator_command(COMPONENT_CREATOR_OPEN_COMMAND));
        let encoded = "4f 70 20 41 6d 70".replace(' ', "");
        assert!(editor.apply_component_creator_command(&format!(
            "{COMPONENT_CREATOR_FIELD_PREFIX}name:{encoded}"
        )));
        assert_eq!(editor.component_creator().draft.name, "Op Amp");
        assert!(!editor.apply_component_creator_command(&format!(
            "{COMPONENT_CREATOR_FIELD_PREFIX}unknown:value"
        )));
    }
}
