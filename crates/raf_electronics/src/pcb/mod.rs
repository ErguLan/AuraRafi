pub mod drc;
pub mod footprint;
pub mod footprint_asset;
pub mod layout;
pub mod manufacturing;
pub mod rules;
pub use rules::{NetRules, PcbDesignRules};
pub mod assembly;
pub use manufacturing::{export_manufacturing_files, ManufacturingFiles};

pub use footprint::{footprint_definition, FootprintDefinition, FootprintPadDefinition};
pub use footprint_asset::{
    parse_kicad_footprint, FootprintAsset, FootprintBoundsMm, FootprintDrill, FootprintDrillShape,
    FootprintGraphic, FootprintGraphicGeometry, FootprintImportError, FootprintLayer,
    FootprintLibrary, FootprintLibraryError, FootprintPad, FootprintPadLayer, FootprintPadShape,
    FootprintPadTechnology, FootprintProvenance, FootprintRules, FootprintStrokeStyle,
    FootprintTextRole, FootprintValidationIssue, PointMm, SizeMm, UnsupportedFootprintFeature,
};
pub use layout::{
    BoardOutline, PcbAirwire, PcbComponentPlacement, PcbLayer, PcbLayout, PcbSyncSummary, PcbTrace, PcbVia,
};
pub(crate) mod geometry;

/// Shared authoring/manufacturing contour validation.
pub fn geometry_outline_is_simple(points: &[glam::Vec2]) -> bool {
    drc::outline_is_valid(points, &std::sync::atomic::AtomicBool::new(false)).unwrap_or(false)
}

pub mod builtin_footprints;

#[cfg(test)]
mod outline_authoring_regressions {
    #[test]
    fn heart_is_valid_but_crossed_contour_is_not() {
        let heart = [(0.,-4.),(4.,-8.),(8.,-4.),(8.,0.),(0.,8.),(-8.,0.),(-8.,-4.),(-4.,-8.),(0.,-4.)].map(|(x,y)|glam::Vec2::new(x,y));
        assert!(super::geometry_outline_is_simple(&heart));
        let crossed = [(0.,0.),(8.,8.),(0.,8.),(8.,0.),(0.,0.)].map(|(x,y)|glam::Vec2::new(x,y));
        assert!(!super::geometry_outline_is_simple(&crossed));
    }
}
