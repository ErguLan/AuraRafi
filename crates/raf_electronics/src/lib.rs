//! # raf_electronics
//!
//! Electronic design subsystem: schematic editor, PCB layout, and
//! component library for AuraRafi.

pub mod component;
pub mod library;
pub mod schematic;

pub use component::{ElectronicComponent, PinDirection};
pub use library::ComponentLibrary;
pub use schematic::Schematic;
