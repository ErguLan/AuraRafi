//! Focused geometry, coverage and DPI checks for the RafUI paint encoder.
//!
//! These tests inspect the CPU-produced overlay stream, which is the same data
//! every native executor consumes. They do not claim native pixel appearance.

use glam::Vec2;

use crate::{backends::recorder::Recorder, Error, Texture, Vertex, Workload};

use super::*;

const LIMIT: u64 = u64::MAX;
const OUTPUT: [u32; 2] = [1000, 700];
const FULL: [f32; 4] = [0., 0., 1., 1.];

fn open_recorder() -> ProductDevice<Recorder> {
    let mut device =
        ProductDevice::new(Recorder::default(), GraphicsMemoryBudget::potato()).unwrap();
    device.configure([64, 64], Output::Capture).unwrap();
    device
}

fn encode(packet: ProductPacket) -> Workload {
    super::encode::encode(packet, LIMIT).unwrap().workload
}

fn quad(rect: [f32; 4], color: [f32; 4]) -> PaintQuad {
    PaintQuad {
        rect,
        clip: FULL,
        uv_rect: [0., 0., 1., 1.],
        color,
    }
}

fn white() -> Texture {
    Texture {
        size: [1, 1],
        rgba: vec![255; 4].into(),
        mipmaps: false,
    }
}

/// Recovers the normalized top-left coordinate of an overlay vertex.
fn normalized(vertex: &Vertex) -> Vec2 {
    Vec2::new(
        (vertex.position[0] + 1.) * 0.5,
        (1. - vertex.position[1]) * 0.5,
    )
}

fn physical(vertex: &Vertex, output: [u32; 2]) -> Vec2 {
    normalized(vertex) * Vec2::new(output[0] as f32, output[1] as f32)
}

fn with_color<'a>(workload: &'a Workload, color: [f32; 4]) -> impl Iterator<Item = &'a Vertex> {
    workload.vertices.iter().filter(move |vertex| {
        vertex
            .rgba
            .iter()
            .zip(color)
            .all(|(value, expected)| (value - expected).abs() < 1e-5)
    })
}

#[test]
fn rafui_matrix_rows_report_their_real_coverage() {
    for feature in [
        Feature::Solid,
        Feature::Stroke,
        Feature::Text,
        Feature::Image,
        Feature::RoundedClips,
        Feature::Gradient,
        Feature::ImageFit,
        Feature::Feather,
        Feature::TextAtlasPatch,
    ] {
        let row = COVERAGE
            .iter()
            .find(|(name, _)| *name == feature)
            .map(|(_, coverage)| *coverage);
        assert_eq!(row, Some(Coverage::Supported), "{}", feature.name());
    }
    assert_eq!(
        COVERAGE
            .iter()
            .find(|(name, _)| *name == Feature::CustomPass)
            .map(|(_, coverage)| *coverage),
        Some(Coverage::Unsupported)
    );
}

#[test]
fn per_corner_radii_and_per_side_borders_emit_exact_geometry() {
    let fill = [0.2, 0.4, 0.6, 1.];
    let top = [0.9, 0.1, 0.1, 1.];
    let right = [0.1, 0.9, 0.1, 1.];
    let bottom = [0.1, 0.1, 0.9, 1.];
    let left = [0.9, 0.9, 0.1, 1.];
    let style = PaintStyle::rounded(0.)
        .with_radii([[0.05, 0.02], [0.; 2], [0.; 2], [0.; 2]])
        .with_border(
            PaintBorder::per_side([0.01, 0.02, 0.03, 0.04], [top, right, bottom, left])
                .with_side(Side::Top, top),
        );
    let workload = encode(ProductPacket {
        textures: vec![white()],
        commands: vec![Command::StyledPaint {
            paint: Paint::Solid(quad([0.2, 0.2, 0.4, 0.3], fill)),
            style: Box::new(style),
        }],
        ..ProductPacket::default()
    });

    // Elliptical top-left radius: the top edge starts at x = 0.2 + 0.05 and the
    // left edge starts at y = 0.2 + 0.02.
    let top_edge = workload
        .vertices
        .iter()
        .filter(|vertex| (normalized(vertex).y - 0.2).abs() < 1e-5)
        .map(|vertex| normalized(vertex).x)
        .fold(f32::INFINITY, f32::min);
    assert!((top_edge - 0.25).abs() < 1e-4, "{top_edge}");
    let left_edge = workload
        .vertices
        .iter()
        .filter(|vertex| (normalized(vertex).x - 0.2).abs() < 1e-5)
        .map(|vertex| normalized(vertex).y)
        .fold(f32::INFINITY, f32::min);
    assert!((left_edge - 0.22).abs() < 1e-4, "{left_edge}");

    for color in [top, right, bottom, left] {
        assert!(with_color(&workload, color).next().is_some());
    }
    // The top border keeps its own width: outer y = 0.2, inner y = 0.21.
    let inner = with_color(&workload, top)
        .map(|vertex| normalized(vertex).y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((inner - 0.21).abs() < 1e-5, "{inner}");
    assert!(with_color(&workload, fill).next().is_some());
}

#[test]
fn feather_uses_a_bounded_kernel_with_a_transparent_outer_edge() {
    let color = [0.3, 0.3, 0.3, 0.8];
    let workload = encode(ProductPacket {
        textures: vec![white()],
        commands: vec![Command::StyledPaint {
            paint: Paint::Solid(quad([0.25, 0.25, 0.5, 0.5], color)),
            style: Box::new(PaintStyle::default().with_softness(0.02, 0.02)),
        }],
        ..ProductPacket::default()
    });

    let mut alphas = Vec::new();
    for vertex in &workload.vertices {
        let point = normalized(vertex);
        assert!(point.x >= 0.24 - 1e-4 && point.x <= 0.76 + 1e-4);
        assert!(point.y >= 0.24 - 1e-4 && point.y <= 0.76 + 1e-4);
        alphas.push(vertex.rgba[3]);
    }
    assert!(alphas.iter().any(|alpha| *alpha < 1e-5));
    assert!(alphas
        .iter()
        .any(|alpha| (*alpha - 0.8).abs() < 1e-5));
    let bands = alphas
        .iter()
        .filter(|alpha| **alpha > 1e-5 && **alpha < 0.8 - 1e-5)
        .count();
    assert!(bands >= 3, "{bands}");
    // Four kernel bands over at most 36 outline segments.
    assert!(workload.vertices.len() / 3 <= 4 * 36 * 2 + 36);
}

#[test]
fn nested_rounded_clips_intersect_in_declared_order() {
    let innermost = RoundedClip::uniform([0.3, 0.3, 0.4, 0.4], 0.05);
    let middle = RoundedClip::uniform([0.2, 0.2, 0.6, 0.6], 0.04);
    let outer = RoundedClip::uniform([0.1, 0.1, 0.8, 0.8], 0.03);
    let workload = encode(ProductPacket {
        textures: vec![white()],
        commands: vec![Command::StyledPaint {
            paint: Paint::Solid(quad([0., 0., 1., 1.], [1., 1., 1., 1.])),
            style: Box::new(
                PaintStyle::default().with_clips([outer, middle, innermost]),
            ),
        }],
        ..ProductPacket::default()
    });
    for vertex in &workload.vertices {
        let point = normalized(vertex);
        assert!(point.x >= 0.3 - 1e-4 && point.x <= 0.7 + 1e-4);
        assert!(point.y >= 0.3 - 1e-4 && point.y <= 0.7 + 1e-4);
        // Rounded corner: nothing may sit outside the innermost arc.
        let center = Vec2::new(0.35, 0.35);
        if point.x < 0.35 && point.y < 0.35 {
            assert!(point.distance(center) >= 0.05 - 1e-3);
        }
    }

    let too_deep = PaintStyle::default().with_clips(
        std::iter::repeat(RoundedClip::uniform([0.1, 0.1, 0.8, 0.8], 0.)).take(9),
    );
    let result = super::encode::encode(
        ProductPacket {
            textures: vec![white()],
            commands: vec![Command::StyledPaint {
                paint: Paint::Solid(quad([0., 0., 1., 1.], [1.; 4])),
                style: Box::new(too_deep),
            }],
            ..ProductPacket::default()
        },
        LIMIT,
    );
    assert!(matches!(result, Err(Error::Invalid(_))));
}

#[test]
fn linear_gradients_drive_fills_and_strokes() {
    let start = [1., 0., 0., 1.];
    let end = [0., 0., 1., 1.];
    let gradient = LinearGradient::vertical(start, end);
    let workload = encode(ProductPacket {
        textures: vec![white()],
        commands: vec![
            Command::StyledPaint {
                paint: Paint::Solid(quad([0.2, 0.2, 0.3, 0.3], [1.; 4])),
                style: Box::new(PaintStyle::default().with_gradient(gradient)),
            },
            Command::StyledPaint {
                paint: Paint::Stroke(PaintStroke {
                    start: [0.2, 0.8],
                    end: [0.8, 0.8],
                    width: [0.02, 0.02],
                    clip: FULL,
                    color: [1.; 4],
                }),
                style: Box::new(PaintStyle::default().with_gradient(gradient)),
            },
        ],
        ..ProductPacket::default()
    });
    let top = workload
        .vertices
        .iter()
        .find(|vertex| (normalized(vertex).y - 0.2).abs() < 1e-4)
        .map(|vertex| vertex.rgba)
        .expect("gradient top edge");
    assert!(top[0] > 0.99 && top[2] < 1e-4, "{top:?}");
    let bottom = workload
        .vertices
        .iter()
        .find(|vertex| (normalized(vertex).y - 0.5).abs() < 1e-4)
        .map(|vertex| vertex.rgba)
        .expect("gradient bottom edge");
    assert!(bottom[2] > 0.99 && bottom[0] < 1e-4, "{bottom:?}");
    // The stroke reuses the same gradient over its own bounds.
    let stroke = workload
        .vertices
        .iter()
        .filter(|vertex| normalized(vertex).y > 0.75)
        .map(|vertex| (normalized(vertex).y, vertex.rgba))
        .collect::<Vec<_>>();
    let lightest = stroke
        .iter()
        .max_by(|a, b| a.1[0].total_cmp(&b.1[0]))
        .expect("stroke vertices");
    let bluest = stroke
        .iter()
        .max_by(|a, b| a.1[2].total_cmp(&b.1[2]))
        .expect("stroke vertices");
    assert!(
        lightest.1[0] > bluest.1[0] + 0.5,
        "{:?} {:?}",
        lightest.1,
        bluest.1
    );
}

#[test]
fn image_fit_and_nine_slice_emit_expected_patches() {
    let texture = Texture {
        size: [200, 100],
        rgba: vec![255; 200 * 100 * 4].into(),
        mipmaps: true,
    };
    let layout = |fit| {
        encode(ProductPacket {
            textures: vec![texture.clone()],
            commands: vec![Command::StyledPaint {
                paint: Paint::Image {
                    quad: quad([0., 0., 0.2, 0.2], [1.; 4]),
                    texture: 0,
                },
                style: Box::new(PaintStyle::default().with_image(ImageLayout::new(fit, OUTPUT))),
            }],
            ..ProductPacket::default()
        })
    };

    let stretched = layout(ImageFit::Stretch);
    assert_eq!(stretched.vertices.len(), 6);
    let stretched_rect = bounds(&stretched);
    assert!((stretched_rect[2] - 0.2).abs() < 1e-4, "{stretched_rect:?}");
    assert!((stretched_rect[3] - 0.2).abs() < 1e-4, "{stretched_rect:?}");

    let contained = layout(ImageFit::Contain);
    let contained_rect = bounds(&contained);
    assert!(
        (contained_rect[3] - contained_rect[1] - 100. / 700.).abs() < 1e-4,
        "{contained_rect:?}"
    );
    assert!(
        (contained_rect[1] - (0.2 - 100. / 700.) * 0.5).abs() < 1e-4,
        "{contained_rect:?}"
    );
    assert_eq!(contained.vertices.len(), 6);

    // Cover overflows the box and is cropped to it, keeping the source UV
    // offset that the crop removed.
    let covered = layout(ImageFit::Cover);
    let covered_rect = bounds(&covered);
    assert!((covered_rect[2] - 0.2).abs() < 1e-4, "{covered_rect:?}");
    assert!((covered_rect[3] - 0.2).abs() < 1e-4, "{covered_rect:?}");
    let left = covered
        .vertices
        .iter()
        .min_by(|a, b| normalized(a).x.total_cmp(&normalized(b).x))
        .expect("cover vertex");
    assert!((left.uv[0] - 1. / 7.).abs() < 1e-4, "{:?}", left.uv);

    let sliced = encode(ProductPacket {
        textures: vec![Texture {
            size: [32, 32],
            rgba: vec![255; 32 * 32 * 4].into(),
            mipmaps: true,
        }],
        commands: vec![Command::StyledPaint {
            paint: Paint::Image {
                quad: quad([0., 0., 0.5, 0.5], [1.; 4]),
                texture: 0,
            },
            style: Box::new(PaintStyle::default().with_image(ImageLayout::sliced(
                [10., 10., 10., 10.],
                OUTPUT,
            ))),
        }],
        ..ProductPacket::default()
    });
    assert_eq!(sliced.vertices.len(), 9 * 6);
    for vertex in &sliced.vertices {
        let point = normalized(vertex);
        assert!(point.x >= -1e-4 && point.x <= 0.5 + 1e-4);
        assert!(point.y >= -1e-4 && point.y <= 0.5 + 1e-4);
        assert!(vertex.uv[0] >= -1e-4 && vertex.uv[0] <= 1. + 1e-4);
        assert!(vertex.uv[1] >= -1e-4 && vertex.uv[1] <= 1. + 1e-4);
    }
}

fn bounds(workload: &Workload) -> [f32; 4] {
    workload
        .vertices
        .iter()
        .map(normalized)
        .fold([f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY], |mut rect, point| {
            rect[0] = rect[0].min(point.x);
            rect[1] = rect[1].min(point.y);
            rect[2] = rect[2].max(point.x);
            rect[3] = rect[3].max(point.y);
            rect
        })
}

#[test]
fn unsupported_combinations_reject_explicitly() {
    let sliced_feather = super::encode::encode(
        ProductPacket {
            textures: vec![Texture {
                size: [32, 32],
                rgba: vec![255; 32 * 32 * 4].into(),
                mipmaps: true,
            }],
            commands: vec![Command::StyledPaint {
                paint: Paint::Image {
                    quad: quad([0., 0., 0.4, 0.4], [1.; 4]),
                    texture: 0,
                },
                style: Box::new(
                    PaintStyle::default()
                        .with_softness(0.01, 0.01)
                        .with_image(ImageLayout::sliced([8., 8., 8., 8.], OUTPUT)),
                ),
            }],
            ..ProductPacket::default()
        },
        LIMIT,
    );
    assert!(matches!(
        sliced_feather,
        Err(Error::Unsupported("feather on a fitted or sliced image"))
    ));

    let stroke_border = super::encode::encode(
        ProductPacket {
            textures: vec![white()],
            commands: vec![Command::StyledPaint {
                paint: Paint::Stroke(PaintStroke {
                    start: [0.2, 0.2],
                    end: [0.8, 0.8],
                    width: [0.02, 0.02],
                    clip: FULL,
                    color: [1.; 4],
                }),
                style: Box::new(PaintStyle::default().with_border(PaintBorder::outline(
                    0.01,
                    [1.; 4],
                ))),
            }],
            ..ProductPacket::default()
        },
        LIMIT,
    );
    assert!(matches!(
        stroke_border,
        Err(Error::Unsupported("stroke corner radii or per-side borders"))
    ));
}

#[test]
fn dpi_fixture_keeps_stroke_width_radius_and_text_sharp() {
    const STROKE: f32 = 1.5;
    const RADIUS: f32 = 6.;
    const BORDER: f32 = 1.;
    const GLYPH: f32 = 8.;
    let panel_fill = [0.2, 0.3, 0.4, 1.];
    let panel_border = [0.9, 0.8, 0.1, 1.];
    let stroke_color = [0.1, 0.9, 0.2, 1.];
    let text_color = [0.95, 0.95, 0.95, 1.];
    let clip_color = [0.8, 0.2, 0.9, 1.];

    for scale in [1.0_f32, 1.25, 1.5, 2.0] {
        let output = [
            (OUTPUT[0] as f32 * scale) as u32,
            (OUTPUT[1] as f32 * scale) as u32,
        ];
        // Logical points converted to normalized target units, honoring the
        // non-square normalized space on each axis.
        let nx = |points: f32| points * scale / output[0] as f32;
        let ny = |points: f32| points * scale / output[1] as f32;
        let panel = [nx(100.), ny(100.), nx(300.), ny(200.)];
        let radius = [nx(RADIUS), ny(RADIUS)];
        let style = PaintStyle::default().with_radii([radius; 4]).with_border(
            PaintBorder::uniform([ny(BORDER), nx(BORDER), ny(BORDER), nx(BORDER)], panel_border),
        );
        let glyph_texels = (GLYPH * scale) as u32;
        let workload = encode(ProductPacket {
            textures: vec![
                white(),
                Texture {
                    size: [glyph_texels, glyph_texels],
                    rgba: vec![255; (glyph_texels * glyph_texels * 4) as usize].into(),
                    mipmaps: false,
                },
            ],
            commands: vec![
                Command::StyledPaint {
                    paint: Paint::Solid(quad(panel, panel_fill)),
                    style: Box::new(style),
                },
                Command::StyledPaint {
                    paint: Paint::Stroke(PaintStroke {
                        start: [nx(120.), ny(350.)],
                        end: [nx(380.), ny(350.)],
                        width: [nx(STROKE), ny(STROKE)],
                        clip: FULL,
                        color: stroke_color,
                    }),
                    style: Box::new(PaintStyle::default()),
                },
                Command::Paint(Paint::Text {
                    quad: quad([nx(200.), ny(200.), nx(GLYPH), ny(GLYPH)], text_color),
                    atlas: 1,
                }),
                Command::StyledPaint {
                    paint: Paint::Solid(quad([nx(500.), ny(350.), nx(100.), ny(100.)], clip_color)),
                    style: Box::new(PaintStyle::default().with_clip(RoundedClip {
                        rect: [nx(520.), ny(370.), nx(60.), ny(60.)],
                        radii: [[nx(4.), ny(4.)]; 4],
                    })),
                },
            ],
            ..ProductPacket::default()
        });

        let stroke_thickness = with_color(&workload, stroke_color)
            .map(|vertex| physical(vertex, output).y)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), value| {
                (low.min(value), high.max(value))
            });
        assert!(
            (stroke_thickness.1 - stroke_thickness.0 - STROKE * scale).abs() < 1e-3,
            "{scale}: {stroke_thickness:?}"
        );

        // The top edge is one quad between the corners, so measure the ring
        // against the fill instead of expecting a vertex in the middle of the
        // straight edge. The fill starts exactly one border width below the
        // outer edge.
        let border_top = with_color(&workload, panel_border)
            .map(|vertex| physical(vertex, output).y)
            .fold(f32::INFINITY, f32::min);
        let fill_top = with_color(&workload, panel_fill)
            .map(|vertex| physical(vertex, output).y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            (fill_top - border_top - BORDER * scale).abs() < 1e-3,
            "{scale}: border_top={border_top:?} fill_top={fill_top:?}"
        );

        // The outer radius stays physical: every painted vertex near the
        // top-left corner sits on or inside the authored arc.
        let center = Vec2::new(
            (100. + RADIUS) * scale,
            (100. + RADIUS) * scale,
        );
        for vertex in &workload.vertices {
            let point = physical(vertex, output);
            if point.x < center.x && point.y < center.y && point.x > 100. * scale + 1e-3 {
                assert!(
                    point.distance(center) <= RADIUS * scale + 1e-3,
                    "{scale}: {point:?} outside the corner arc"
                );
            }
        }

        // Text keeps one atlas texel per output pixel at every density.
        let text = with_color(&workload, text_color).copied().collect::<Vec<_>>();
        let left = text
            .iter()
            .map(|vertex| physical(vertex, output).x)
            .fold(f32::INFINITY, f32::min);
        let right = text
            .iter()
            .map(|vertex| physical(vertex, output).x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (right - left - GLYPH * scale).abs() < 1e-3,
            "{scale}: {}",
            right - left
        );
        for vertex in text {
            assert!(
                (0. ..=1.).contains(&vertex.uv[0]) && (0. ..=1.).contains(&vertex.uv[1]),
                "{:?}",
                vertex.uv
            );
        }

        // A rounded clip keeps its physical rectangle and radius.
        let clipped = with_color(&workload, clip_color)
            .map(|vertex| physical(vertex, output))
            .collect::<Vec<_>>();
        let low = clipped.iter().fold(
            Vec2::splat(f32::INFINITY),
            |low, point| low.min(*point),
        );
        let high = clipped.iter().fold(
            Vec2::splat(f32::NEG_INFINITY),
            |high, point| high.max(*point),
        );
        assert!(
            (low - Vec2::new(520., 370.) * scale).length() < 1e-3,
            "{scale}: {low:?}"
        );
        assert!(
            (high - Vec2::new(580., 430.) * scale).length() < 1e-3,
            "{scale}: {high:?}"
        );
        let clip_corner = Vec2::new(524., 374.) * scale;
        for point in &clipped {
            if point.x < clip_corner.x && point.y < clip_corner.y {
                assert!(
                    point.distance(clip_corner) <= 4. * scale + 1e-3,
                    "{scale}: {point:?} outside the clip arc"
                );
            }
        }
    }
}

#[test]
fn paint_order_and_alpha_conventions_survive_batching() {
    let workload = encode(ProductPacket {
        textures: vec![white()],
        commands: vec![
            Command::Paint(Paint::Solid(quad([0., 0., 0.5, 0.5], [1., 0., 0., 1.]))),
            Command::Paint(Paint::Solid(quad([0.5, 0.5, 0.5, 0.5], [0., 0., 1., 0.5]))),
        ],
        ..ProductPacket::default()
    });
    let first_red = workload.vertices[0].rgba;
    assert_eq!(first_red, [1., 0., 0., 1.]);
    let last = workload.vertices[workload.vertices.len() - 1].rgba;
    assert!((last[2] - 1.).abs() < 1e-5 && (last[3] - 0.5).abs() < 1e-5);
    assert_eq!(workload.batches.len(), 1);
}

#[test]
fn text_atlas_patch_uploads_only_its_rectangle() {
    let mut device = open_recorder();
    let atlas = Texture {
        size: [64, 16],
        rgba: vec![0; 64 * 16 * 4].into(),
        mipmaps: false,
    };
    let handle = device
        .prepare(PacketKey(1), 1, || ProductPacket {
            textures: vec![atlas],
            commands: vec![Command::Paint(Paint::Text {
                quad: quad([0.1, 0.1, 0.2, 0.2], [1.; 4]),
                atlas: 0,
            })],
            ..ProductPacket::default()
        })
        .unwrap();

    let glyph = [255u8; 8 * 8 * 4];
    let patch = TexturePatch::new(0, [4, 2], [8, 8], &glyph);
    assert!(patch.into_resource_patch([64, 16]).is_ok());
    device
        .patch(handle, 1, 2, patch.into_resource_patch([64, 16]).unwrap())
        .unwrap();
    assert_eq!(
        device.draw(handle, &FrameUniforms::default()).unwrap().upload_bytes,
        8 * 8 * 4
    );

    assert!(matches!(
        TexturePatch::new(0, [60, 2], [8, 8], &glyph).into_resource_patch([64, 16]),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        TexturePatch::new(0, [0, 0], [8, 8], &glyph[..16]).into_resource_patch([64, 16]),
        Err(Error::Invalid(_))
    ));
}
