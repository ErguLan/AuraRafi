//! Raf-authored generic land patterns, in mm. Explicit assignment is required;
//! these do not identify or certify a manufacturer's LED or battery holder.
use super::{parse_kicad_footprint, FootprintAsset};

pub fn assets() -> Vec<FootprintAsset> {
    [
        r#"(footprint "Raf_R_0805_Generic" (descr "Generic 0805 resistor land pattern; verify against part datasheet")
        (pad "1" smd rect (at -0.95 0) (size 1.1 1.5) (layers "F.Cu" "F.Mask" "F.Paste"))
        (pad "2" smd rect (at 0.95 0) (size 1.1 1.5) (layers "F.Cu" "F.Mask" "F.Paste")))"#,
        r#"(footprint "Raf_LED_0805_Generic" (descr "Generic 0805 LED land pattern; map A and K using the part datasheet")
        (pad "1" smd rect (at -0.95 0) (size 1.1 1.5) (layers "F.Cu" "F.Mask" "F.Paste"))
        (pad "2" smd rect (at 0.95 0) (size 1.1 1.5) (layers "F.Cu" "F.Mask" "F.Paste")))"#,
        r#"(footprint "Raf_PowerInput_P2.54mm" (descr "Two wire power input; 2.54 mm pitch, 1 mm drills. Not an 18650 holder")
        (pad "1" thru_hole rect (at -1.27 0) (size 2 2) (drill 1) (layers "*.Cu" "*.Mask"))
        (pad "2" thru_hole circle (at 1.27 0) (size 2 2) (drill 1) (layers "*.Cu" "*.Mask")))"#,
    ].into_iter().map(|source| {
        let mut asset = parse_kicad_footprint(source).expect("bundled physical footprint");
        asset.provenance.library = Some("Raf generic / project license".into());
        asset
    }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_land_patterns_have_physical_geometry_and_stable_identity() {
        let assets = super::assets();
        assert_eq!(assets.len(), 3);
        for (a, b) in assets.iter().zip(super::assets()) {
            assert!(a.is_fabrication_ready(), "{:?}", a.validate());
            assert_eq!(a.id, b.id);
            assert_eq!(a.pads.len(), 2);
            assert!(a.dimensions_mm().unwrap().width < 5.0);
        }
    }
}
