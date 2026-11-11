//! Assembly tables use the same origin and Y-up coordinates as the CAM writer.
use super::{FootprintPadTechnology, PcbLayer, PcbLayout};
use std::collections::BTreeMap;

fn cell(raw: &str) -> String {
    // Spreadsheet formula characters must stay literal when a BOM is opened.
    let value = if raw.trim().len() > 1 && raw.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{raw}")
    } else {
        raw.to_string()
    };
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Logical pin names are retained alongside physical pad numbers, especially
/// for polarized parts whose pad numbering cannot be inferred from geometry.
pub fn export_pin_map(schematic: &crate::Schematic, layout: &PcbLayout) -> Result<String, String> {
    let mut csv = String::from("Designator,Logical Pin,Physical Pad,Net\r\n");
    for source in &schematic.components {
        let placed = layout
            .components
            .iter()
            .find(|p| p.component_id == source.id)
            .ok_or_else(|| format!("Missing PCB placement: {}", source.designator))?;
        let asset = layout
            .footprint_assets
            .get(&placed.footprint)
            .ok_or_else(|| format!("Missing physical footprint: {}", source.designator))?;
        for pin in &source.pins {
            let number = source.footprint_pin_map.get(&pin.name).unwrap_or(&pin.name);
            let index = asset
                .pads
                .iter()
                .position(|pad| &pad.number == number)
                .ok_or_else(|| format!("Unmapped pin {} / {}", source.designator, pin.name))?;
            csv.push_str(&format!(
                "{},{},{},{}\r\n",
                cell(&source.designator),
                cell(&pin.name),
                cell(number),
                cell(placed.pad_nets.get(index).map(String::as_str).unwrap_or(""))
            ));
        }
    }
    Ok(csv)
}

/// Real project data only. Ordering codes are deliberately empty until a part
/// is selected; the file never invents purchasable parts from footprint names.
pub fn export_assembly_files(layout: &PcbLayout) -> Result<BTreeMap<String, String>, String> {
    let mut bom = String::from("Comment,Designator,Footprint,LCSC Part #\r\n");
    let mut cpl = String::from("Designator,Mid X,Mid Y,Layer,Rotation\r\n");
    let mut components: Vec<_> = layout.components.iter().collect();
    components.sort_by(|a, b| a.designator.cmp(&b.designator));
    for component in components {
        if !component.position.is_finite() || !component.rotation.is_finite() {
            return Err(format!(
                "Invalid assembly coordinates: {}",
                component.designator
            ));
        }
        let asset = layout
            .footprint_assets
            .get(&component.footprint)
            .filter(|asset| asset.is_fabrication_ready())
            .ok_or_else(|| format!("Unresolved physical footprint: {}", component.designator))?;
        bom.push_str(&format!(
            "{},{},{},\r\n",
            cell(&component.value),
            cell(&component.designator),
            cell(&asset.name)
        ));
        if asset
            .pads
            .iter()
            .any(|pad| matches!(pad.technology, FootprintPadTechnology::Smd))
        {
            cpl.push_str(&format!(
                "{},{:.4},{:.4},{},{:.3}\r\n",
                cell(&component.designator),
                component.position.x,
                -component.position.y,
                if component.layer == PcbLayer::TopCopper {
                    "Top"
                } else {
                    "Bottom"
                },
                (-component.rotation).rem_euclid(360.0)
            ));
        }
    }
    Ok(BTreeMap::from([
        ("BOM.csv".into(), bom), ("CPL.csv".into(), cpl),
        ("ASSEMBLY-README.txt".into(), "Assembly preparation tables; ordering codes are unassigned.\nSelect actual parts and verify polarity, centroid and rotation with their datasheets and the assembly preview.\nCPL contains surface-mount components only. Through-hole parts remain in BOM for manual assembly.\nCoordinates: mm, board origin, Y-up, same as Gerber. Rotation is counterclockwise Y-up; bottom components are mirrored as in CAM.\nDo not submit as an approved assembly order without reviewing these fields.\n".into()),
    ]))
}
