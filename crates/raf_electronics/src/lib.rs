//! # raf_electronics
//!
//! Electronic design subsystem: schematic editor, PCB layout,
//! simulation, DRC, export, and component library for AuraRafi.

pub mod component;
pub mod drc;
pub mod extensions;
pub mod export;
pub mod library;
pub mod netlist;
pub mod pcb;
pub mod schematic;
pub mod schematic_graph;
pub mod simulation;

pub use component::{ElectronicComponent, PinDirection, SimModel};
pub use drc::{run_drc, DrcReport, DrcIssue, DrcSeverity};
pub use extensions::{
    register_component_template,
    register_drc_rule,
    registered_extension_summary,
    ElectricalExtensionRegistry,
    ElectricalExtensionSummary,
    ElectricalRule,
};
pub use export::{
    export_bom_csv, export_netlist_text, export_svg,
    export_gerber_layout_stub, export_gerber_stub, share_circuit, load_shared_circuit,
    ExportFormat, ExportResult, GerberTarget,
};
pub use library::{ComponentLibrary, ComponentTemplate};
pub use netlist::Netlist;
pub use pcb::{
    footprint_definition,
    BoardOutline,
    FootprintDefinition,
    FootprintPadDefinition,
    PcbAirwire,
    PcbComponentPlacement,
    PcbLayer,
    PcbLayout,
    PcbSyncSummary,
    PcbTrace,
};
pub use schematic::Schematic;
pub use schematic_graph::{SchematicGraph, LegacyWarning};
pub use simulation::{simulate_dc, SimulationResults};

