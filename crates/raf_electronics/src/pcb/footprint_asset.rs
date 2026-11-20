//! Physical, serializable PCB footprint assets and a conservative KiCad importer.
//!
//! Geometry in this module is expressed in millimeters. Imported source text is
//! retained verbatim so unsupported KiCad data cannot disappear on a local
//! save/load cycle.

mod geometry;
mod library;
mod parser;
mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use library::{FootprintLibrary, FootprintLibraryError};
pub use parser::{parse_kicad_footprint, FootprintImportError};
pub use types::{
    FootprintAsset, FootprintBoundsMm, FootprintDrill, FootprintDrillShape, FootprintGraphic,
    FootprintGraphicGeometry, FootprintLayer, FootprintPad, FootprintPadLayer, FootprintPadShape,
    FootprintPadTechnology, FootprintProvenance, FootprintRules, FootprintStrokeStyle,
    FootprintTextRole, FootprintValidationIssue, PointMm, SizeMm, UnsupportedFootprintFeature,
};

const ASSET_SCHEMA_VERSION: u32 = 1;
const MAX_FOOTPRINT_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STORED_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_LIBRARY_TOTAL_BYTES: usize = 128 * 1024 * 1024;
const MAX_LIBRARY_ASSETS: usize = 4096;
const MAX_PADS_PER_ASSET: usize = 8192;
const MAX_GRAPHICS_PER_ASSET: usize = 32768;
const MAX_SEXPR_NODES: usize = 100_000;
const MAX_NESTING_DEPTH: usize = 128;
const MAX_ASSET_ID_BYTES: usize = 128;
