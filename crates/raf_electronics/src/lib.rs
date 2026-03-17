//! # raf_electronics
//!
//! Electronic design subsystem: schematic editor, PCB layout,
//! simulation, DRC, export, and component library for AuraRafi.

pub mod component;
pub mod drc;
pub mod export;
pub mod library;
pub mod netlist;
pub mod schematic;
pub mod simulation;

pub use component::{ElectronicComponent, PinDirection, SimModel};
pub use drc::{run_drc, DrcReport, DrcIssue, DrcSeverity};
pub use export::{
    export_bom_csv, export_netlist_text, export_svg,
    export_gerber_stub, share_circuit, load_shared_circuit,
    ExportFormat, ExportResult, GerberTarget,
};
pub use library::ComponentLibrary;
pub use netlist::Netlist;
pub use schematic::Schematic;
pub use simulation::{simulate_dc, SimulationResults};
