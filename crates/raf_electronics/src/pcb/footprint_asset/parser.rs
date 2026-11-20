use std::error::Error;
use std::fmt;

use super::library::safe_asset_stem;
use super::types::{
    FootprintAsset, FootprintDrill, FootprintDrillShape, FootprintGraphic,
    FootprintGraphicGeometry, FootprintLayer, FootprintPad, FootprintPadShape,
    FootprintPadTechnology, FootprintProvenance, FootprintRules, FootprintStrokeStyle,
    FootprintTextRole, PointMm, SizeMm, UnsupportedFootprintFeature,
};
use super::{
    ASSET_SCHEMA_VERSION, MAX_FOOTPRINT_SOURCE_BYTES, MAX_NESTING_DEPTH, MAX_SEXPR_NODES,
};

/// Syntax or data error reported while reading one KiCad footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootprintImportError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl FootprintImportError {
    fn at(source: &str, offset: usize, message: impl Into<String>) -> Self {
        let safe_offset = offset.min(source.len());
        let prefix = &source[..safe_offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .map(|tail| tail.chars().count() + 1)
            .unwrap_or(1);
        Self {
            message: message.into(),
            line: Some(line),
            column: Some(column),
        }
    }

    fn data(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
        }
    }
}

impl fmt::Display for FootprintImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(line), Some(column)) = (self.line, self.column) {
            write!(
                formatter,
                "{} (línea {line}, columna {column})",
                self.message
            )
        } else {
            formatter.write_str(&self.message)
        }
    }
}

impl Error for FootprintImportError {}

/// Parses a single `.kicad_mod` file. Unsupported physical features stay in
/// `unsupported_features` and make `is_fabrication_ready()` return false.
pub fn parse_kicad_footprint(source: &str) -> Result<FootprintAsset, FootprintImportError> {
    if source.len() > MAX_FOOTPRINT_SOURCE_BYTES {
        return Err(FootprintImportError::data(format!(
            "footprint source exceeds the {MAX_FOOTPRINT_SOURCE_BYTES}-byte size limit"
        )));
    }
    let root = SExprParser::new(source).parse_document()?;
    parse_footprint_node(source, &root)
}

fn parse_footprint_node(
    source: &str,
    root: &SExpr,
) -> Result<FootprintAsset, FootprintImportError> {
    let Some(items) = root.as_list() else {
        return Err(FootprintImportError::at(
            source,
            root.start,
            "top-level expression must be a footprint list",
        ));
    };
    let root_kind = items.first().and_then(SExpr::as_text).unwrap_or_default();
    if root_kind != "footprint" && root_kind != "module" {
        return Err(FootprintImportError::at(
            source,
            root.start,
            format!("expected (footprint ...), found ({root_kind} ...)"),
        ));
    }
    let name = items
        .get(1)
        .and_then(SExpr::as_text)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            FootprintImportError::at(source, root.start, "footprint name is missing or empty")
        })?
        .to_string();

    let mut asset = FootprintAsset {
        schema_version: ASSET_SCHEMA_VERSION,
        id: safe_asset_stem(&name).ok_or_else(|| {
            FootprintImportError::at(
                source,
                items[1].start,
                "footprint name cannot form a local asset id",
            )
        })?,
        name,
        description: None,
        tags: Vec::new(),
        pads: Vec::new(),
        graphics: Vec::new(),
        rules: FootprintRules::default(),
        provenance: FootprintProvenance {
            source_format: if root_kind == "module" {
                "kicad_mod_legacy_module".to_string()
            } else {
                "kicad_mod".to_string()
            },
            ..FootprintProvenance::default()
        },
        unsupported_features: Vec::new(),
        source_text: source.to_string(),
    };

    for item in items.iter().skip(2) {
        let Some(child) = item.as_list() else {
            if let Some(attribute) = item.as_text() {
                if attribute == "locked" || attribute == "placed" {
                    asset.unsupported_features.push(feature(
                        source,
                        "footprint",
                        attribute,
                        item,
                        false,
                    ));
                } else {
                    asset.unsupported_features.push(feature(
                        source,
                        "footprint",
                        attribute,
                        item,
                        true,
                    ));
                }
            }
            continue;
        };
        let Some(kind) = child.first().and_then(SExpr::as_text) else {
            continue;
        };
        match kind {
            "version" => {
                asset.provenance.format_version = child
                    .get(1)
                    .and_then(SExpr::as_text)
                    .and_then(|value| value.parse::<u32>().ok());
            }
            "generator" => {
                asset.provenance.generator =
                    child.get(1).and_then(SExpr::as_text).map(str::to_owned);
            }
            "descr" => {
                asset.description = child.get(1).and_then(SExpr::as_text).map(str::to_owned);
            }
            "tags" => {
                if let Some(tags) = child.get(1).and_then(SExpr::as_text) {
                    asset.tags = split_tags(tags);
                }
            }
            "property" => {
                let property = child.get(1).and_then(SExpr::as_text).unwrap_or_default();
                if matches!(
                    property.to_ascii_lowercase().as_str(),
                    "license" | "licence"
                ) {
                    asset.provenance.license =
                        child.get(2).and_then(SExpr::as_text).map(str::to_owned);
                } else {
                    asset.unsupported_features.push(feature(
                        source,
                        "footprint",
                        "property metadata",
                        item,
                        false,
                    ));
                }
            }
            "solder_mask_margin" => {
                asset.rules.solder_mask_margin_mm =
                    Some(number_required(source, child.get(1), "solder_mask_margin")?);
            }
            "solder_paste_margin" => {
                asset.rules.solder_paste_margin_mm = Some(number_required(
                    source,
                    child.get(1),
                    "solder_paste_margin",
                )?);
            }
            "solder_paste_ratio" => {
                asset.rules.solder_paste_ratio =
                    Some(number_required(source, child.get(1), "solder_paste_ratio")?);
            }
            "clearance" => {
                asset.rules.clearance_mm =
                    Some(number_required(source, child.get(1), "clearance")?);
            }
            "thermal_width" => {
                asset.rules.thermal_width_mm =
                    Some(number_required(source, child.get(1), "thermal_width")?);
            }
            "thermal_gap" => {
                asset.rules.thermal_gap_mm =
                    Some(number_required(source, child.get(1), "thermal_gap")?);
            }
            "zone_connect" => {
                asset.rules.zone_connect = child.get(1).and_then(SExpr::as_text).map(str::to_owned);
            }
            "attr" => {
                asset.rules.attributes = child
                    .iter()
                    .skip(1)
                    .filter_map(SExpr::as_text)
                    .map(str::to_owned)
                    .collect();
            }
            "private_layers" => {
                asset.rules.private_layers = child
                    .iter()
                    .skip(1)
                    .filter_map(SExpr::as_text)
                    .map(FootprintLayer::from_kicad_name)
                    .collect();
            }
            "pad" => parse_pad(source, item, &mut asset)?,
            "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly" | "fp_text" => {
                parse_graphic(source, item, &mut asset)?;
            }
            "layer" | "tedit" | "tstamp" | "uuid" | "path" | "autoplace_cost90"
            | "autoplace_cost180" | "locked" | "placed" => {
                asset.unsupported_features.push(feature(
                    source,
                    "footprint",
                    format!("non-geometry metadata '{kind}'"),
                    item,
                    false,
                ));
            }
            "model" | "image" => asset.unsupported_features.push(feature(
                source,
                "footprint",
                format!("presentation data '{kind}'"),
                item,
                false,
            )),
            _ => asset.unsupported_features.push(feature(
                source,
                "footprint",
                format!("unhandled footprint feature '{kind}'"),
                item,
                true,
            )),
        }
    }

    for (index, pad) in asset.pads.iter().enumerate() {
        let location = format!("pad[{index}] ({})", pad.number);
        let mut issues = Vec::new();
        super::validation::validate_pad(pad, &location, &mut issues);
        if let Some(issue) = issues.first() {
            return Err(FootprintImportError::data(format!(
                "{}: {}",
                issue.location, issue.message
            )));
        }
    }
    for (index, graphic) in asset.graphics.iter().enumerate() {
        let location = format!("graphics[{index}]");
        let mut issues = Vec::new();
        super::validation::validate_graphic(graphic, &location, &mut issues);
        if let Some(issue) = issues.first() {
            return Err(FootprintImportError::data(format!(
                "{}: {}",
                issue.location, issue.message
            )));
        }
    }
    if let Some(issue) = asset.validate().first() {
        return Err(FootprintImportError::data(format!(
            "{}: {}",
            issue.location, issue.message
        )));
    }
    Ok(asset)
}

fn parse_pad(
    source: &str,
    expression: &SExpr,
    asset: &mut FootprintAsset,
) -> Result<(), FootprintImportError> {
    let Some(items) = expression.as_list() else {
        return Ok(());
    };
    if items.len() < 6 {
        return Err(FootprintImportError::at(
            source,
            expression.start,
            "pad needs a number, type, shape, position, and size",
        ));
    }
    let number_text = items.get(1).and_then(SExpr::as_text).unwrap_or_default();
    let source_type = items.get(2).and_then(SExpr::as_text).unwrap_or_default();
    let source_shape = items.get(3).and_then(SExpr::as_text).unwrap_or_default();
    let at = find_child(items, "at").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("pad '{number_text}' has no at field"),
        )
    })?;
    let (position_mm, rotation_degrees) = parse_position(source, at, "pad at")?;
    let size = find_child(items, "size").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("pad '{number_text}' has no size field"),
        )
    })?;
    let size_items = size.as_list().unwrap_or_default();
    let size_mm = SizeMm {
        width: number_required(source, size_items.get(1), "pad size width")?,
        height: number_required(source, size_items.get(2), "pad size height")?,
    };

    let (technology, technology_supported) = match source_type {
        "smd" => (FootprintPadTechnology::Smd, true),
        "thru_hole" => (FootprintPadTechnology::PlatedThroughHole, true),
        "np_thru_hole" => (FootprintPadTechnology::NonPlatedThroughHole, true),
        other => (
            FootprintPadTechnology::Unsupported {
                source_type: other.to_string(),
            },
            false,
        ),
    };
    if !technology_supported {
        asset.unsupported_features.push(feature(
            source,
            format!("pad '{number_text}'"),
            format!("pad type '{source_type}'"),
            expression,
            true,
        ));
    }

    let ratio = find_child(items, "roundrect_rratio")
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .map(|value| number_required(source, Some(value), "roundrect_rratio"))
        .transpose()?;
    let shape = match source_shape {
        "rect" => FootprintPadShape::Rectangle,
        "circle" => FootprintPadShape::Circle,
        "oval" => FootprintPadShape::Oval,
        "roundrect" => FootprintPadShape::RoundRect {
            corner_ratio: ratio,
        },
        other => {
            asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("pad shape '{other}'"),
                expression,
                true,
            ));
            FootprintPadShape::Unsupported {
                source_shape: other.to_string(),
            }
        }
    };

    let layer_node = find_child(items, "layers").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("pad '{number_text}' has no layers field"),
        )
    })?;
    let layer_items = layer_node.as_list().unwrap_or_default();
    let mut layers = Vec::new();
    for item in layer_items.iter().skip(1) {
        let Some(name) = item.as_text() else {
            return Err(FootprintImportError::at(
                source,
                item.start,
                format!("pad '{number_text}' has a non-string layer"),
            ));
        };
        let layer = FootprintLayer::from_kicad_name(name);
        if matches!(&layer, FootprintLayer::Other(_)) {
            asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("pad layer '{name}'"),
                layer_node,
                true,
            ));
        }
        if matches!(&layer, FootprintLayer::InnerCopper(_)) {
            asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("inner copper layer '{name}' is outside the two-layer PCB stack"),
                layer_node,
                true,
            ));
        }
        layers.push(layer);
    }

    let drill = find_child(items, "drill")
        .map(|node| parse_drill(source, node))
        .transpose()?;
    let solder_mask_margin_mm = parse_optional_number(source, items, "solder_mask_margin")?;
    let solder_paste_margin_mm = parse_optional_number(source, items, "solder_paste_margin")?;
    let solder_paste_margin_ratio =
        parse_optional_number(source, items, "solder_paste_margin_ratio")?;
    let pad = FootprintPad {
        number: number_text.to_string(),
        position_mm,
        rotation_degrees,
        size_mm,
        shape,
        technology,
        layers,
        drill,
        solder_mask_margin_mm,
        solder_paste_margin_mm,
        solder_paste_margin_ratio,
    };

    for child in items
        .iter()
        .skip(4)
        .filter(|child| child.as_list().is_some())
    {
        let children = child.as_list().unwrap_or_default();
        let kind = children
            .first()
            .and_then(SExpr::as_text)
            .unwrap_or_default();
        match kind {
            "at"
            | "size"
            | "drill"
            | "layers"
            | "roundrect_rratio"
            | "solder_mask_margin"
            | "solder_paste_margin"
            | "solder_paste_margin_ratio"
            | "net"
            | "uuid"
            | "pinfunction"
            | "pintype"
            | "property"
            | "clearance"
            | "zone_connect"
            | "thermal_width"
            | "thermal_gap"
            | "die_length" => {}
            "chamfer" | "chamfer_ratio" => asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("chamfered pad feature '{kind}'"),
                child,
                true,
            )),
            _ => asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("unhandled pad feature '{kind}'"),
                child,
                true,
            )),
        }
    }
    if matches!(source_shape, "roundrect") && ratio.is_none() {
        asset.unsupported_features.push(feature(
            source,
            format!("pad '{number_text}'"),
            "round rectangle with implicit corner ratio",
            expression,
            true,
        ));
    }
    asset.pads.push(pad);
    Ok(())
}

fn parse_drill(source: &str, expression: &SExpr) -> Result<FootprintDrill, FootprintImportError> {
    let values = expression.as_list().unwrap_or_default();
    let is_oval = values.get(1).and_then(SExpr::as_text) == Some("oval");
    let first_number_index = if is_oval { 2 } else { 1 };
    let first = number_required(source, values.get(first_number_index), "drill diameter")?;
    let shape = if is_oval {
        let second = number_required(
            source,
            values.get(first_number_index + 1),
            "oval drill height",
        )?;
        FootprintDrillShape::Oval {
            width_mm: first,
            height_mm: second,
        }
    } else {
        FootprintDrillShape::Circular { diameter_mm: first }
    };
    let offset_mm = find_child(values, "offset")
        .map(|node| parse_point(source, node, "drill offset"))
        .transpose()?
        .unwrap_or(PointMm::ZERO);
    Ok(FootprintDrill { shape, offset_mm })
}

fn parse_graphic(
    source: &str,
    expression: &SExpr,
    asset: &mut FootprintAsset,
) -> Result<(), FootprintImportError> {
    let Some(items) = expression.as_list() else {
        return Ok(());
    };
    let kind = items.first().and_then(SExpr::as_text).unwrap_or_default();
    let layer_node = find_child(items, "layer").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("graphic '{kind}' has no layer field"),
        )
    })?;
    let layer_name = layer_node
        .as_list()
        .and_then(|layer| layer.get(1))
        .and_then(SExpr::as_text)
        .ok_or_else(|| {
            FootprintImportError::at(source, layer_node.start, "graphic layer is malformed")
        })?;
    let layer = FootprintLayer::from_kicad_name(layer_name);
    let style = parse_stroke_style(items);
    if matches!(&style, FootprintStrokeStyle::Unsupported(_)) {
        asset.unsupported_features.push(feature(
            source,
            format!("graphic on {layer_name}"),
            "unsupported stroke style",
            expression,
            true,
        ));
    }
    let width = if kind == "fp_text" {
        0.0
    } else {
        parse_graphic_width(source, items)?
    };
    let geometry = match kind {
        "fp_line" => FootprintGraphicGeometry::Line {
            start_mm: required_point(source, items, "start")?,
            end_mm: required_point(source, items, "end")?,
            width_mm: width,
        },
        "fp_rect" => FootprintGraphicGeometry::Rectangle {
            start_mm: required_point(source, items, "start")?,
            end_mm: required_point(source, items, "end")?,
            width_mm: width,
            filled: parse_filled(items),
        },
        "fp_circle" => {
            let center_mm = required_point(source, items, "center")?;
            let end_mm = required_point(source, items, "end")?;
            let dx = end_mm.x - center_mm.x;
            let dy = end_mm.y - center_mm.y;
            FootprintGraphicGeometry::Circle {
                center_mm,
                radius_mm: dx.hypot(dy),
                width_mm: width,
                filled: parse_filled(items),
            }
        }
        "fp_arc" => FootprintGraphicGeometry::Arc {
            start_mm: required_point(source, items, "start")?,
            mid_mm: required_point(source, items, "mid")?,
            end_mm: required_point(source, items, "end")?,
            width_mm: width,
        },
        "fp_poly" => FootprintGraphicGeometry::Polygon {
            points_mm: parse_points(source, items)?,
            width_mm: width,
            filled: parse_filled(items),
        },
        "fp_text" => parse_text(source, items)?,
        _ => FootprintGraphicGeometry::Unsupported {
            source_kind: kind.to_string(),
        },
    };
    if matches!(&layer, FootprintLayer::Other(_)) {
        asset.unsupported_features.push(feature(
            source,
            format!("graphic '{kind}'"),
            format!("graphic layer '{layer_name}'"),
            layer_node,
            layer.is_physical(),
        ));
    }
    asset.graphics.push(FootprintGraphic {
        layer,
        style,
        geometry,
    });
    Ok(())
}

fn parse_text(
    source: &str,
    items: &[SExpr],
) -> Result<FootprintGraphicGeometry, FootprintImportError> {
    let role_text = items.get(1).and_then(SExpr::as_text).unwrap_or_default();
    let text = items.get(2).and_then(SExpr::as_text).ok_or_else(|| {
        FootprintImportError::at(source, items[0].start, "fp_text string is missing")
    })?;
    let at = find_child(items, "at").ok_or_else(|| {
        FootprintImportError::at(source, items[0].start, "fp_text has no at field")
    })?;
    let (position_mm, rotation_degrees) = parse_position(source, at, "fp_text at")?;
    let effects = find_child(items, "effects");
    let (size_mm, thickness_mm, font_face, bold, italic, mirrored, line_spacing) =
        parse_text_effects(source, effects)?;
    Ok(FootprintGraphicGeometry::Text {
        role: match role_text {
            "reference" => FootprintTextRole::Reference,
            "value" => FootprintTextRole::Value,
            "user" => FootprintTextRole::User,
            other => FootprintTextRole::Other(other.to_string()),
        },
        text: text.to_string(),
        position_mm,
        rotation_degrees,
        size_mm,
        thickness_mm,
        font_face,
        bold,
        italic,
        mirrored,
        hidden: items.iter().any(|item| item.as_text() == Some("hide")),
        line_spacing,
    })
}

type TextEffects = (SizeMm, f64, Option<String>, bool, bool, bool, Option<f64>);

fn parse_text_effects(
    source: &str,
    effects: Option<&SExpr>,
) -> Result<TextEffects, FootprintImportError> {
    let Some(effects) = effects.and_then(SExpr::as_list) else {
        return Ok((
            SizeMm {
                width: 1.0,
                height: 1.0,
            },
            0.15,
            None,
            false,
            false,
            false,
            None,
        ));
    };
    let font = find_child(effects, "font").and_then(SExpr::as_list);
    let size_node = font.and_then(|font| find_child(font, "size"));
    let size_mm = if let Some(size_node) = size_node {
        let size = size_node.as_list().unwrap_or_default();
        SizeMm {
            height: number_required(source, size.get(1), "text height")?,
            width: number_required(source, size.get(2), "text width")?,
        }
    } else {
        SizeMm {
            width: 1.0,
            height: 1.0,
        }
    };
    let thickness_mm = font
        .and_then(|font| find_child(font, "thickness"))
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .map(|value| number_required(source, Some(value), "text thickness"))
        .transpose()?
        .unwrap_or(0.15);
    let font_face = font
        .and_then(|font| find_child(font, "face"))
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .and_then(SExpr::as_text)
        .map(str::to_owned);
    let bold = font.is_some_and(|font| font.iter().any(|item| item.as_text() == Some("bold")));
    let italic = font.is_some_and(|font| font.iter().any(|item| item.as_text() == Some("italic")));
    let justify = find_child(effects, "justify").and_then(SExpr::as_list);
    let mirrored =
        justify.is_some_and(|items| items.iter().any(|item| item.as_text() == Some("mirror")));
    let line_spacing = font
        .and_then(|font| find_child(font, "line_spacing"))
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .map(|value| number_required(source, Some(value), "text line spacing"))
        .transpose()?;
    Ok((
        size_mm,
        thickness_mm,
        font_face,
        bold,
        italic,
        mirrored,
        line_spacing,
    ))
}

fn parse_graphic_width(source: &str, items: &[SExpr]) -> Result<f64, FootprintImportError> {
    if let Some(width) = find_child(items, "width") {
        return number_required(
            source,
            width.as_list().and_then(|values| values.get(1)),
            "graphic width",
        );
    }
    if let Some(stroke) = find_child(items, "stroke") {
        return number_required(
            source,
            find_child(stroke.as_list().unwrap_or_default(), "width")
                .and_then(|node| node.as_list())
                .and_then(|values| values.get(1)),
            "graphic stroke width",
        );
    }
    Err(FootprintImportError::at(
        source,
        items[0].start,
        "graphic has no width or stroke width",
    ))
}

fn parse_stroke_style(items: &[SExpr]) -> FootprintStrokeStyle {
    let Some(stroke) = find_child(items, "stroke").and_then(SExpr::as_list) else {
        return FootprintStrokeStyle::Default;
    };
    let Some(style) = find_child(stroke, "type")
        .and_then(SExpr::as_list)
        .and_then(|values| values.get(1))
        .and_then(SExpr::as_text)
    else {
        return FootprintStrokeStyle::Default;
    };
    match style {
        "default" => FootprintStrokeStyle::Default,
        "solid" => FootprintStrokeStyle::Solid,
        "dash" => FootprintStrokeStyle::Dashed,
        "dot" => FootprintStrokeStyle::Dotted,
        "dash_dot" => FootprintStrokeStyle::DashDot,
        "dash_dot_dot" => FootprintStrokeStyle::DashDotDot,
        other => FootprintStrokeStyle::Unsupported(other.to_string()),
    }
}

fn parse_filled(items: &[SExpr]) -> bool {
    find_child(items, "fill")
        .and_then(SExpr::as_list)
        .and_then(|values| values.get(1))
        .and_then(SExpr::as_text)
        .is_some_and(|value| matches!(value, "yes" | "solid"))
}

fn parse_points(source: &str, items: &[SExpr]) -> Result<Vec<PointMm>, FootprintImportError> {
    let points = find_child(items, "pts").ok_or_else(|| {
        FootprintImportError::at(source, items[0].start, "polygon has no pts field")
    })?;
    points
        .as_list()
        .unwrap_or_default()
        .iter()
        .skip(1)
        .map(|point| parse_point(source, point, "polygon point"))
        .collect()
}

fn required_point(
    source: &str,
    items: &[SExpr],
    name: &str,
) -> Result<PointMm, FootprintImportError> {
    let node = find_child(items, name).ok_or_else(|| {
        FootprintImportError::at(
            source,
            items[0].start,
            format!("graphic has no {name} field"),
        )
    })?;
    parse_point(source, node, name)
}

fn parse_position(
    source: &str,
    expression: &SExpr,
    label: &str,
) -> Result<(PointMm, f64), FootprintImportError> {
    let values = expression.as_list().unwrap_or_default();
    let point = PointMm {
        x: number_required(source, values.get(1), &format!("{label} x"))?,
        y: number_required(source, values.get(2), &format!("{label} y"))?,
    };
    let rotation = values
        .get(3)
        .map(|value| number_required(source, Some(value), &format!("{label} rotation")))
        .transpose()?
        .unwrap_or(0.0);
    Ok((point, rotation))
}

fn parse_point(
    source: &str,
    expression: &SExpr,
    label: &str,
) -> Result<PointMm, FootprintImportError> {
    let values = expression.as_list().unwrap_or_default();
    Ok(PointMm {
        x: number_required(source, values.get(1), &format!("{label} x"))?,
        y: number_required(source, values.get(2), &format!("{label} y"))?,
    })
}

fn parse_optional_number(
    source: &str,
    items: &[SExpr],
    key: &str,
) -> Result<Option<f64>, FootprintImportError> {
    find_child(items, key)
        .map(|node| number_required(source, node.as_list().and_then(|values| values.get(1)), key))
        .transpose()
}

fn number_required(
    source: &str,
    value: Option<&SExpr>,
    label: &str,
) -> Result<f64, FootprintImportError> {
    let Some(value) = value else {
        return Err(FootprintImportError::data(format!("{label} is missing")));
    };
    let raw = value.as_text().ok_or_else(|| {
        FootprintImportError::at(source, value.start, format!("{label} must be a number"))
    })?;
    raw.parse::<f64>().map_err(|_| {
        FootprintImportError::at(
            source,
            value.start,
            format!("{label} '{raw}' is not a number"),
        )
    })
}

fn find_child<'a>(items: &'a [SExpr], key: &str) -> Option<&'a SExpr> {
    items.iter().find(|item| {
        item.as_list()
            .and_then(|values| values.first())
            .and_then(SExpr::as_text)
            == Some(key)
    })
}

fn feature(
    source: &str,
    location: impl Into<String>,
    name: impl Into<String>,
    expression: &SExpr,
    blocks_fabrication: bool,
) -> UnsupportedFootprintFeature {
    UnsupportedFootprintFeature {
        location: location.into(),
        feature: name.into(),
        raw_expression: source
            .get(expression.start..expression.end)
            .unwrap_or_default()
            .to_string(),
        blocks_fabrication,
    }
}

fn split_tags(value: &str) -> Vec<String> {
    value
        .split_ascii_whitespace()
        .map(str::to_owned)
        .filter(|tag| !tag.is_empty())
        .collect()
}

#[derive(Debug)]
enum SExprKind {
    List(Vec<SExpr>),
    Atom(String),
    String(String),
}

#[derive(Debug)]
struct SExpr {
    kind: SExprKind,
    start: usize,
    end: usize,
}

impl SExpr {
    fn as_list(&self) -> Option<&[SExpr]> {
        match &self.kind {
            SExprKind::List(items) => Some(items),
            _ => None,
        }
    }

    fn as_text(&self) -> Option<&str> {
        match &self.kind {
            SExprKind::Atom(value) | SExprKind::String(value) => Some(value),
            SExprKind::List(_) => None,
        }
    }
}

struct SExprParser<'a> {
    source: &'a str,
    cursor: usize,
    node_count: usize,
}

impl<'a> SExprParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: 0,
            node_count: 0,
        }
    }

    fn parse_document(mut self) -> Result<SExpr, FootprintImportError> {
        self.skip_space_and_comments();
        let root = self.parse_expression(0)?;
        self.skip_space_and_comments();
        if self.cursor < self.source.len() {
            return Err(FootprintImportError::at(
                self.source,
                self.cursor,
                "unexpected content after the footprint expression",
            ));
        }
        Ok(root)
    }

    fn parse_expression(&mut self, depth: usize) -> Result<SExpr, FootprintImportError> {
        self.skip_space_and_comments();
        let start = self.cursor;
        if depth > MAX_NESTING_DEPTH {
            return Err(FootprintImportError::at(
                self.source,
                start,
                "S-expression nesting exceeds the supported limit",
            ));
        }
        if self.node_count >= MAX_SEXPR_NODES {
            return Err(FootprintImportError::at(
                self.source,
                start,
                "footprint has too many S-expression nodes",
            ));
        }
        self.node_count += 1;
        match self.peek_char() {
            Some('(') => self.parse_list(depth),
            Some(')') => Err(FootprintImportError::at(
                self.source,
                start,
                "unexpected closing parenthesis",
            )),
            Some('"') => self.parse_string(),
            Some(_) => self.parse_atom(),
            None => Err(FootprintImportError::at(
                self.source,
                start,
                "unexpected end of file",
            )),
        }
    }

    fn parse_list(&mut self, depth: usize) -> Result<SExpr, FootprintImportError> {
        let start = self.cursor;
        self.advance_char();
        let mut items = Vec::new();
        loop {
            self.skip_space_and_comments();
            match self.peek_char() {
                Some(')') => {
                    self.advance_char();
                    return Ok(SExpr {
                        kind: SExprKind::List(items),
                        start,
                        end: self.cursor,
                    });
                }
                None => {
                    return Err(FootprintImportError::at(
                        self.source,
                        start,
                        "unclosed list; expected ')'",
                    ));
                }
                _ => items.push(self.parse_expression(depth + 1)?),
            }
        }
    }

    fn parse_string(&mut self) -> Result<SExpr, FootprintImportError> {
        let start = self.cursor;
        self.advance_char();
        let mut value = String::new();
        loop {
            match self.advance_char() {
                Some('"') => {
                    return Ok(SExpr {
                        kind: SExprKind::String(value),
                        start,
                        end: self.cursor,
                    });
                }
                Some('\\') => match self.advance_char() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some(other) => {
                        value.push('\\');
                        value.push(other);
                    }
                    None => {
                        return Err(FootprintImportError::at(
                            self.source,
                            start,
                            "unterminated escape sequence in string",
                        ));
                    }
                },
                Some(character) => value.push(character),
                None => {
                    return Err(FootprintImportError::at(
                        self.source,
                        start,
                        "unterminated quoted string",
                    ));
                }
            }
        }
    }

    fn parse_atom(&mut self) -> Result<SExpr, FootprintImportError> {
        let start = self.cursor;
        while let Some(character) = self.peek_char() {
            if character.is_whitespace() || matches!(character, '(' | ')' | ';') {
                break;
            }
            self.advance_char();
        }
        if self.cursor == start {
            return Err(FootprintImportError::at(
                self.source,
                start,
                "expected an atom",
            ));
        }
        Ok(SExpr {
            kind: SExprKind::Atom(self.source[start..self.cursor].to_string()),
            start,
            end: self.cursor,
        })
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            while self.peek_char().is_some_and(char::is_whitespace) {
                self.advance_char();
            }
            if self.peek_char() == Some(';') {
                while let Some(character) = self.advance_char() {
                    if character == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source.get(self.cursor..)?.chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let character = self.peek_char()?;
        self.cursor += character.len_utf8();
        Some(character)
    }
}
