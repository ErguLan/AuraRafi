use super::library::safe_asset_stem;
use super::*;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use super::MAX_FOOTPRINT_SOURCE_BYTES;

const SAMPLE: &str = r#"; imported fixture with an escaped quote: \"part\"
(footprint "Test:Header 2.54mm"
  (version 20240108)
  (generator "pcbnew")
  (descr "Two pad \"header\"")
  (tags "header pin through-hole")
  (property "License" "CC0-1.0")
  (fp_line (start -1 -2) (end 1 -2) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
  (fp_rect (start -1 -1) (end 1 1) (stroke (width 0.1) (type default)) (fill none) (layer "F.Fab"))
  (fp_circle (center 0 0) (end 0.5 0) (stroke (width 0.1) (type solid)) (fill none) (layer "F.CrtYd"))
  (fp_text reference "REF**" (at 0 -3 90) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
  (pad "1" thru_hole circle (at -1.27 0 90) (size 1.8 1.8) (drill 0.8) (layers "*.Cu" "*.Mask"))
  (pad "2" thru_hole oval (at 1.27 0) (size 1.8 2.1) (drill oval 0.8 1.2 (offset 0.1 0)) (layers "*.Cu" "*.Mask"))
)"#;

fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("raf_footprints_{}", Uuid::new_v4()))
}

#[test]
fn parses_numbered_pads_rotations_drills_layers_and_manufacturing_graphics() {
    let footprint = parse_kicad_footprint(SAMPLE).expect("valid fixture");
    assert_eq!(footprint.name, "Test:Header 2.54mm");
    assert_eq!(footprint.provenance.format_version, Some(20240108));
    assert_eq!(footprint.provenance.license.as_deref(), Some("CC0-1.0"));
    assert_eq!(footprint.pads.len(), 2);
    assert_eq!(footprint.pads[0].number, "1");
    assert_eq!(footprint.pads[0].position_mm, PointMm { x: -1.27, y: 0.0 });
    assert_eq!(footprint.pads[0].rotation_degrees, 90.0);
    assert!(matches!(footprint.pads[0].shape, FootprintPadShape::Circle));
    assert!(matches!(
        footprint.pads[0].technology,
        FootprintPadTechnology::PlatedThroughHole
    ));
    assert!(matches!(
        footprint.pads[0].drill.as_ref().map(|drill| &drill.shape),
        Some(FootprintDrillShape::Circular { diameter_mm: 0.8 })
    ));
    assert!(footprint.pads[0]
        .layers
        .contains(&FootprintLayer::AllCopper));
    assert!(footprint.pads[0].layers.contains(&FootprintLayer::AllMask));
    assert!(matches!(
        footprint.pads[1].drill.as_ref().map(|drill| &drill.shape),
        Some(FootprintDrillShape::Oval {
            width_mm: 0.8,
            height_mm: 1.2
        })
    ));
    assert_eq!(footprint.graphics.len(), 4);
    assert!(footprint.is_fabrication_ready());
}

#[test]
fn unsupported_pad_shapes_are_reported_and_block_fabrication() {
    let source = r#"(footprint "Custom"
      (pad "1" smd custom (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste")
        (options (clearance outline) (anchor rect))
        (primitives (gr_poly (pts (xy 0 0) (xy 1 0) (xy 0 1)) (width 0) (fill yes))))
    )"#;
    let footprint = parse_kicad_footprint(source).expect("valid but unsupported feature");
    assert!(matches!(
        &footprint.pads[0].shape,
        FootprintPadShape::Unsupported { source_shape } if source_shape == "custom"
    ));
    assert!(footprint
        .unsupported_features
        .iter()
        .any(|feature| feature.blocks_fabrication && feature.feature.contains("custom")));
    assert!(!footprint.is_fabrication_ready());
}

#[test]
fn comments_escaped_strings_and_malformed_input_have_clear_behavior() {
    let source = r#"; top comment
(footprint "Escaped \"name\"" (property "license" "MIT\\compatible")
  (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste"))
)"#;
    let footprint = parse_kicad_footprint(source).expect("escaped input parses");
    assert_eq!(footprint.name, "Escaped \"name\"");
    assert!(footprint.provenance.license.unwrap().contains("\\"));
    let error = parse_kicad_footprint(
        "(footprint \"Broken\"\n  (pad \"1\" smd rect (at 0 0) (size 1 1)",
    )
    .expect_err("unclosed list must fail");
    assert!(error.message.contains("unclosed list"));
    assert_eq!(error.line, Some(2));
}

#[test]
fn parser_enforces_input_limits_and_asset_ids_cannot_escape_the_library() {
    let oversized = " ".repeat(MAX_FOOTPRINT_SOURCE_BYTES + 1);
    let error = parse_kicad_footprint(&oversized).unwrap_err();
    assert!(error.message.contains("size limit"));

    let traversal = safe_asset_stem("../../outside").expect("sanitized local id");
    assert!(!traversal.contains('/') && !traversal.contains('\\'));
    assert!(!traversal.contains(".."));
    assert_eq!(traversal, "outside");
}

#[test]
fn invalid_dimensions_and_drills_are_rejected_with_field_names() {
    let invalid_size = r#"(footprint "BadSize"
      (pad "A1" smd rect (at 0 0) (size 0 1) (layers "F.Cu" "F.Mask"))
    )"#;
    assert!(parse_kicad_footprint(invalid_size)
        .unwrap_err()
        .message
        .contains("size_mm"));
    let missing_drill = r#"(footprint "BadDrill"
      (pad "1" thru_hole circle (at 0 0) (size 1 1) (layers "*.Cu" "*.Mask"))
    )"#;
    assert!(parse_kicad_footprint(missing_drill)
        .unwrap_err()
        .message
        .contains("requires a drill"));
    let non_finite = r#"(footprint "BadNumber"
      (pad "1" smd rect (at nan 0) (size 1 1) (layers "F.Cu"))
    )"#;
    assert!(parse_kicad_footprint(non_finite)
        .unwrap_err()
        .message
        .contains("finite"));
}

#[test]
fn local_library_import_search_and_ron_round_trip_preserve_source() {
    let root = scratch_dir();
    let library = FootprintLibrary::open(&root).expect("open library");
    let imported = library
        .import_kicad_footprint(SAMPLE, None)
        .expect("import asset");
    assert_eq!(
        imported.source_text, SAMPLE,
        "source text is retained byte-for-byte"
    );
    assert_eq!(library.search("header").unwrap().len(), 1);
    let loaded = library
        .load("test_header_2_54mm")
        .unwrap()
        .expect("load by id");
    assert_eq!(loaded, imported);
    assert!(library.import_kicad_footprint(SAMPLE, None).is_err());
    fs::remove_dir_all(root).expect("cleanup scratch directory");
}
