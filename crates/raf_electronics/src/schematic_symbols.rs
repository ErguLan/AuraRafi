//! Backend-neutral schematic symbol recipes.
//!
//! Symbol meaning belongs to the Electronics domain. Renderers consume these
//! small immutable line recipes but do not define their own electrical shape
//! catalogue.

use crate::component::{ElectronicComponent, SimModel};

pub type SymbolSegment = [[f32; 2]; 2];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymbolCircle {
    pub center: [f32; 2],
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchematicSymbolKind {
    Generic,
    Resistor,
    Capacitor,
    Led,
    Battery,
    Ground,
    Magnet,
}

#[derive(Debug, Clone, Copy)]
pub struct SchematicSymbolRecipe {
    pub half_size: [f32; 2],
    pub segments: &'static [SymbolSegment],
    pub open_circles: &'static [SymbolCircle],
    pub filled_circles: &'static [SymbolCircle],
}

pub fn symbol_kind_for_component(component: &ElectronicComponent) -> SchematicSymbolKind {
    match component.sim_model {
        SimModel::Resistor { .. } => SchematicSymbolKind::Resistor,
        SimModel::Capacitor { .. } => SchematicSymbolKind::Capacitor,
        SimModel::Led { .. } => SchematicSymbolKind::Led,
        SimModel::Magnet { .. } => SchematicSymbolKind::Magnet,
        SimModel::DcSource { .. } => SchematicSymbolKind::Battery,
        SimModel::Wire if component.designator.eq_ignore_ascii_case("GND") => {
            SchematicSymbolKind::Ground
        }
        _ => SchematicSymbolKind::Generic,
    }
}

const GENERIC_SEGMENTS: &[SymbolSegment] = &[
    [[-24.0, 0.0], [-12.0, 0.0]],
    [[-12.0, -10.0], [12.0, -10.0]],
    [[12.0, -10.0], [12.0, 10.0]],
    [[12.0, 10.0], [-12.0, 10.0]],
    [[-12.0, 10.0], [-12.0, -10.0]],
    [[12.0, 0.0], [24.0, 0.0]],
];

const RESISTOR_SEGMENTS: &[SymbolSegment] = &[
    [[-26.0, 0.0], [-16.0, 0.0]],
    [[-16.0, 0.0], [-10.0, -6.0]],
    [[-10.0, -6.0], [-4.0, 6.0]],
    [[-4.0, 6.0], [2.0, -6.0]],
    [[2.0, -6.0], [8.0, 6.0]],
    [[8.0, 6.0], [14.0, -6.0]],
    [[14.0, -6.0], [20.0, 0.0]],
    [[20.0, 0.0], [26.0, 0.0]],
];

const CAPACITOR_SEGMENTS: &[SymbolSegment] = &[
    [[-24.0, 0.0], [-8.0, 0.0]],
    [[-8.0, -12.0], [-8.0, 12.0]],
    [[8.0, -12.0], [8.0, 12.0]],
    [[8.0, 0.0], [24.0, 0.0]],
];

const LED_SEGMENTS: &[SymbolSegment] = &[
    [[-24.0, 0.0], [-10.0, 0.0]],
    [[-10.0, -10.0], [6.0, 0.0]],
    [[-10.0, 10.0], [6.0, 0.0]],
    [[-10.0, -10.0], [-10.0, 10.0]],
    [[10.0, -10.0], [10.0, 10.0]],
    [[10.0, 0.0], [24.0, 0.0]],
    [[14.0, -6.0], [20.0, -12.0]],
    [[17.0, -4.0], [20.0, -12.0]],
    [[14.0, 6.0], [20.0, 0.0]],
    [[17.0, 8.0], [20.0, 0.0]],
];

const BATTERY_SEGMENTS: &[SymbolSegment] = &[
    [[-24.0, 0.0], [-8.0, 0.0]],
    [[-8.0, -12.0], [-8.0, 12.0]],
    [[4.0, -8.0], [4.0, 8.0]],
    [[4.0, 0.0], [24.0, 0.0]],
    [[-14.0, -4.0], [-14.0, 4.0]],
    [[0.0, -4.0], [0.0, 4.0]],
];

const GROUND_SEGMENTS: &[SymbolSegment] = &[
    [[0.0, -20.0], [0.0, -6.0]],
    [[-12.0, -6.0], [12.0, -6.0]],
    [[-8.0, 0.0], [8.0, 0.0]],
    [[-4.0, 6.0], [4.0, 6.0]],
];

const MAGNET_SEGMENTS: &[SymbolSegment] = &[
    [[-24.0, 0.0], [-14.0, 0.0]],
    [[14.0, 0.0], [24.0, 0.0]],
    [[-14.0, -12.0], [14.0, -12.0]],
    [[14.0, -12.0], [14.0, 12.0]],
    [[14.0, 12.0], [-14.0, 12.0]],
    [[-14.0, 12.0], [-14.0, -12.0]],
    [[0.0, -12.0], [0.0, 12.0]],
    [[-8.0, -12.0], [-8.0, 12.0]],
    [[8.0, -12.0], [8.0, 12.0]],
];

const EMPTY_CIRCLES: &[SymbolCircle] = &[];

pub fn schematic_symbol_recipe(kind: SchematicSymbolKind) -> SchematicSymbolRecipe {
    let (half_size, segments) = match kind {
        SchematicSymbolKind::Generic => ([28.0, 14.0], GENERIC_SEGMENTS),
        SchematicSymbolKind::Resistor => ([28.0, 12.0], RESISTOR_SEGMENTS),
        SchematicSymbolKind::Capacitor => ([24.0, 14.0], CAPACITOR_SEGMENTS),
        SchematicSymbolKind::Led => ([24.0, 14.0], LED_SEGMENTS),
        SchematicSymbolKind::Battery => ([24.0, 14.0], BATTERY_SEGMENTS),
        SchematicSymbolKind::Ground => ([14.0, 20.0], GROUND_SEGMENTS),
        SchematicSymbolKind::Magnet => ([26.0, 14.0], MAGNET_SEGMENTS),
    };
    SchematicSymbolRecipe {
        half_size,
        segments,
        open_circles: EMPTY_CIRCLES,
        filled_circles: EMPTY_CIRCLES,
    }
}
