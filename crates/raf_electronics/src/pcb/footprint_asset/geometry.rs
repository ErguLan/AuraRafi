use super::types::{
    FootprintAsset, FootprintBoundsMm, FootprintDrillShape, FootprintGraphic,
    FootprintGraphicGeometry, FootprintPad, FootprintPadShape, PointMm, SizeMm,
};

impl FootprintPad {
    /// Returns the four corners of the pad's rotated rectangular envelope.
    /// Rounded, circular, and oval pads use the same conservative envelope.
    pub fn corners_mm(&self) -> [PointMm; 4] {
        let half_width = self.size_mm.width * 0.5;
        let half_height = self.size_mm.height * 0.5;
        [
            PointMm {
                x: -half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: half_height,
            },
            PointMm {
                x: -half_width,
                y: half_height,
            },
        ]
        .map(|point| self.to_footprint_coordinates(point))
    }

    /// Approximates the supported pad outline with a conservative polygon in
    /// footprint-local coordinates. Curved outlines use circumscribed chords:
    /// the maximum radial overrun is reported by `outline_tolerance_mm()`.
    /// Returns `None` for an unsupported KiCad pad shape.
    pub fn outline_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.outline_polygon_bound_mm(curve_segments, true)
    }

    /// Inscribed copper is used to prove connectivity; a conservative outer
    /// bound used for clearance must never manufacture a false connection.
    pub(crate) fn contact_outline_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.outline_polygon_bound_mm(curve_segments, false)
    }

    fn outline_polygon_bound_mm(&self, curve_segments: usize, outer: bool) -> Option<Vec<PointMm>> {
        let half_width = self.size_mm.width * 0.5;
        let half_height = self.size_mm.height * 0.5;
        let mut local = Vec::new();
        match &self.shape {
            FootprintPadShape::Rectangle => {
                local.extend([
                    PointMm {
                        x: -half_width,
                        y: -half_height,
                    },
                    PointMm {
                        x: half_width,
                        y: -half_height,
                    },
                    PointMm {
                        x: half_width,
                        y: half_height,
                    },
                    PointMm {
                        x: -half_width,
                        y: half_height,
                    },
                ]);
            }
            FootprintPadShape::Circle => {
                append_circle_points(&mut local, PointMm::ZERO, half_width, curve_segments, outer);
            }
            FootprintPadShape::Oval => {
                append_capsule_points(
                    &mut local,
                    self.size_mm.width,
                    self.size_mm.height,
                    curve_segments,
                    outer,
                );
            }
            FootprintPadShape::RoundRect { corner_ratio } => {
                let ratio = (*corner_ratio)?;
                if !ratio.is_finite() || !(0.0..=0.5).contains(&ratio) {
                    return None;
                }
                let radius = self.size_mm.width.min(self.size_mm.height) * ratio;
                append_round_rect_points(
                    &mut local,
                    self.size_mm.width,
                    self.size_mm.height,
                    radius,
                    curve_segments,
                    outer,
                );
            }
            FootprintPadShape::Unsupported { .. } => return None,
        }
        Some(
            local
                .into_iter()
                .map(|point| self.to_footprint_coordinates(point))
                .collect(),
        )
    }

    /// Largest radial overrun introduced by the conservative curve polygon, in
    /// millimeters. Rectangles and pads with invalid or absent shape settings
    /// return `0.0` here; call `validate()` before using such geometry.
    pub fn outline_tolerance_mm(&self, curve_segments: usize) -> f64 {
        let segments = curve_segments.clamp(8, 4096);
        let radius = self.size_mm.width.min(self.size_mm.height) * 0.5;
        match &self.shape {
            FootprintPadShape::Circle => {
                let half_step = std::f64::consts::PI / segments as f64;
                radius * (half_step.cos().recip() - 1.0)
            }
            FootprintPadShape::Oval => {
                let per_half = (segments / 2).max(4);
                let half_step = std::f64::consts::PI / (2.0 * per_half as f64);
                radius * (half_step.cos().recip() - 1.0)
            }
            FootprintPadShape::RoundRect {
                corner_ratio: Some(ratio),
            } if ratio.is_finite() && (0.0..=0.5).contains(ratio) => {
                let per_corner = (segments / 4).max(2);
                if *ratio == 0.0 {
                    0.0
                } else {
                    let half_step =
                        std::f64::consts::PI / (4.0 * per_corner.saturating_sub(1) as f64);
                    self.size_mm.width.min(self.size_mm.height)
                        * *ratio
                        * (half_step.cos().recip() - 1.0)
                }
            }
            _ => 0.0,
        }
    }

    /// Returns the drill outline, offset and rotated with its pad.
    pub fn drill_outline_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.drill_outline_bound_mm(curve_segments, false)
    }

    pub(crate) fn contact_drill_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.drill_outline_bound_mm(curve_segments, true)
    }

    fn drill_outline_bound_mm(&self, curve_segments: usize, outer: bool) -> Option<Vec<PointMm>> {
        let drill = self.drill.as_ref()?;
        let mut local = Vec::new();
        match &drill.shape {
            FootprintDrillShape::Circular { diameter_mm } => append_circle_points(
                &mut local,
                drill.offset_mm,
                *diameter_mm * 0.5,
                curve_segments,
                outer,
            ),
            FootprintDrillShape::Oval {
                width_mm,
                height_mm,
            } => {
                let mut outline = Vec::new();
                append_capsule_points(&mut outline, *width_mm, *height_mm, curve_segments, outer);
                local.extend(outline.into_iter().map(|point| PointMm {
                    x: point.x + drill.offset_mm.x,
                    y: point.y + drill.offset_mm.y,
                }));
            }
        }
        Some(
            local
                .into_iter()
                .map(|point| self.to_footprint_coordinates(point))
                .collect(),
        )
    }

    /// Maximum radial undercut from approximating a drill with an inscribed
    /// polygon, in millimeters. Copper calculations keep the hole slightly
    /// smaller so the modeled annular copper is conservative.
    pub fn drill_outline_tolerance_mm(&self, curve_segments: usize) -> f64 {
        let Some(drill) = &self.drill else {
            return 0.0;
        };
        let segments = curve_segments.clamp(8, 4096);
        match &drill.shape {
            FootprintDrillShape::Circular { diameter_mm } => {
                let radius = *diameter_mm * 0.5;
                radius * (1.0 - (std::f64::consts::PI / segments as f64).cos())
            }
            FootprintDrillShape::Oval {
                width_mm,
                height_mm,
            } => {
                let radius = (*width_mm).min(*height_mm) * 0.5;
                let per_half = (segments / 2).max(4);
                let half_step = std::f64::consts::PI / (2.0 * per_half as f64);
                radius * (1.0 - half_step.cos())
            }
        }
    }

    fn to_footprint_coordinates(&self, point: PointMm) -> PointMm {
        let radians = self.rotation_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        PointMm {
            x: self.position_mm.x + point.x * cos - point.y * sin,
            y: self.position_mm.y + point.x * sin + point.y * cos,
        }
    }
}

fn append_circle_points(
    points: &mut Vec<PointMm>,
    center: PointMm,
    radius: f64,
    segments: usize,
    conservative: bool,
) {
    let count = segments.clamp(8, 4096);
    let half_step = std::f64::consts::PI / count as f64;
    let polygon_radius = if conservative {
        radius / half_step.cos()
    } else {
        radius * half_step.cos()
    };
    points.reserve(count);
    for index in 0..count {
        let angle = std::f64::consts::TAU * index as f64 / count as f64;
        points.push(PointMm {
            x: center.x + polygon_radius * angle.cos(),
            y: center.y + polygon_radius * angle.sin(),
        });
    }
}

fn append_capsule_points(
    points: &mut Vec<PointMm>,
    width: f64,
    height: f64,
    segments: usize,
    conservative: bool,
) {
    let per_half = (segments.clamp(8, 4096) / 2).max(4);
    let radius = width.min(height) * 0.5;
    let half_step = std::f64::consts::PI / (2.0 * per_half as f64);
    let polygon_radius = if conservative {
        radius / half_step.cos()
    } else {
        radius * half_step.cos()
    };
    if width >= height {
        let center_x = (width - height) * 0.5;
        for index in 0..=per_half {
            let angle = -std::f64::consts::FRAC_PI_2
                + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: center_x + polygon_radius * angle.cos(),
                y: polygon_radius * angle.sin(),
            });
        }
        for index in 0..=per_half {
            let angle =
                std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: -center_x + polygon_radius * angle.cos(),
                y: polygon_radius * angle.sin(),
            });
        }
    } else {
        let center_y = (height - width) * 0.5;
        for index in 0..=per_half {
            let angle = 0.0 + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: polygon_radius * angle.cos(),
                y: center_y + polygon_radius * angle.sin(),
            });
        }
        for index in 0..=per_half {
            let angle =
                std::f64::consts::PI + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: polygon_radius * angle.cos(),
                y: -center_y + polygon_radius * angle.sin(),
            });
        }
    }
}

fn append_round_rect_points(
    points: &mut Vec<PointMm>,
    width: f64,
    height: f64,
    radius: f64,
    segments: usize,
    outer: bool,
) {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let true_radius = radius.min(half_width).min(half_height);
    let per_corner = (segments.clamp(8, 4096) / 4).max(2);
    let draw_radius = if true_radius > 0.0 && outer {
        true_radius / (std::f64::consts::PI / (4.0 * per_corner.saturating_sub(1) as f64)).cos()
    } else {
        true_radius
    };
    if true_radius <= 0.0 {
        points.extend([
            PointMm {
                x: -half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: half_height,
            },
            PointMm {
                x: -half_width,
                y: half_height,
            },
        ]);
        return;
    }
    let corners = [
        (
            half_width - true_radius,
            -half_height + true_radius,
            -std::f64::consts::FRAC_PI_2,
        ),
        (half_width - true_radius, half_height - true_radius, 0.0),
        (
            -half_width + true_radius,
            half_height - true_radius,
            std::f64::consts::FRAC_PI_2,
        ),
        (
            -half_width + true_radius,
            -half_height + true_radius,
            std::f64::consts::PI,
        ),
    ];
    for (center_x, center_y, start_angle) in corners {
        for index in 0..per_corner {
            let angle =
                start_angle + std::f64::consts::FRAC_PI_2 * index as f64 / (per_corner - 1) as f64;
            points.push(PointMm {
                x: center_x + draw_radius * angle.cos(),
                y: center_y + draw_radius * angle.sin(),
            });
        }
    }
}

impl FootprintAsset {
    /// Returns the axis-aligned bounds of all pads and captured graphics.
    pub fn bounds_mm(&self) -> Option<FootprintBoundsMm> {
        let mut bounds = BoundsAccumulator::default();
        for pad in &self.pads {
            for corner in pad.corners_mm() {
                bounds.include(corner);
            }
        }
        for graphic in &self.graphics {
            bounds.include_graphic(graphic);
        }
        bounds.finish()
    }

    /// Returns the width and height of `bounds_mm()`.
    pub fn dimensions_mm(&self) -> Option<SizeMm> {
        self.bounds_mm().map(FootprintBoundsMm::size_mm)
    }
}

#[derive(Default)]
struct BoundsAccumulator {
    min: Option<PointMm>,
    max: Option<PointMm>,
}

impl BoundsAccumulator {
    fn include(&mut self, point: PointMm) {
        if !point.is_finite() {
            return;
        }
        self.min = Some(match self.min {
            Some(min) => PointMm {
                x: min.x.min(point.x),
                y: min.y.min(point.y),
            },
            None => point,
        });
        self.max = Some(match self.max {
            Some(max) => PointMm {
                x: max.x.max(point.x),
                y: max.y.max(point.y),
            },
            None => point,
        });
    }

    fn include_with_radius(&mut self, point: PointMm, radius: f64) {
        if !radius.is_finite() || radius < 0.0 {
            return;
        }
        self.include(PointMm {
            x: point.x - radius,
            y: point.y - radius,
        });
        self.include(PointMm {
            x: point.x + radius,
            y: point.y + radius,
        });
    }

    fn include_graphic(&mut self, graphic: &FootprintGraphic) {
        let half_stroke = match &graphic.geometry {
            FootprintGraphicGeometry::Line { width_mm, .. }
            | FootprintGraphicGeometry::Rectangle { width_mm, .. }
            | FootprintGraphicGeometry::Circle { width_mm, .. }
            | FootprintGraphicGeometry::Arc { width_mm, .. }
            | FootprintGraphicGeometry::Polygon { width_mm, .. } => width_mm * 0.5,
            FootprintGraphicGeometry::Text { thickness_mm, .. } => thickness_mm * 0.5,
            FootprintGraphicGeometry::Unsupported { .. } => return,
        };
        match &graphic.geometry {
            FootprintGraphicGeometry::Line {
                start_mm, end_mm, ..
            }
            | FootprintGraphicGeometry::Rectangle {
                start_mm, end_mm, ..
            } => {
                self.include_with_radius(*start_mm, half_stroke);
                self.include_with_radius(*end_mm, half_stroke);
            }
            FootprintGraphicGeometry::Circle {
                center_mm,
                radius_mm,
                ..
            } => self.include_with_radius(*center_mm, radius_mm + half_stroke),
            FootprintGraphicGeometry::Arc {
                start_mm,
                mid_mm,
                end_mm,
                ..
            } => {
                self.include_with_radius(*start_mm, half_stroke);
                self.include_with_radius(*mid_mm, half_stroke);
                self.include_with_radius(*end_mm, half_stroke);
            }
            FootprintGraphicGeometry::Polygon { points_mm, .. } => {
                for point in points_mm {
                    self.include_with_radius(*point, half_stroke);
                }
            }
            FootprintGraphicGeometry::Text {
                position_mm,
                rotation_degrees,
                size_mm,
                ..
            } => {
                let radians = rotation_degrees.to_radians();
                let (sin, cos) = radians.sin_cos();
                let half_width = size_mm.width * 0.5 + half_stroke;
                let half_height = size_mm.height * 0.5 + half_stroke;
                for (x, y) in [
                    (-half_width, -half_height),
                    (half_width, -half_height),
                    (half_width, half_height),
                    (-half_width, half_height),
                ] {
                    self.include(PointMm {
                        x: position_mm.x + x * cos - y * sin,
                        y: position_mm.y + x * sin + y * cos,
                    });
                }
            }
            FootprintGraphicGeometry::Unsupported { .. } => {}
        }
    }

    fn finish(self) -> Option<FootprintBoundsMm> {
        Some(FootprintBoundsMm {
            min: self.min?,
            max: self.max?,
        })
    }
}
