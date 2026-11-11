//! Physical two-layer bare-board output. No illustrative footprint fallback.
//!
//! Gerber uses millimetres, absolute 4.6 coordinates and exact circular draws
//! and flashes. Pads are composed from positive primitives, so drilled holes
//! are described independently in plated/non-plated Excellon files.

use super::geometry::transform_footprint_point;
use super::*;
use glam::Vec2;
use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct ManufacturingFiles {
    pub files: BTreeMap<String, String>,
    /// Decorative elements not emitted by the current writer are explicit.
    pub warnings: Vec<String>,
}

struct Gerber {
    data: String,
    apertures: BTreeMap<String, usize>,
}

impl Gerber {
    fn new(function: &str) -> Self {
        Self {
            data: format!("G04 AuraRafi physical PCB output*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TF.FilePolarity,Positive*%\n%LPD*%\nG01*\n"),
            apertures: BTreeMap::new(),
        }
    }

    fn aperture(&mut self, diameter: f64) {
        let key = format!("{diameter:.6}");
        let next = self.apertures.len() + 10;
        let code = *self.apertures.entry(key.clone()).or_insert(next);
        if code == next {
            writeln!(self.data, "%ADD{code}C,{key}*%").unwrap();
        }
        writeln!(self.data, "D{code}*").unwrap();
    }

    fn at(&mut self, point: Vec2, operation: u8) {
        // The native drafting canvas has downward Y; all exported files share
        // the same Cartesian origin and inversion, including drill files.
        writeln!(
            self.data,
            "X{}Y{}D{operation:02}*",
            coordinate(point.x as f64),
            coordinate(-point.y as f64)
        )
        .unwrap();
    }

    fn stroke(&mut self, points: &[Vec2], width: f64) {
        if points.len() < 2 || width <= 0.0 {
            return;
        }
        self.aperture(width);
        self.at(points[0], 2);
        for point in &points[1..] {
            self.at(*point, 1);
        }
    }

    fn disk(&mut self, center: Vec2, diameter: f64) {
        if diameter <= 0.0 {
            return;
        }
        self.aperture(diameter);
        self.at(center, 3);
    }

    fn polygon(&mut self, points: &[Vec2]) {
        if points.len() < 3 {
            return;
        }
        self.aperture(0.001);
        self.data.push_str("G36*\n");
        self.at(points[0], 2);
        for point in &points[1..] {
            self.at(*point, 1);
        }
        self.at(points[0], 1);
        self.data.push_str("G37*\n");
    }

    fn circle(&mut self, center: Vec2, radius: f64, width: f64, filled: bool) {
        if filled {
            self.disk(center, radius * 2.0);
            return;
        }
        if width <= 0.0 {
            return;
        }
        self.aperture(width);
        let start = center + Vec2::new(radius as f32, 0.0);
        self.at(start, 2);
        self.data.push_str("G75*\nG02*\n");
        writeln!(
            self.data,
            "X{}Y{}I{}J0D01*",
            coordinate(start.x as f64),
            coordinate(-start.y as f64),
            coordinate(-radius)
        )
        .unwrap();
        self.data.push_str("G01*\n");
    }

    fn finish(mut self) -> String {
        self.data.push_str("M02*\n");
        self.data
    }
}

fn coordinate(value: f64) -> i64 {
    (value * 1_000_000.0).round() as i64
}

fn local_point(placement: &PcbComponentPlacement, point: PointMm) -> Vec2 {
    transform_footprint_point(placement, point, true)
}

fn pad_point(placement: &PcbComponentPlacement, pad: &FootprintPad, x: f64, y: f64) -> Vec2 {
    let (sin, cos) = pad.rotation_degrees.to_radians().sin_cos();
    local_point(
        placement,
        PointMm {
            x: pad.position_mm.x + x * cos - y * sin,
            y: pad.position_mm.y + x * sin + y * cos,
        },
    )
}

fn pad_shape(
    output: &mut Gerber,
    placement: &PcbComponentPlacement,
    pad: &FootprintPad,
    margin: f64,
) {
    let width = pad.size_mm.width + margin * 2.0;
    let height = pad.size_mm.height + margin * 2.0;
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let radius = match pad.shape {
        FootprintPadShape::Circle => {
            output.disk(pad_point(placement, pad, 0.0, 0.0), width);
            return;
        }
        FootprintPadShape::Oval => width.min(height) * 0.5,
        FootprintPadShape::RoundRect {
            corner_ratio: Some(ratio),
        } => (pad.size_mm.width.min(pad.size_mm.height) * ratio + margin)
            .clamp(0.0, width.min(height) * 0.5),
        _ => 0.0,
    };
    let rect = |x: f64, y: f64| {
        [(-x, -y), (x, -y), (x, y), (-x, y)].map(|(x, y)| pad_point(placement, pad, x, y))
    };
    if radius <= 0.0 {
        output.polygon(&rect(width * 0.5, height * 0.5));
        return;
    }
    let x = width * 0.5 - radius;
    let y = height * 0.5 - radius;
    if x > 0.0 {
        output.polygon(&rect(x, height * 0.5));
    }
    if y > 0.0 {
        output.polygon(&rect(width * 0.5, y));
    }
    for (x, y) in [(-x, -y), (x, -y), (x, y), (-x, y)] {
        output.disk(pad_point(placement, pad, x, y), radius * 2.0);
    }
}

fn side_matches(layer: &FootprintLayer, placement: PcbLayer, side: PcbLayer, mask: bool) -> bool {
    let front = placement == side;
    if mask {
        matches!(layer, FootprintLayer::AllMask)
            || (front && matches!(layer, FootprintLayer::FrontMask))
            || (!front && matches!(layer, FootprintLayer::BackMask))
    } else {
        matches!(layer, FootprintLayer::AllCopper)
            || (front && matches!(layer, FootprintLayer::FrontCopper))
            || (!front && matches!(layer, FootprintLayer::BackCopper))
    }
}

/// Generate a complete bare-board file set only after physical preflight.
/// Does not write files, mutate the board, or claim assembly readiness.
pub fn export_manufacturing_files(layout: &PcbLayout) -> Result<ManufacturingFiles, String> {
    let report = layout.run_manufacturing_preflight();
    if !report.errors.is_empty() {
        return Err(report
            .errors
            .iter()
            .map(|issue| format!("{}: {}", issue.rule, issue.message))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    let in_range = |point: Vec2| point.is_finite() && point.abs().max_element() < 9999.0;
    if layout.board_outline.points.iter().any(|p| !in_range(*p))
        || layout
            .vias
            .iter()
            .any(|via| !in_range(via.position) || via.diameter > 10.0)
        || layout
            .traces
            .iter()
            .flat_map(|t| &t.points)
            .any(|p| !in_range(*p))
        || layout.components.iter().any(|c| {
            let asset = &layout.footprint_assets[&c.footprint];
            asset.bounds_mm().is_none_or(|bounds| {
                [
                    bounds.min,
                    bounds.max,
                    PointMm {
                        x: bounds.min.x,
                        y: bounds.max.y,
                    },
                    PointMm {
                        x: bounds.max.x,
                        y: bounds.min.y,
                    },
                ]
                .iter()
                .any(|p| !in_range(local_point(c, *p)))
            })
        })
    {
        return Err("Geometry exceeds the Gerber 4.6 coordinate range.".into());
    }
    let mut result = ManufacturingFiles {
        files: BTreeMap::new(),
        warnings: Vec::new(),
    };
    for (side, copper_name, mask_name, silk_name, number, face) in [
        (
            PcbLayer::TopCopper,
            "board-F_Cu.gbr",
            "board-F_Mask.gbr",
            "board-F_Silkscreen.gbr",
            1,
            "Top",
        ),
        (
            PcbLayer::BottomCopper,
            "board-B_Cu.gbr",
            "board-B_Mask.gbr",
            "board-B_Silkscreen.gbr",
            2,
            "Bot",
        ),
    ] {
        let mut copper = Gerber::new(&format!("Copper,L{number},{face}"));
        let mut mask = Gerber::new(&format!("Soldermask,{face}"));
        let mut silk = Gerber::new(&format!("Legend,{face}"));
        for trace in layout.traces.iter().filter(|trace| trace.layer == side) {
            copper.stroke(&trace.points, trace.width as f64);
        }
        for via in &layout.vias {
            copper.disk(via.position, via.diameter as f64);
            // Untented through vias: both mask openings follow the actual land.
            mask.disk(via.position, via.diameter as f64);
        }
        for placement in &layout.components {
            let asset = &layout.footprint_assets[&placement.footprint];
            for pad in &asset.pads {
                if !matches!(pad.technology, FootprintPadTechnology::NonPlatedThroughHole)
                    && pad
                        .layers
                        .iter()
                        .any(|layer| side_matches(layer, placement.layer, side, false))
                {
                    pad_shape(&mut copper, placement, pad, 0.0);
                }
                if pad
                    .layers
                    .iter()
                    .any(|layer| side_matches(layer, placement.layer, side, true))
                {
                    pad_shape(
                        &mut mask,
                        placement,
                        pad,
                        pad.solder_mask_margin_mm
                            .or(asset.rules.solder_mask_margin_mm)
                            .unwrap_or(0.0),
                    );
                }
            }
            for graphic in &asset.graphics {
                let is_front = placement.layer == side;
                if (is_front && graphic.layer == FootprintLayer::FrontSilkscreen)
                    || (!is_front && graphic.layer == FootprintLayer::BackSilkscreen)
                {
                    write_graphic(&mut silk, placement, graphic, &mut result.warnings)?;
                }
            }
        }
        result.files.insert(copper_name.into(), copper.finish());
        result.files.insert(mask_name.into(), mask.finish());
        result.files.insert(silk_name.into(), silk.finish());
    }
    let mut outline = Gerber::new("Profile,NP");
    outline.stroke(&layout.board_outline.points, 0.05);
    result
        .files
        .insert("board-Edge_Cuts.gbr".into(), outline.finish());
    for plated in [true, false] {
        result.files.insert(
            if plated {
                "board-PTH.drl"
            } else {
                "board-NPTH.drl"
            }
            .into(),
            drills(layout, plated),
        );
    }
    result.warnings.sort();
    result.warnings.dedup();
    result.files.insert("README.txt".into(),format!("AuraRafi bare-board manufacturing data\nUnits: mm; Gerber 4.6; Excellon decimal mm.\nNative canvas Y is negated consistently in every file.\nCheck all layers and drill alignment in an independent CAM reader before ordering.\nAssembly BOM/CPL and 3D models are not included.\n{}\n",result.warnings.join("\n")));
    Ok(result)
}

fn write_graphic(
    output: &mut Gerber,
    placement: &PcbComponentPlacement,
    graphic: &FootprintGraphic,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if !matches!(
        graphic.style,
        FootprintStrokeStyle::Default | FootprintStrokeStyle::Solid
    ) {
        warnings.push("Dashed silkscreen strokes are emitted as continuous strokes.".into());
    }
    let transform = |point| local_point(placement, point);
    match &graphic.geometry {
        FootprintGraphicGeometry::Line {
            start_mm,
            end_mm,
            width_mm,
        } => output.stroke(&[transform(*start_mm), transform(*end_mm)], *width_mm),
        FootprintGraphicGeometry::Rectangle {
            start_mm,
            end_mm,
            width_mm,
            filled,
        } => {
            let points = [
                *start_mm,
                PointMm {
                    x: end_mm.x,
                    y: start_mm.y,
                },
                *end_mm,
                PointMm {
                    x: start_mm.x,
                    y: end_mm.y,
                },
                *start_mm,
            ]
            .map(transform);
            if *filled {
                output.polygon(&points[..4]);
            }
            output.stroke(&points, *width_mm);
        }
        FootprintGraphicGeometry::Circle {
            center_mm,
            radius_mm,
            width_mm,
            filled,
        } => output.circle(transform(*center_mm), *radius_mm, *width_mm, *filled),
        FootprintGraphicGeometry::Polygon {
            points_mm,
            width_mm,
            filled,
        } => {
            let mut points = points_mm.iter().copied().map(transform).collect::<Vec<_>>();
            if *filled {
                output.polygon(&points);
            }
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            output.stroke(&points, *width_mm);
        }
        FootprintGraphicGeometry::Arc {
            start_mm,
            mid_mm,
            end_mm,
            width_mm,
        } => {
            // Three points determine a unique circular arc. Split at its
            // supplied middle point, retaining its sweep under reflection.
            let (a, b, c) = (transform(*start_mm), transform(*mid_mm), transform(*end_mm));
            let determinant = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
            if determinant.abs() < 1e-8 {
                return Err("Degenerate footprint arc".into());
            }
            let (aa, bb, cc) = (a.length_squared(), b.length_squared(), c.length_squared());
            let center = Vec2::new(
                (aa * (b.y - c.y) + bb * (c.y - a.y) + cc * (a.y - b.y)) / determinant,
                (aa * (c.x - b.x) + bb * (a.x - c.x) + cc * (b.x - a.x)) / determinant,
            );
            output.aperture(*width_mm);
            output.at(a, 2);
            output.data.push_str("G75*\n");
            // The native coordinate reflection reverses the sweep in Gerber.
            let native_ccw = (b - a).perp_dot(c - b) > 0.0;
            output
                .data
                .push_str(if native_ccw { "G02*\n" } else { "G03*\n" });
            for (from, to) in [(a, b), (b, c)] {
                writeln!(
                    output.data,
                    "X{}Y{}I{}J{}D01*",
                    coordinate(to.x as f64),
                    coordinate(-to.y as f64),
                    coordinate((center.x - from.x) as f64),
                    coordinate(-(center.y - from.y) as f64)
                )
                .unwrap();
            }
            output.data.push_str("G01*\n");
        }
        FootprintGraphicGeometry::Text { hidden, .. } => {
            if !hidden {
                warnings.push("Silkscreen text is omitted by this writer; vector silkscreen geometry is included.".into());
            }
        }
        FootprintGraphicGeometry::Unsupported { source_kind } => {
            return Err(format!("Unsupported manufacturing graphic: {source_kind}"))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{component::ElectronicComponent, Schematic};

    fn board() -> PcbLayout {
        let asset=parse_kicad_footprint(r#"(footprint "RealTwoPads"
          (layer "F.Cu")
          (fp_line (start -2 -1.2) (end 2 -1.2) (stroke (width 0.15) (type default)) (layer "F.SilkS"))
          (pad "1" thru_hole circle (at -1.5 0) (size 1.6 1.6) (drill 0.8) (layers "*.Cu" "*.Mask"))
          (pad "2" thru_hole oval (at 1.5 0 30) (size 1.6 2.0) (drill oval 0.8 1.2 (offset 0.1 0)) (layers "*.Cu" "*.Mask"))
          (pad "" np_thru_hole circle (at 0 -2) (size 0.6 0.6) (drill 0.6) (layers "*.Cu" "*.Mask"))
        )"#).unwrap();
        let mut schematic = Schematic::new("physical");
        let mut component = ElectronicComponent::resistor("1k");
        component.footprint = asset.id.clone();
        schematic.add_component(component);
        let mut board = PcbLayout::new("physical");
        board.footprint_assets.insert(asset.id.clone(), asset);
        board.sync_from_schematic(&schematic);
        board.board_outline = BoardOutline::default_rect(12.0, 10.0);
        board.components[0].position = Vec2::new(6.0, 5.0);
        board.components[0].rotation = 90.0;
        board.rebuild_airwires();
        board
    }

    #[test]
    fn manufacturing_emits_real_aligned_layers_and_separate_plated_drills() {
        let board = board();
        let output = export_manufacturing_files(&board).unwrap();
        assert_eq!(output.files.len(), 10);
        let copper = &output.files["board-F_Cu.gbr"];
        assert!(copper.contains("%FSLAX46Y46*%"));
        assert!(copper.contains("X6000000Y-3500000D03*"));
        assert!(copper.ends_with("M02*\n"));
        assert!(output.files["board-B_Cu.gbr"].contains("X6000000Y-3500000D03*"));
        assert!(output.files["board-PTH.drl"].contains("T01C0.800000"));
        assert!(output.files["board-PTH.drl"].contains("X6.000000Y-3.500000"));
        assert!(output.files["board-PTH.drl"].contains("G85"));
        assert!(output.files["board-NPTH.drl"].contains("T01C0.600000"));
        assert!(output.files["board-NPTH.drl"].contains("X8.000000Y-5.000000"));
        assert!(output.files["board-Edge_Cuts.gbr"].contains("X12000000Y-10000000D01*"));
        if let Some(directory) = std::env::var_os("RAF_PCB_CAM_FIXTURE_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            for (name, content) in &output.files {
                std::fs::write(directory.join(name), content).unwrap();
            }
        }
    }

    #[test]
    fn unresolved_physical_footprints_cannot_be_exported() {
        let mut board = board();
        board.footprint_assets.clear();
        assert!(export_manufacturing_files(&board).is_err());
    }

    #[test]
    fn bottom_placement_mirrors_pads_and_drills_together() {
        let mut board = board();
        board.components[0].rotation = 0.0;
        board.components[0].layer = PcbLayer::BottomCopper;
        let output = export_manufacturing_files(&board).unwrap();
        assert!(output.files["board-F_Cu.gbr"].contains("X7500000Y-5000000D03*"));
        assert!(output.files["board-PTH.drl"].contains("X7.500000Y-5.000000"));
    }
}

fn drills(layout: &PcbLayout, plated: bool) -> String {
    let mut tools: BTreeMap<String, Vec<(Vec2, Option<Vec2>)>> = BTreeMap::new();
    if plated {
        for via in &layout.vias {
            tools
                .entry(format!("{:.6}", via.drill))
                .or_default()
                .push((via.position, None));
        }
    }
    for placement in &layout.components {
        for pad in &layout.footprint_assets[&placement.footprint].pads {
            if matches!(pad.technology, FootprintPadTechnology::PlatedThroughHole) != plated {
                continue;
            }
            let Some(drill) = &pad.drill else {
                continue;
            };
            let (width, height) = match drill.shape {
                FootprintDrillShape::Circular { diameter_mm } => (diameter_mm, diameter_mm),
                FootprintDrillShape::Oval {
                    width_mm,
                    height_mm,
                } => (width_mm, height_mm),
            };
            let center = drill.offset_mm;
            let (dx, dy) = (
                (width - height).max(0.0) * 0.5,
                (height - width).max(0.0) * 0.5,
            );
            let start = pad_point(placement, pad, center.x - dx, center.y - dy);
            let end = pad_point(placement, pad, center.x + dx, center.y + dy);
            tools
                .entry(format!("{:.6}", width.min(height)))
                .or_default()
                .push((start, (width != height).then_some(end)));
        }
    }
    let mut out = String::from("M48\n; AuraRafi Excellon; decimal millimeters\nMETRIC,TZ\nG90\n");
    for (index, diameter) in tools.keys().enumerate() {
        writeln!(out, "T{:02}C{diameter}", index + 1).unwrap();
    }
    out.push_str("%\nG05\n");
    for (index, holes) in tools.values().enumerate() {
        writeln!(out, "T{:02}", index + 1).unwrap();
        for (start, end) in holes {
            write!(out, "X{:.6}Y{:.6}", start.x, -start.y).unwrap();
            if let Some(end) = end {
                write!(out, "G85X{:.6}Y{:.6}", end.x, -end.y).unwrap();
            }
            out.push('\n');
        }
    }
    out.push_str("M30\n");
    out
}
