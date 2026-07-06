//! # raf_electronics
//!
//! Electronic design subsystem: schematic editor, PCB layout,
//! simulation, DRC, export, and component library for AuraRafi.

pub mod component;
pub mod drc;
pub mod export;
pub mod extensions;
pub mod library;
pub mod netlist;
pub mod pcb;
pub mod schematic;
pub mod schematic_graph;
pub mod simulation;

pub use component::{ElectronicComponent, PinDirection, SimModel};
pub use drc::{run_drc, DrcIssue, DrcReport, DrcSeverity};
pub use export::{
    export_bom_csv, export_gerber_layout_stub, export_gerber_stub, export_netlist_text, export_svg,
    load_shared_circuit, share_circuit, ExportFormat, ExportResult, GerberTarget,
};
pub use extensions::{
    register_component_template, register_drc_rule, registered_extension_summary,
    ElectricalExtensionRegistry, ElectricalExtensionSummary, ElectricalRule,
};
pub use library::{ComponentLibrary, ComponentTemplate};
pub use netlist::Netlist;
pub use pcb::{
    footprint_definition, BoardOutline, FootprintDefinition, FootprintPadDefinition, PcbAirwire,
    PcbComponentPlacement, PcbLayer, PcbLayout, PcbSyncSummary, PcbTrace,
};
pub use schematic::Schematic;
pub use schematic_graph::{LegacyWarning, SchematicGraph};
pub use simulation::{simulate_dc, SimulationResults};
