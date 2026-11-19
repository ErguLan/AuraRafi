//! Electronic component definition.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::symbol::SymbolDefinition;

/// Longest accepted designator on a custom component.
pub const MAX_COMPONENT_DESIGNATOR: usize = 16;
/// Longest accepted value/part name on a custom component.
pub const MAX_COMPONENT_VALUE: usize = 64;
/// Longest accepted category on a custom component.
pub const MAX_COMPONENT_CATEGORY: usize = 64;
/// Longest accepted library display name on a custom component.
pub const MAX_COMPONENT_NAME: usize = 64;
/// Largest accepted pin count on a custom component.
pub const MAX_COMPONENT_PINS: usize = 256;
/// Longest accepted footprint reference on a custom component.
pub const MAX_COMPONENT_FOOTPRINT: usize = 128;
/// Longest accepted pin name on a custom component.
pub const MAX_COMPONENT_PIN_NAME: usize = 64;
/// Longest accepted datasheet reference on a custom component.
pub const MAX_COMPONENT_DATASHEET: usize = 512;
/// Longest accepted appearance size label on a custom component.
pub const MAX_COMPONENT_APPEARANCE_SIZE: usize = 32;
/// Largest absolute pin offset accepted on a custom component.
pub const MAX_COMPONENT_OFFSET: f32 = 10_000.0;

fn default_visible() -> bool {
    true
}

/// Pin direction for connection semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinDirection {
    Input,
    Output,
    Bidirectional,
    Power,
    Ground,
}

/// Simulation model for a component.
///
/// Defines how the simulation engine treats each component type.
/// Only covers the 3 core components for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimModel {
    /// Resistor with resistance in ohms.
    Resistor { ohms: f64 },
    /// Capacitor with capacitance in farads (DC steady-state = open circuit).
    Capacitor { farads: f64 },
    /// LED with a fixed forward voltage drop.
    Led { forward_voltage: f64 },
    /// Magnet with field strength in Tesla and polarity.
    /// Induces EMF in nearby conductors proportional to field strength.
    Magnet {
        /// Field strength in Tesla (e.g. 0.5 for a small neodymium).
        tesla: f64,
        /// Polarity: true = North facing up, false = South facing up.
        north_up: bool,
    },
    /// Simple wire / passthrough (zero resistance).
    Wire,
    /// DC Voltage Source (ideal battery).
    DcSource { voltage: f64 },
    /// Custom part with no electrical model in the DC solver.
    ///
    /// The component keeps every pin connected in the netlist, but it never
    /// contributes conductance. Treating it as a wire would short nets, so the
    /// solver skips it and reports a bounded diagnostic instead.
    Unmodeled,
}

impl Default for SimModel {
    fn default() -> Self {
        SimModel::Wire
    }
}

/// A pin on an electronic component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    pub id: Uuid,
    pub name: String,
    pub direction: PinDirection,
    /// Offset from component origin on the schematic grid.
    pub offset: Vec2,
    /// Net name this pin is connected to (or empty).
    pub net: String,
}

/// User-facing visual metadata for schematic/PCB inspectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentAppearance {
    pub color: [u8; 4],
    pub size: String,
}

impl Default for ComponentAppearance {
    fn default() -> Self {
        Self {
            color: [92, 214, 142, 255],
            size: "Normal".to_string(),
        }
    }
}

/// An electronic component (resistor, IC, transistor, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectronicComponent {
    pub id: Uuid,
    /// Component designator (e.g. "R1", "U3").
    pub designator: String,
    /// Part name / value (e.g. "10k", "ATmega328P").
    pub value: String,
    /// Category for library browsing.
    pub category: String,
    /// Pins.
    pub pins: Vec<Pin>,
    /// Position on the schematic grid.
    pub position: Vec2,
    /// Rotation in degrees (0, 90, 180, 270).
    pub rotation: f32,
    /// Footprint reference for PCB (e.g. "0805", "DIP-28").
    pub footprint: String,
    /// Explicit logical pin name -> physical pad number assignment. An empty
    /// map preserves the legacy exact-name convention.
    #[serde(default)]
    pub footprint_pin_map: std::collections::BTreeMap<String, String>,
    /// Authored schematic symbol. `None` keeps the built-in look driven by the
    /// simulation model, so legacy templates and the six built-ins render
    /// exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SymbolDefinition>,
    /// Optional display name used by data-driven library entries. Empty for
    /// built-ins and for legacy templates, whose file stem supplies the name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub library_name: String,
    /// Simulation model for this component.
    pub sim_model: SimModel,
    /// Optional datasheet URL or project-relative path.
    #[serde(default)]
    pub datasheet: Option<String>,
    /// Locks placement in schematic/PCB editing.
    #[serde(default)]
    pub locked: bool,
    /// Visibility toggle for premium inspector workflows.
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Visual metadata used by the editor.
    #[serde(default)]
    pub appearance: ComponentAppearance,
}

/// Parse a human-readable value string into ohms.
///
/// Supports suffixes: k (kilo), M (mega), m (milli), u (micro).
/// Examples: "10k" -> 10_000.0, "4.7M" -> 4_700_000.0, "100" -> 100.0
pub fn parse_resistance(value: &str) -> f64 {
    parse_si_value(value).unwrap_or(1000.0)
}

/// Parse a human-readable capacitance string into farads.
///
/// Supports suffixes: u (micro), n (nano), p (pico), F (ignored).
/// Examples: "100nF" -> 100e-9, "10u" -> 10e-6, "22p" -> 22e-12
pub fn parse_capacitance(value: &str) -> f64 {
    parse_si_value(value).unwrap_or(100.0e-9)
}

/// Parse a human-readable voltage string into volts.
pub fn parse_voltage(value: &str) -> f64 {
    let cleaned = value.trim().trim_end_matches(['V', 'v']);
    cleaned.parse::<f64>().unwrap_or(5.0)
}

/// Parse a value string with SI suffix into a numeric value.
fn parse_si_value(value: &str) -> Option<f64> {
    let s = value.trim().replace('F', "");
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Find where the numeric part ends.
    let mut num_end = s.len();
    for (i, ch) in s.char_indices().rev() {
        if ch.is_ascii_digit() || ch == '.' {
            num_end = i + ch.len_utf8();
            break;
        }
    }

    // If the entire string is a suffix with no digits, try treating
    // the suffix as the last char only.
    if num_end == 0 {
        return None;
    }

    let num_part = &s[..num_end];
    let suffix = s[num_end..].trim();

    let base: f64 = num_part.parse().ok()?;

    let multiplier = match suffix {
        "M" => 1_000_000.0,
        "k" | "K" => 1_000.0,
        "m" => 0.001,
        "u" | "uF" => 1e-6,
        "n" | "nF" => 1e-9,
        "p" | "pF" => 1e-12,
        "" => 1.0,
        _ => 1.0,
    };

    Some(base * multiplier)
}

impl ElectronicComponent {
    /// Human-readable component kind derived from its simulation model.
    pub fn kind_label(&self) -> &'static str {
        match self.sim_model {
            SimModel::Resistor { .. } => "Resistor",
            SimModel::Capacitor { .. } => "Capacitor",
            SimModel::Led { .. } => "LED",
            SimModel::Magnet { .. } => "Magnet",
            SimModel::Wire => "Ground",
            SimModel::DcSource { .. } => "Battery",
            SimModel::Unmodeled => "Custom",
        }
    }

    /// Keep the simulation model in sync after editing the human-readable value.
    pub fn sync_sim_model_from_value(&mut self) {
        match &mut self.sim_model {
            SimModel::Resistor { ohms } => {
                *ohms = parse_resistance(&self.value);
            }
            SimModel::Capacitor { farads } => {
                *farads = parse_capacitance(&self.value);
            }
            SimModel::Magnet { tesla, .. } => {
                *tesla = parse_magnet_strength(&self.value);
            }
            SimModel::DcSource { voltage } => {
                *voltage = parse_voltage(&self.value);
            }
            SimModel::Led { .. } | SimModel::Wire | SimModel::Unmodeled => {}
        }
    }

    /// Create a basic resistor.
    pub fn resistor(value: &str) -> Self {
        let ohms = parse_resistance(value);
        Self {
            id: Uuid::new_v4(),
            designator: "R?".to_string(),
            value: value.to_string(),
            category: "Passive".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "1".to_string(),
                    direction: PinDirection::Bidirectional,
                    offset: Vec2::new(-1.0, 0.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "2".to_string(),
                    direction: PinDirection::Bidirectional,
                    offset: Vec2::new(1.0, 0.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "0805".to_string(),
            footprint_pin_map: Default::default(),
            symbol: None,
            library_name: String::new(),
            sim_model: SimModel::Resistor { ohms },
            datasheet: None,
            locked: false,
            visible: true,
            appearance: ComponentAppearance::default(),
        }
    }

    /// Create a basic capacitor.
    pub fn capacitor(value: &str) -> Self {
        let farads = parse_capacitance(value);
        let mut comp = Self::resistor(value);
        comp.designator = "C?".to_string();
        comp.category = "Passive".to_string();
        comp.sim_model = SimModel::Capacitor { farads };
        comp
    }

    /// Create a basic LED.
    pub fn led() -> Self {
        Self {
            id: Uuid::new_v4(),
            designator: "D?".to_string(),
            value: "LED".to_string(),
            category: "Diode".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "A".to_string(),
                    direction: PinDirection::Input,
                    offset: Vec2::new(-1.0, 0.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "K".to_string(),
                    direction: PinDirection::Output,
                    offset: Vec2::new(1.0, 0.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "0805".to_string(),
            footprint_pin_map: Default::default(),
            symbol: None,
            library_name: String::new(),
            sim_model: SimModel::Led {
                forward_voltage: 2.0,
            },
            datasheet: None,
            locked: false,
            visible: true,
            appearance: ComponentAppearance {
                color: [255, 142, 56, 255],
                size: "Normal".to_string(),
            },
        }
    }

    /// Create a magnet component.
    ///
    /// Magnets have North (N) and South (S) poles as pins.
    /// The field strength is specified as a human-readable label
    /// (e.g. "0.5T", "1T", "Weak", "Strong").
    /// They induce EMF in nearby conductors and attract/repel
    /// other magnets based on polarity.
    pub fn magnet(strength: &str) -> Self {
        let tesla = parse_magnet_strength(strength);
        Self {
            id: Uuid::new_v4(),
            designator: "MAG?".to_string(),
            value: strength.to_string(),
            category: "Magnet".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "N".to_string(),
                    direction: PinDirection::Output,
                    offset: Vec2::new(0.0, -1.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "S".to_string(),
                    direction: PinDirection::Input,
                    offset: Vec2::new(0.0, 1.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "MAG-10x5".to_string(),
            footprint_pin_map: Default::default(),
            symbol: None,
            library_name: String::new(),
            sim_model: SimModel::Magnet {
                tesla,
                north_up: true,
            },
            datasheet: None,
            locked: false,
            visible: true,
            appearance: ComponentAppearance {
                color: [82, 178, 224, 255],
                size: "Normal".to_string(),
            },
        }
    }

    /// Create a DC Source (Battery).
    pub fn dc_source(voltage: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            designator: "V?".to_string(),
            value: format!("{}V", voltage),
            category: "Power".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "+".to_string(),
                    direction: PinDirection::Power,
                    offset: Vec2::new(0.0, -1.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "-".to_string(),
                    direction: PinDirection::Power,
                    offset: Vec2::new(0.0, 1.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "BAT-18650".to_string(),
            footprint_pin_map: Default::default(),
            symbol: None,
            library_name: String::new(),
            sim_model: SimModel::DcSource { voltage },
            datasheet: None,
            locked: false,
            visible: true,
            appearance: ComponentAppearance {
                color: [255, 145, 35, 255],
                size: "Normal".to_string(),
            },
        }
    }

    /// Create a Ground terminal.
    pub fn ground() -> Self {
        Self {
            id: Uuid::new_v4(),
            designator: "GND".to_string(),
            value: "GND".to_string(),
            category: "Power".to_string(),
            pins: vec![Pin {
                id: Uuid::new_v4(),
                name: "GND".to_string(),
                direction: PinDirection::Ground,
                offset: Vec2::new(0.0, 0.0),
                net: String::new(),
            }],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "TP-GND".to_string(),
            footprint_pin_map: Default::default(),
            symbol: None,
            library_name: String::new(),
            sim_model: SimModel::Wire, // electrically transparent
            datasheet: None,
            locked: false,
            visible: true,
            appearance: ComponentAppearance {
                color: [112, 220, 140, 255],
                size: "Normal".to_string(),
            },
        }
    }

    /// Validates a data-driven custom component before it is persisted.
    ///
    /// Built-in constructors are intentionally not required to pass this gate
    /// (`R?`, `D?` and `GND` keep their historic labels); only a component the
    /// author created through the component authoring flow is compared against
    /// the stricter custom contract.
    pub fn validate_custom(&self) -> Result<(), String> {
        let designator = self.designator.trim();
        let designator_len = designator.chars().count();
        if designator_len == 0 || designator_len > MAX_COMPONENT_DESIGNATOR {
            return Err(format!(
                "designator must use 1 to {MAX_COMPONENT_DESIGNATOR} characters"
            ));
        }
        if !designator
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        {
            return Err("designator must use ASCII letters or digits only".to_string());
        }
        if self.value.chars().count() > MAX_COMPONENT_VALUE {
            return Err(format!(
                "value is limited to {MAX_COMPONENT_VALUE} characters"
            ));
        }
        if self.category.chars().count() > MAX_COMPONENT_CATEGORY {
            return Err(format!(
                "category is limited to {MAX_COMPONENT_CATEGORY} characters"
            ));
        }
        if !self.library_name.is_empty() && self.library_name.chars().count() > MAX_COMPONENT_NAME {
            return Err(format!(
                "library name is limited to {MAX_COMPONENT_NAME} characters"
            ));
        }
        if self.pins.is_empty() || self.pins.len() > MAX_COMPONENT_PINS {
            return Err(format!("pins must stay between 1 and {MAX_COMPONENT_PINS}"));
        }
        let mut names = std::collections::HashSet::with_capacity(self.pins.len());
        for pin in &self.pins {
            let name = pin.name.trim();
            if name.is_empty() || name.chars().count() > MAX_COMPONENT_PIN_NAME {
                return Err(format!(
                    "pin names must use 1 to {MAX_COMPONENT_PIN_NAME} characters"
                ));
            }
            if !names.insert(name) {
                return Err(format!("duplicate pin name '{name}'"));
            }
            if !pin.offset.is_finite()
                || pin.offset.x.abs() > MAX_COMPONENT_OFFSET
                || pin.offset.y.abs() > MAX_COMPONENT_OFFSET
            {
                return Err("pin offsets must be finite and inside the schematic grid".to_string());
            }
            if pin.net.chars().count() > MAX_COMPONENT_PIN_NAME {
                return Err("pin net labels are too long".to_string());
            }
        }
        if self.footprint.chars().count() > MAX_COMPONENT_FOOTPRINT {
            return Err(format!(
                "footprint is limited to {MAX_COMPONENT_FOOTPRINT} characters"
            ));
        }
        if self.footprint_pin_map.len() > MAX_COMPONENT_PINS {
            return Err("the footprint pin map is too large".to_string());
        }
        if self.footprint_pin_map.iter().any(|(logical, physical)| {
            logical.chars().count() > MAX_COMPONENT_PIN_NAME
                || physical.chars().count() > MAX_COMPONENT_PIN_NAME
        }) {
            return Err("a footprint pin mapping is too long".to_string());
        }
        if self
            .datasheet
            .as_ref()
            .is_some_and(|datasheet| datasheet.chars().count() > MAX_COMPONENT_DATASHEET)
        {
            return Err(format!(
                "datasheet reference is limited to {MAX_COMPONENT_DATASHEET} characters"
            ));
        }
        if self.appearance.size.chars().count() > MAX_COMPONENT_APPEARANCE_SIZE {
            return Err("appearance size label is too long".to_string());
        }
        if let Some(symbol) = &self.symbol {
            symbol.validate()?;
        }
        Ok(())
    }

    /// True when this component owns authored vector ink that the scene accepts.
    pub fn has_authored_symbol(&self) -> bool {
        self.symbol
            .as_ref()
            .is_some_and(|symbol| symbol.validate().is_ok())
    }

    /// True when the DC solver has no electrical model for this part.
    pub fn is_unmodeled(&self) -> bool {
        matches!(self.sim_model, SimModel::Unmodeled)
    }
}

/// Parse a magnet strength string into Tesla.
/// Supports: "0.5T", "1T", "Weak" (0.1T), "Strong" (1.0T), "Neodymium" (0.5T).
fn parse_magnet_strength(value: &str) -> f64 {
    let s = value.trim().to_lowercase();
    match s.as_str() {
        "weak" => 0.1,
        "medium" => 0.3,
        "strong" => 1.0,
        "neodymium" => 0.5,
        _ => {
            // Try parsing as number with optional T suffix.
            let cleaned = s.replace('t', "");
            cleaned.parse::<f64>().unwrap_or(0.3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resistance_values() {
        assert!((parse_resistance("10k") - 10_000.0).abs() < 0.1);
        assert!((parse_resistance("4.7k") - 4_700.0).abs() < 0.1);
        assert!((parse_resistance("1M") - 1_000_000.0).abs() < 0.1);
        assert!((parse_resistance("100") - 100.0).abs() < 0.1);
        assert!((parse_resistance("470") - 470.0).abs() < 0.1);
    }

    #[test]
    fn parse_capacitance_values() {
        assert!((parse_capacitance("100nF") - 100e-9).abs() < 1e-12);
        assert!((parse_capacitance("10u") - 10e-6).abs() < 1e-9);
        assert!((parse_capacitance("22p") - 22e-12).abs() < 1e-15);
    }

    #[test]
    fn parse_voltage_values() {
        assert!((parse_voltage("9V") - 9.0).abs() < 0.0001);
        assert!((parse_voltage("3.3") - 3.3).abs() < 0.0001);
    }

    #[test]
    fn resistor_has_sim_model() {
        let r = ElectronicComponent::resistor("10k");
        match r.sim_model {
            SimModel::Resistor { ohms } => assert!((ohms - 10_000.0).abs() < 0.1),
            _ => panic!("Expected Resistor sim model"),
        }
    }

    #[test]
    fn capacitor_has_sim_model() {
        let c = ElectronicComponent::capacitor("100nF");
        match c.sim_model {
            SimModel::Capacitor { farads } => assert!((farads - 100e-9).abs() < 1e-12),
            _ => panic!("Expected Capacitor sim model"),
        }
    }

    #[test]
    fn led_has_sim_model() {
        let l = ElectronicComponent::led();
        match l.sim_model {
            SimModel::Led { forward_voltage } => assert!((forward_voltage - 2.0).abs() < 0.01),
            _ => panic!("Expected Led sim model"),
        }
    }

    #[test]
    fn edited_value_updates_sim_model() {
        let mut r = ElectronicComponent::resistor("10k");
        r.value = "1k".to_string();
        r.sync_sim_model_from_value();

        match r.sim_model {
            SimModel::Resistor { ohms } => assert!((ohms - 1_000.0).abs() < 0.1),
            _ => panic!("Expected Resistor sim model"),
        }
    }

    #[test]
    fn custom_validation_rejects_unsafe_identifiers_and_accepts_a_generated_symbol() {
        let mut custom = ElectronicComponent::resistor("10k");
        custom.designator = "U?".to_string();
        assert!(custom.validate_custom().is_err());

        custom.designator = "U".to_string();
        custom.symbol = Some(crate::symbol::SymbolPreset::Ic.symbol_for(2));
        custom.sim_model = SimModel::Unmodeled;
        custom.validate_custom().expect("valid custom part");
        assert!(custom.is_unmodeled());
        assert!(custom.has_authored_symbol());

        custom.pins[1].name = custom.pins[0].name.clone();
        assert!(custom.validate_custom().is_err());
    }
}
