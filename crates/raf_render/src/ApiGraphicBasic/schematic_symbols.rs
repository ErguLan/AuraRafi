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

const GENERIC_FILLED_CIRCLES: &[SymbolCircle] = &[];
const GENERIC_OPEN_CIRCLES: &[SymbolCircle] = &[];

pub fn schematic_symbol_recipe(kind: SchematicSymbolKind) -> SchematicSymbolRecipe {
    match kind {
        SchematicSymbolKind::Generic => SchematicSymbolRecipe {
            half_size: [28.0, 14.0],
            segments: GENERIC_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
        SchematicSymbolKind::Resistor => SchematicSymbolRecipe {
            half_size: [28.0, 12.0],
            segments: RESISTOR_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
        SchematicSymbolKind::Capacitor => SchematicSymbolRecipe {
            half_size: [24.0, 14.0],
            segments: CAPACITOR_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
        SchematicSymbolKind::Led => SchematicSymbolRecipe {
            half_size: [24.0, 14.0],
            segments: LED_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
        SchematicSymbolKind::Battery => SchematicSymbolRecipe {
            half_size: [24.0, 14.0],
            segments: BATTERY_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
        SchematicSymbolKind::Ground => SchematicSymbolRecipe {
            half_size: [14.0, 20.0],
            segments: GROUND_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
        SchematicSymbolKind::Magnet => SchematicSymbolRecipe {
            half_size: [26.0, 14.0],
            segments: MAGNET_SEGMENTS,
            open_circles: GENERIC_OPEN_CIRCLES,
            filled_circles: GENERIC_FILLED_CIRCLES,
        },
    }
}