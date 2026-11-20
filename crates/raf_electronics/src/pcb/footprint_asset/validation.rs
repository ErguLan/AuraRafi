use super::library::safe_asset_stem;
use super::types::{
    FootprintAsset, FootprintDrill, FootprintDrillShape, FootprintGraphic,
    FootprintGraphicGeometry, FootprintLayer, FootprintPad, FootprintPadShape,
    FootprintPadTechnology, FootprintStrokeStyle, FootprintValidationIssue, PointMm, SizeMm,
};
use super::{ASSET_SCHEMA_VERSION, MAX_FOOTPRINT_SOURCE_BYTES, MAX_GRAPHICS_PER_ASSET, MAX_PADS_PER_ASSET};

impl FootprintAsset {
    /// Checks the numeric and structural invariants required by this model.
    pub fn validate(&self) -> Vec<FootprintValidationIssue> {
        let mut issues = Vec::new();
        if self.schema_version != ASSET_SCHEMA_VERSION {
            issue(
                &mut issues,
                "schema_version",
                format!("unsupported footprint asset schema {}", self.schema_version),
            );
        }
        if self.name.trim().is_empty()
            || safe_asset_stem(&self.name).as_deref() != Some(self.id.as_str())
        {
            issue(
                &mut issues,
                "identity",
                "asset id must be the safe normalized stem of a non-empty name",
            );
        }
        if self.source_text.len() > MAX_FOOTPRINT_SOURCE_BYTES {
            issue(
                &mut issues,
                "source_text",
                "source text exceeds the supported size limit",
            );
        }
        if self.pads.len() > MAX_PADS_PER_ASSET {
            issue(
                &mut issues,
                "pads",
                format!("asset exceeds the {} pad limit", MAX_PADS_PER_ASSET),
            );
        } else {
            for (index, pad) in self.pads.iter().enumerate() {
                let location = format!("pads[{index}] ({})", pad.number);
                validate_pad(pad, &location, &mut issues);
            }
        }
        if self.graphics.len() > MAX_GRAPHICS_PER_ASSET {
            issue(
                &mut issues,
                "graphics",
                format!(
                    "asset exceeds the {} graphics limit",
                    MAX_GRAPHICS_PER_ASSET
                ),
            );
        } else {
            for (index, graphic) in self.graphics.iter().enumerate() {
                let location = format!("graphics[{index}]");
                validate_graphic(graphic, &location, &mut issues);
            }
        }
        validate_optional_finite(
            self.rules.solder_mask_margin_mm,
            "rules.solder_mask_margin_mm",
            &mut issues,
        );
        validate_optional_finite(
            self.rules.solder_paste_margin_mm,
            "rules.solder_paste_margin_mm",
            &mut issues,
        );
        validate_optional_ratio(
            self.rules.solder_paste_ratio,
            "rules.solder_paste_ratio",
            &mut issues,
        );
        validate_optional_nonnegative(self.rules.clearance_mm, "rules.clearance_mm", &mut issues);
        validate_optional_nonnegative(
            self.rules.thermal_width_mm,
            "rules.thermal_width_mm",
            &mut issues,
        );
        validate_optional_nonnegative(
            self.rules.thermal_gap_mm,
            "rules.thermal_gap_mm",
            &mut issues,
        );
        issues
    }

    /// Returns false when any geometry is invalid or any physical source data
    /// remains unsupported.
    pub fn is_fabrication_ready(&self) -> bool {
        self.validate().is_empty()
            && !self
                .unsupported_features
                .iter()
                .any(|feature| feature.blocks_fabrication)
            && !self.pads.iter().any(|pad| {
                matches!(
                    &pad.shape,
                    FootprintPadShape::RoundRect { corner_ratio: None } | FootprintPadShape::Unsupported{..}
                )
                || matches!(pad.technology,FootprintPadTechnology::Unsupported{..})
                || pad.layers.iter().any(|layer|matches!(layer,FootprintLayer::InnerCopper(_)|FootprintLayer::Other(_)))
            })
            // Free copper, mask, paste and footprint-defined board cutouts need
            // their own connectivity/rules contract. Never silently omit them.
            && !self.graphics.iter().any(|graphic|matches!(graphic.layer,
                FootprintLayer::FrontCopper|FootprintLayer::BackCopper|FootprintLayer::AllCopper|FootprintLayer::InnerCopper(_)
                |FootprintLayer::FrontMask|FootprintLayer::BackMask|FootprintLayer::AllMask
                |FootprintLayer::FrontPaste|FootprintLayer::BackPaste|FootprintLayer::AllPaste|FootprintLayer::BoardOutline))
    }
}

pub(super) fn validate_pad(
    pad: &FootprintPad,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if pad.number.trim().is_empty()
        && !matches!(
            &pad.technology,
            FootprintPadTechnology::NonPlatedThroughHole
        )
    {
        issue(issues, format!("{location}.number"), "pad number is empty");
    }
    if !pad.position_mm.is_finite() {
        issue(
            issues,
            format!("{location}.position_mm"),
            "coordinates must be finite",
        );
    }
    if !pad.rotation_degrees.is_finite() {
        issue(
            issues,
            format!("{location}.rotation_degrees"),
            "rotation must be finite",
        );
    }
    if !pad.size_mm.is_positive_finite() {
        issue(
            issues,
            format!("{location}.size_mm"),
            "width and height must be finite and greater than zero",
        );
    }
    if pad.layers.is_empty() {
        issue(
            issues,
            format!("{location}.layers"),
            "at least one layer is required",
        );
    }
    if matches!(&pad.shape, FootprintPadShape::Circle)
        && pad.size_mm.width.is_finite()
        && pad.size_mm.height.is_finite()
        && (pad.size_mm.width - pad.size_mm.height).abs() > 1e-6
    {
        issue(
            issues,
            format!("{location}.size_mm"),
            "a circular pad must have equal width and height",
        );
    }
    match &pad.shape {
        FootprintPadShape::RoundRect {
            corner_ratio: Some(value),
        } if !value.is_finite() || !(0.0..=0.5).contains(value) => issue(
            issues,
            format!("{location}.shape.corner_ratio"),
            "corner ratio must be finite and between zero and 0.5",
        ),
        FootprintPadShape::RoundRect { corner_ratio: None } => {}
        FootprintPadShape::RoundRect {
            corner_ratio: Some(_),
        } => {}
        _ => {}
    }
    validate_optional_finite(
        pad.solder_mask_margin_mm,
        &format!("{location}.solder_mask_margin_mm"),
        issues,
    );
    validate_optional_finite(
        pad.solder_paste_margin_mm,
        &format!("{location}.solder_paste_margin_mm"),
        issues,
    );
    validate_optional_ratio(
        pad.solder_paste_margin_ratio,
        &format!("{location}.solder_paste_margin_ratio"),
        issues,
    );

    let has_front_copper = pad.layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::FrontCopper | FootprintLayer::AllCopper
        )
    });
    let has_back_copper = pad.layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::BackCopper | FootprintLayer::AllCopper
        )
    });
    let has_any_copper = pad.layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::FrontCopper
                | FootprintLayer::BackCopper
                | FootprintLayer::InnerCopper(_)
                | FootprintLayer::AllCopper
        )
    });
    if matches!(&pad.technology, FootprintPadTechnology::Smd) && !has_any_copper {
        issue(
            issues,
            format!("{location}.layers"),
            "an SMD pad must include a copper layer",
        );
    }
    if matches!(&pad.technology, FootprintPadTechnology::PlatedThroughHole)
        && !(has_front_copper && has_back_copper)
    {
        issue(
            issues,
            format!("{location}.layers"),
            "a plated through-hole pad must include front and back copper",
        );
    }
    // KiCad NPTH pads commonly declare *.Cu to identify the pierced layers.
    // Their technology excludes them from copper; this is not an annulus.
    match (&pad.technology, &pad.drill) {
        (FootprintPadTechnology::Smd, Some(_)) => issue(
            issues,
            format!("{location}.drill"),
            "an SMD pad cannot define a drill",
        ),
        (FootprintPadTechnology::PlatedThroughHole, None) => issue(
            issues,
            format!("{location}.drill"),
            "a plated through-hole pad requires a drill",
        ),
        (FootprintPadTechnology::NonPlatedThroughHole, None) => issue(
            issues,
            format!("{location}.drill"),
            "a non-plated through-hole pad requires a drill",
        ),
        (_, Some(drill)) => validate_drill(drill, &format!("{location}.drill"), issues),
        _ => {}
    }
    if matches!(pad.technology, FootprintPadTechnology::PlatedThroughHole)
        && pad.size_mm.is_positive_finite()
        && pad.rotation_degrees.is_finite()
    {
        if let (Some(mut copper), Some(drill)) = (
            pad.contact_outline_polygon_mm(128),
            pad.contact_drill_polygon_mm(128),
        ) {
            if let Some(first) = copper.first().copied() {
                copper.push(first);
            }
            let copper = copper
                .iter()
                .map(|p| glam::Vec2::new(p.x as f32, p.y as f32))
                .collect::<Vec<_>>();
            if drill.iter().any(|p| {
                !super::super::geometry::point_in_polygon(glam::Vec2::new(p.x as f32, p.y as f32), &copper)
            }) {
                issue(
                    issues,
                    format!("{location}.drill"),
                    "plated drill must fit completely inside its copper pad",
                );
            }
        }
    }
}

fn validate_drill(
    drill: &FootprintDrill,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if !drill.offset_mm.is_finite() {
        issue(
            issues,
            format!("{location}.offset_mm"),
            "drill offset coordinates must be finite",
        );
    }
    match &drill.shape {
        FootprintDrillShape::Circular { diameter_mm }
            if !diameter_mm.is_finite() || *diameter_mm <= 0.0 =>
        {
            issue(
                issues,
                format!("{location}.diameter_mm"),
                "drill diameter must be finite and greater than zero",
            );
        }
        FootprintDrillShape::Oval {
            width_mm,
            height_mm,
        } if !(SizeMm {
            width: *width_mm,
            height: *height_mm,
        })
        .is_positive_finite() =>
        {
            issue(
                issues,
                format!("{location}.size_mm"),
                "oval drill width and height must be finite and greater than zero",
            )
        }
        _ => {}
    }
}

pub(super) fn validate_graphic(
    graphic: &FootprintGraphic,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    match &graphic.geometry {
        FootprintGraphicGeometry::Line {
            start_mm,
            end_mm,
            width_mm,
        } => {
            validate_graphic_point(*start_mm, location, "start_mm", issues);
            validate_graphic_point(*end_mm, location, "end_mm", issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Rectangle {
            start_mm,
            end_mm,
            width_mm,
            ..
        } => {
            validate_graphic_point(*start_mm, location, "start_mm", issues);
            validate_graphic_point(*end_mm, location, "end_mm", issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Circle {
            center_mm,
            radius_mm,
            width_mm,
            ..
        } => {
            validate_graphic_point(*center_mm, location, "center_mm", issues);
            validate_positive(*radius_mm, &format!("{location}.radius_mm"), issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Arc {
            start_mm,
            mid_mm,
            end_mm,
            width_mm,
        } => {
            validate_graphic_point(*start_mm, location, "start_mm", issues);
            validate_graphic_point(*mid_mm, location, "mid_mm", issues);
            validate_graphic_point(*end_mm, location, "end_mm", issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Polygon {
            points_mm,
            width_mm,
            ..
        } => {
            if points_mm.len() < 3 {
                issue(
                    issues,
                    format!("{location}.points_mm"),
                    "a polygon requires at least three points",
                );
            }
            for (index, point) in points_mm.iter().enumerate() {
                validate_graphic_point(*point, location, &format!("points_mm[{index}]"), issues);
            }
            validate_nonnegative(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Text {
            position_mm,
            rotation_degrees,
            size_mm,
            thickness_mm,
            line_spacing,
            ..
        } => {
            validate_graphic_point(*position_mm, location, "position_mm", issues);
            if !rotation_degrees.is_finite() {
                issue(
                    issues,
                    format!("{location}.rotation_degrees"),
                    "rotation must be finite",
                );
            }
            if !size_mm.is_positive_finite() {
                issue(
                    issues,
                    format!("{location}.size_mm"),
                    "text width and height must be finite and greater than zero",
                );
            }
            validate_positive(*thickness_mm, &format!("{location}.thickness_mm"), issues);
            if let Some(value) = line_spacing {
                validate_positive(*value, &format!("{location}.line_spacing"), issues);
            }
        }
        FootprintGraphicGeometry::Unsupported { .. } => {}
    }
    if let FootprintStrokeStyle::Unsupported(style) = &graphic.style {
        issue(
            issues,
            format!("{location}.style"),
            format!("unsupported stroke style '{style}'"),
        );
    }
}

fn validate_graphic_point(
    point: PointMm,
    location: &str,
    field: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if !point.is_finite() {
        issue(
            issues,
            format!("{location}.{field}"),
            "coordinates must be finite",
        );
    }
}

fn validate_optional_nonnegative(
    value: Option<f64>,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if let Some(value) = value {
        validate_nonnegative(value, location, issues);
    }
}

fn validate_optional_ratio(
    value: Option<f64>,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if let Some(value) = value {
        if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
            issue(
                issues,
                location,
                "paste ratio must be finite and between -1 and 1",
            );
        }
    }
}

fn validate_optional_finite(
    value: Option<f64>,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if value.is_some_and(|v| !v.is_finite()) {
        issue(issues, location, "margin must be finite");
    }
}

fn validate_positive(value: f64, location: &str, issues: &mut Vec<FootprintValidationIssue>) {
    if !value.is_finite() || value <= 0.0 {
        issue(
            issues,
            location,
            "value must be finite and greater than zero",
        );
    }
}

fn validate_nonnegative(value: f64, location: &str, issues: &mut Vec<FootprintValidationIssue>) {
    if !value.is_finite() || value < 0.0 {
        issue(issues, location, "value must be finite and non-negative");
    }
}

fn issue(
    issues: &mut Vec<FootprintValidationIssue>,
    location: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(FootprintValidationIssue {
        location: location.into(),
        message: message.into(),
    });
}
