//! Stable structural fingerprint for Schematic and PcbLayout documents.
//!
//! Used by the editor history to detect document changes without paying the
//! cost of a full RON serialisation on every command. FNV-1a over every field
//! that affects logical state. Independent of pointer addresses so it stays
//! valid across editor restarts and across CPU/GPU paths.

use glam::Vec2;
use uuid::Uuid;

use crate::component::ElectronicComponent;
use crate::pcb::{BoardOutline, PcbAirwire, PcbComponentPlacement, PcbLayout, PcbTrace};
use crate::schematic::{Schematic, Wire, WireAnchor};

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy)]
struct Hasher(u64);

impl Hasher {
    fn new() -> Self {
        Self(OFFSET)
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_f32(&mut self, value: f32) {
        self.write_u32(value.to_bits());
    }

    fn write_vec2(&mut self, value: Vec2) {
        self.write_f32(value.x);
        self.write_f32(value.y);
    }

    fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        self.write_bytes(value.as_bytes());
    }

    fn write_option_str(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_str(value);
            }
            None => self.write_u8(0),
        }
    }

    fn write_uuid(&mut self, value: Uuid) {
        self.write_bytes(value.as_bytes());
    }

    fn write_anchor(&mut self, anchor: Option<WireAnchor>) {
        match anchor {
            Some(WireAnchor::Pin {
                component_id,
                pin_id,
            }) => {
                self.write_u8(1);
                self.write_uuid(component_id);
                self.write_uuid(pin_id);
            }
            Some(WireAnchor::Point(point)) => {
                self.write_u8(2);
                self.write_vec2(point);
            }
            None => self.write_u8(0),
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

pub fn schematic_fingerprint(schematic: &Schematic) -> u64 {
    let mut hasher = Hasher::new();
    hasher.write_str(&schematic.name);
    hasher.write_usize(schematic.components.len());
    for component in &schematic.components {
        write_component(&mut hasher, component);
    }
    hasher.write_usize(schematic.wires.len());
    for wire in &schematic.wires {
        write_wire(&mut hasher, wire);
    }
    hasher.finish()
}

pub fn pcb_fingerprint(layout: &PcbLayout) -> u64 {
    let mut hasher = Hasher::new();
    hasher.write_str(&layout.name);
    write_board_outline(&mut hasher, &layout.board_outline);
    hasher.write_usize(layout.components.len());
    for component in &layout.components {
        write_pcb_component(&mut hasher, component);
    }
    hasher.write_usize(layout.traces.len());
    for trace in &layout.traces {
        write_pcb_trace(&mut hasher, trace);
    }
    hasher.write_usize(layout.airwires.len());
    for airwire in &layout.airwires {
        write_pcb_airwire(&mut hasher, airwire);
    }
    hasher.finish()
}

fn write_component(hasher: &mut Hasher, component: &ElectronicComponent) {
    hasher.write_uuid(component.id);
    hasher.write_str(&component.designator);
    hasher.write_str(&component.value);
    hasher.write_str(&component.category);
    hasher.write_vec2(component.position);
    hasher.write_f32(component.rotation);
    hasher.write_str(&component.footprint);
    hasher.write_option_str(component.datasheet.as_deref());
    hasher.write_bool(component.locked);
    hasher.write_bool(component.visible);
    hasher.write_bytes(&component.appearance.color);
    hasher.write_str(&component.appearance.size);
    hasher.write_usize(component.pins.len());
    for pin in &component.pins {
        hasher.write_uuid(pin.id);
        hasher.write_str(&pin.name);
        hasher.write_u8(match pin.direction {
            crate::component::PinDirection::Input => 0,
            crate::component::PinDirection::Output => 1,
            crate::component::PinDirection::Bidirectional => 2,
            crate::component::PinDirection::Power => 3,
            crate::component::PinDirection::Ground => 4,
        });
        hasher.write_vec2(pin.offset);
        hasher.write_str(&pin.net);
    }
    match &component.sim_model {
        crate::component::SimModel::Resistor { ohms } => {
            hasher.write_u8(0);
            hasher.write_f32(*ohms as f32);
        }
        crate::component::SimModel::Capacitor { farads } => {
            hasher.write_u8(1);
            hasher.write_f32(*farads as f32);
        }
        crate::component::SimModel::Led { forward_voltage } => {
            hasher.write_u8(2);
            hasher.write_f32(*forward_voltage as f32);
        }
        crate::component::SimModel::Magnet { tesla, north_up } => {
            hasher.write_u8(3);
            hasher.write_f32(*tesla as f32);
            hasher.write_bool(*north_up);
        }
        crate::component::SimModel::Wire => hasher.write_u8(4),
        crate::component::SimModel::DcSource { voltage } => {
            hasher.write_u8(5);
            hasher.write_f32(*voltage as f32);
        }
    }
}

fn write_wire(hasher: &mut Hasher, wire: &Wire) {
    hasher.write_uuid(wire.id);
    hasher.write_vec2(wire.start);
    hasher.write_vec2(wire.end);
    hasher.write_str(&wire.net);
    hasher.write_anchor(wire.start_anchor);
    hasher.write_anchor(wire.end_anchor);
}

fn write_board_outline(hasher: &mut Hasher, outline: &BoardOutline) {
    hasher.write_usize(outline.points.len());
    for point in &outline.points {
        hasher.write_vec2(*point);
    }
}

fn write_pcb_component(hasher: &mut Hasher, component: &PcbComponentPlacement) {
    hasher.write_uuid(component.component_id);
    hasher.write_str(&component.designator);
    hasher.write_str(&component.value);
    hasher.write_str(&component.footprint);
    hasher.write_vec2(component.position);
    hasher.write_f32(component.rotation);
    hasher.write_u8(match component.layer {
        crate::pcb::PcbLayer::TopCopper => 0,
        crate::pcb::PcbLayer::BottomCopper => 1,
    });
    hasher.write_bool(component.locked);
    hasher.write_option_str(component.image_asset.as_deref());
    hasher.write_usize(component.pad_nets.len());
    for net in &component.pad_nets {
        hasher.write_str(net);
    }
}

fn write_pcb_trace(hasher: &mut Hasher, trace: &PcbTrace) {
    hasher.write_uuid(trace.id);
    hasher.write_str(&trace.net);
    hasher.write_u8(match trace.layer {
        crate::pcb::PcbLayer::TopCopper => 0,
        crate::pcb::PcbLayer::BottomCopper => 1,
    });
    hasher.write_f32(trace.width);
    hasher.write_usize(trace.points.len());
    for point in &trace.points {
        hasher.write_vec2(*point);
    }
}

fn write_pcb_airwire(hasher: &mut Hasher, airwire: &PcbAirwire) {
    hasher.write_str(&airwire.net);
    hasher.write_uuid(airwire.from_component_id);
    hasher.write_uuid(airwire.to_component_id);
    hasher.write_vec2(airwire.from);
    hasher.write_vec2(airwire.to);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;

    #[test]
    fn identical_schematics_share_fingerprint() {
        let mut a = Schematic::new("Test");
        let resistor = ElectronicComponent::resistor("10k");
        a.add_component(resistor.clone());
        let mut b = Schematic::new("Test");
        b.add_component(resistor);
        assert_eq!(schematic_fingerprint(&a), schematic_fingerprint(&b));
    }

    #[test]
    fn edited_value_changes_fingerprint() {
        let mut a = Schematic::new("Test");
        a.add_component(ElectronicComponent::resistor("10k"));
        let mut b = Schematic::new("Test");
        b.add_component(ElectronicComponent::resistor("22k"));
        assert_ne!(schematic_fingerprint(&a), schematic_fingerprint(&b));
    }

    #[test]
    fn position_change_changes_fingerprint() {
        let mut a = Schematic::new("Test");
        let mut r1 = ElectronicComponent::resistor("10k");
        r1.position = Vec2::new(0.0, 0.0);
        a.add_component(r1);

        let mut b = Schematic::new("Test");
        let mut r2 = ElectronicComponent::resistor("10k");
        r2.position = Vec2::new(40.0, 0.0);
        b.add_component(r2);

        assert_ne!(schematic_fingerprint(&a), schematic_fingerprint(&b));
    }

    #[test]
    fn empty_pcb_layout_fingerprint_is_deterministic() {
        let a = PcbLayout::new("Board");
        let b = PcbLayout::new("Board");
        assert_eq!(pcb_fingerprint(&a), pcb_fingerprint(&b));
    }
}
