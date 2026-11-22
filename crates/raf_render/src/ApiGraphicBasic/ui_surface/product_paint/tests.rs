//! Focused checks for the retained RafUI paint producer.
//!
//! These tests inspect the product command stream that every native executor
//! consumes. They are not native pixel or performance evidence.

use raf_agb::backends::recorder::Recorder;
use raf_agb::product::{Command, ImageFit, PacketKey, Paint, PaintStyle, ProductDevice, ProductPacket};
use raf_agb::Output;
use raf_ui::{
    UiImageFit, UiImageInsets, UiImageOptions, UiLayout, UiLinearGradient, UiNode, UiNodeKind,
    UiRect,
};

use super::super::{
    craftsmanship_gallery, DirectUiSurfaceHost, StudioUiPalette, UiPaintEffects, UiPaintKey,
    UiRoundedClip, UiSurface, UiSurfaceDrawList, UiSurfaceImageQuad, UiSurfaceImageStore,
    UiSurfacePaintCommand, UiSurfaceQuad, UiSurfaceStroke, UiSurfaceTextQuad, UiTextAtlas,
};
use super::*;
use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;

const TARGET: [u32; 2] = [1000, 700];
const LOGICAL: [u32; 2] = [1000, 700];
const FULL: [f32; 4] = [0.0, 0.0, 1000.0, 700.0];
const CLIP: UiRect = UiRect {
    x: 0.0,
    y: 0.0,
    width: 1000.0,
    height: 700.0,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> UiRect {
    UiRect::new(x, y, width, height)
}

fn solid_command(index: usize, sequence: u32) -> UiSurfacePaintCommand {
    UiSurfacePaintCommand::Solid {
        index,
        z_index: 0,
        sequence,
    }
}

fn stroke_command(index: usize, sequence: u32) -> UiSurfacePaintCommand {
    UiSurfacePaintCommand::Stroke {
        index,
        z_index: 0,
        sequence,
    }
}

fn text_command(index: usize, sequence: u32) -> UiSurfacePaintCommand {
    UiSurfacePaintCommand::Text {
        index,
        z_index: 0,
        sequence,
    }
}

fn image_command(index: usize, sequence: u32) -> UiSurfacePaintCommand {
    UiSurfacePaintCommand::Image {
        index,
        z_index: 0,
        sequence,
    }
}

fn panel_quad(color: [u8; 4], radius: f32) -> UiSurfaceQuad {
    UiSurfaceQuad {
        rect: rect(10.0, 20.0, 300.0, 200.0),
        clip_rect: CLIP,
        color,
        z_index: 0,
        radius,
    }
}

fn mark_stroke() -> UiSurfaceStroke {
    UiSurfaceStroke {
        start: [20.0, 40.0],
        end: [120.0, 40.0],
        clip_rect: CLIP,
        color: [200, 200, 200, 255],
        width: 2.0,
        z_index: 1,
    }
}

fn glyph(atlas_rect: UiRect, color: [u8; 4]) -> UiSurfaceTextQuad {
    UiSurfaceTextQuad {
        rect: rect(30.0, 50.0, 8.0, 8.0),
        clip_rect: CLIP,
        atlas_rect,
        color,
        z_index: 2,
    }
}

fn image(source: &str, x: f32, fit: UiImageFit) -> UiSurfaceImageQuad {
    UiSurfaceImageQuad {
        rect: rect(x, 300.0, 20.0, 20.0),
        clip_rect: CLIP,
        source_key: source.to_string(),
        fit,
        tint: [255; 4],
        z_index: 3,
    }
}

fn paint_at(packet: &ProductPacket, index: usize) -> &Paint {
    match &packet.commands[index] {
        Command::Paint(paint) | Command::StyledPaint { paint, .. } => paint,
        _ => panic!("paint command expected"),
    }
}

fn style_at(packet: &ProductPacket, index: usize) -> &PaintStyle {
    match &packet.commands[index] {
        Command::StyledPaint { style, .. } => style,
        _ => panic!("styled paint expected"),
    }
}

fn append_packet(
    list: &UiSurfaceDrawList,
    atlas: &UiTextAtlas,
    images: &UiSurfaceImageStore,
    surface: &str,
    target_rect: [f32; 4],
    logical: [u32; 2],
) -> Result<(ProductPacket, UiPaintMetrics), String> {
    let mut packet = ProductPacket::default();
    let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
    let metrics = append_paint(
        &mut packet,
        list,
        atlas,
        images,
        &mut cache,
        surface,
        TARGET,
        target_rect,
        logical,
        1.0,
    )?;
    Ok((packet, metrics))
}

fn append(list: &UiSurfaceDrawList, atlas: &UiTextAtlas, images: &UiSurfaceImageStore) -> ProductPacket {
    append_packet(list, atlas, images, "test", FULL, LOGICAL)
        .expect("retained paint is supported")
        .0
}

#[test]
fn paint_order_is_preserved_exactly() {
    // Deliberately interleaved: a kind-ordered emitter would reorder this.
    let mut list = UiSurfaceDrawList {
        solids: vec![panel_quad([10, 20, 30, 255], 3.0)],
        strokes: vec![mark_stroke()],
        text: vec![glyph(rect(0.0, 0.0, 8.0, 8.0), [255; 4])],
        images: vec![image("img", 200.0, UiImageFit::Stretch)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![
        image_command(0, 0),
        solid_command(0, 1),
        text_command(0, 2),
        stroke_command(0, 3),
    ];
    let mut images = UiSurfaceImageStore::default();
    images.insert_rgba("img", [2, 2], vec![255; 16]).unwrap();
    let atlas = UiTextAtlas::new([16, 16]);
    let packet = append(&list, &atlas, &images);
    assert_eq!(packet.commands.len(), 4);
    assert!(matches!(paint_at(&packet, 0), Paint::Image { .. }));
    assert!(matches!(paint_at(&packet, 1), Paint::Solid(_)));
    assert!(matches!(paint_at(&packet, 2), Paint::Text { .. }));
    assert!(matches!(paint_at(&packet, 3), Paint::Stroke(_)));
}

#[test]
fn per_corner_radii_and_border_sides_are_axis_normalized() {
    let mut list = UiSurfaceDrawList {
        solids: vec![panel_quad([10, 20, 30, 255], 3.0)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![solid_command(0, 0)];
    list.effects.insert(
        UiPaintKey::Solid(0),
        UiPaintEffects {
            corners: Some([4.0, 8.0, 12.0, 16.0]),
            border: [255, 0, 0, 255],
            border_widths: [1.0, 2.0, 3.0, 4.0],
            ..Default::default()
        },
    );
    let packet = append(
        &list,
        &UiTextAtlas::new([16, 16]),
        &UiSurfaceImageStore::default(),
    );
    let Paint::Solid(quad) = paint_at(&packet, 0) else {
        panic!("solid paint expected");
    };
    assert!((quad.color[0] - 10.0 / 255.0).abs() < 1e-6);
    assert!((quad.color[3] - 1.0).abs() < 1e-6);
    let style = style_at(&packet, 0);
    let expected = [
        [4.0 / 1000.0, 4.0 / 700.0],
        [8.0 / 1000.0, 8.0 / 700.0],
        [12.0 / 1000.0, 12.0 / 700.0],
        [16.0 / 1000.0, 16.0 / 700.0],
    ];
    for (got, expected) in style.radii.into_iter().zip(expected) {
        assert!((got[0] - expected[0]).abs() < 1e-6, "{got:?}");
        assert!((got[1] - expected[1]).abs() < 1e-6, "{got:?}");
    }
    let border = style.border.expect("border");
    assert_eq!(border.color, [1.0, 0.0, 0.0, 1.0]);
    assert!((border.widths[0] - 1.0 / 700.0).abs() < 1e-6);
    assert!((border.widths[1] - 2.0 / 1000.0).abs() < 1e-6);
    assert!((border.widths[2] - 3.0 / 700.0).abs() < 1e-6);
    assert!((border.widths[3] - 4.0 / 1000.0).abs() < 1e-6);
}

#[test]
fn gradient_clip_stack_and_rect_mapping_keep_their_values() {
    let mut panel = panel_quad([10, 20, 30, 255], 3.0);
    panel.rect = rect(0.0, 0.0, 400.0, 300.0);
    let mut list = UiSurfaceDrawList {
        solids: vec![panel],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![solid_command(0, 0)];
    list.effects.insert(
        UiPaintKey::Solid(0),
        UiPaintEffects {
            gradient: Some(UiLinearGradient {
                start: [255, 0, 0, 255],
                end: [0, 0, 255, 128],
                direction: [0.0, 1.0],
            }),
            clips: vec![
                UiRoundedClip {
                    rect: rect(0.0, 0.0, 500.0, 350.0),
                    corners: [2.0; 4],
                },
                UiRoundedClip {
                    rect: rect(10.0, 10.0, 400.0, 300.0),
                    corners: [0.0, 6.0, 0.0, 6.0],
                },
            ],
            ..Default::default()
        },
    );
    let packet = append_packet(
        &list,
        &UiTextAtlas::new([16, 16]),
        &UiSurfaceImageStore::default(),
        "test",
        [100.0, 50.0, 800.0, 600.0],
        [400, 300],
    )
    .unwrap()
    .0;
    let Paint::Solid(quad) = paint_at(&packet, 0) else {
        panic!("solid paint expected");
    };
    assert!((quad.rect[0] - 0.1).abs() < 1e-6);
    assert!((quad.rect[1] - 50.0 / 700.0).abs() < 1e-6);
    assert!((quad.rect[2] - 0.8).abs() < 1e-6);
    assert!((quad.rect[3] - 600.0 / 700.0).abs() < 1e-6);
    let style = style_at(&packet, 0);
    let gradient = style.gradient.expect("gradient");
    assert_eq!(gradient.direction, [0.0, 1.0]);
    assert_eq!(gradient.start, [1.0, 0.0, 0.0, 1.0]);
    assert!((gradient.end[3] - 128.0 / 255.0).abs() < 1e-6);
    assert_eq!(style.clips.len(), 2);
    assert!((style.clips[0].rect[0] - 0.1).abs() < 1e-6);
    assert!((style.clips[1].rect[2] - 0.8).abs() < 1e-6);
    assert!((style.clips[1].radii[1][0] - 12.0 / 1000.0).abs() < 1e-6);
}

#[test]
fn nine_clip_depth_is_the_declared_limit_plus_the_painted_edge() {
    let clips = |count: usize| {
        vec![
            UiRoundedClip {
                rect: rect(0.0, 0.0, 900.0, 600.0),
                corners: [4.0; 4],
            };
            count
        ]
    };
    let mut list = UiSurfaceDrawList {
        solids: vec![panel_quad([10, 20, 30, 255], 3.0)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![solid_command(0, 0)];
    let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
    let mut packet = ProductPacket::default();
    list.effects.insert(
        UiPaintKey::Solid(0),
        UiPaintEffects {
            clips: clips(8),
            ..Default::default()
        },
    );
    let metrics = append_paint(
        &mut packet,
        &list,
        &UiTextAtlas::new([16, 16]),
        &UiSurfaceImageStore::default(),
        &mut cache,
        "test",
        TARGET,
        FULL,
        LOGICAL,
        1.0,
    )
    .expect("eight declared clips plus the painted edge are supported");
    assert_eq!(metrics.clips, 9);
    assert_eq!(style_at(&packet, 0).clips.len(), 8);

    list.effects.insert(
        UiPaintKey::Solid(0),
        UiPaintEffects {
            clips: clips(9),
            ..Default::default()
        },
    );
    let error = append_paint(
        &mut ProductPacket::default(),
        &list,
        &UiTextAtlas::new([16, 16]),
        &UiSurfaceImageStore::default(),
        &mut cache,
        "test",
        TARGET,
        FULL,
        LOGICAL,
        1.0,
    )
    .expect_err("nine declared clips exceed the shared limit");
    assert!(
        error.contains("RafUI surface 'test': solid paint #0"),
        "{error}"
    );
    assert!(error.contains("9 nested rounded clips"), "{error}");
}

#[test]
fn nine_slice_uses_source_texels_and_normalized_target_insets() {
    let mut list = UiSurfaceDrawList {
        images: vec![image("nine", 100.0, UiImageFit::Stretch)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![image_command(0, 0)];
    list.effects.insert(
        UiPaintKey::Image(0),
        UiPaintEffects {
            image: Some(UiImageOptions {
                nine_slice: Some(UiImageInsets::new(10.0, 10.0, 10.0, 10.0)),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let mut images = UiSurfaceImageStore::default();
    images
        .insert_rgba("nine", [32, 32], vec![255; 32 * 32 * 4])
        .unwrap();
    let packet = append(&list, &UiTextAtlas::new([16, 16]), &images);
    let Paint::Image { texture, quad } = paint_at(&packet, 0) else {
        panic!("image paint expected");
    };
    assert_eq!(*texture, 0);
    assert_eq!(packet.textures[0].size, [32, 32]);
    assert_eq!(quad.uv_rect, [0.0, 0.0, 1.0, 1.0]);
    let layout = style_at(&packet, 0).image.expect("image layout");
    assert_eq!(layout.fit, ImageFit::Stretch);
    assert_eq!(layout.target_extent, TARGET);
    let slices = layout.nine_slice.expect("nine slice");
    assert_eq!(slices.source, [10.0; 4]);
    assert!((slices.target[0] - 10.0 / 700.0).abs() < 1e-6);
    assert!((slices.target[1] - 10.0 / 1000.0).abs() < 1e-6);
    assert!((slices.target[2] - 10.0 / 700.0).abs() < 1e-6);
    assert!((slices.target[3] - 10.0 / 1000.0).abs() < 1e-6);
}

#[test]
fn image_crop_fit_and_feather_reach_the_style() {
    let mut list = UiSurfaceDrawList {
        images: vec![image("photo", 40.0, UiImageFit::Cover)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![image_command(0, 0)];
    list.effects.insert(
        UiPaintKey::Image(0),
        UiPaintEffects {
            image: Some(UiImageOptions {
                source_rect: Some(raf_ui::UiImageRect::new(0.25, 0.1, 0.5, 0.5)),
                focal_point: [1.0, 0.0],
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let mut images = UiSurfaceImageStore::default();
    images
        .insert_rgba("photo", [8, 8], vec![255; 8 * 8 * 4])
        .unwrap();
    let packet = append(&list, &UiTextAtlas::new([16, 16]), &images);
    let Paint::Image { quad, .. } = paint_at(&packet, 0) else {
        panic!("image paint expected");
    };
    assert_eq!(quad.uv_rect, [0.25, 0.1, 0.5, 0.5]);
    let layout = style_at(&packet, 0).image.expect("image layout");
    assert_eq!(layout.fit, ImageFit::Cover);
    assert_eq!(layout.focal_point, [1.0, 0.0]);
    assert!(layout.nine_slice.is_none());
}

#[test]
fn dpi_100_125_150_200_keep_physical_stroke_radius_and_glyph_size() {
    for percent in [100_u16, 125, 150, 200] {
        let scale = f32::from(percent) / 100.0;
        let target = [
            (1000.0 * scale).round() as u32,
            (700.0 * scale).round() as u32,
        ];
        let layer = [0.0, 0.0, target[0] as f32, target[1] as f32];
        let mut list = UiSurfaceDrawList {
            solids: vec![UiSurfaceQuad {
                rect: rect(100.0, 100.0, 300.0, 200.0),
                clip_rect: CLIP,
                color: [20, 30, 40, 255],
                z_index: 0,
                radius: 6.0,
            }],
            strokes: vec![UiSurfaceStroke {
                start: [120.0, 350.0],
                end: [380.0, 350.0],
                clip_rect: CLIP,
                color: [0, 255, 0, 255],
                width: 1.5,
                z_index: 1,
            }],
            text: vec![UiSurfaceTextQuad {
                rect: rect(200.0, 200.0, 8.0, 8.0),
                clip_rect: CLIP,
                atlas_rect: rect(0.0, 0.0, 8.0 * scale, 8.0 * scale),
                color: [240; 4],
                z_index: 2,
            }],
            ..UiSurfaceDrawList::default()
        };
        list.paint_order = vec![solid_command(0, 0), stroke_command(0, 1), text_command(0, 2)];
        let atlas = UiTextAtlas::new([(32.0 * scale) as u16, 32]);
        let mut packet = ProductPacket::default();
        let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
        append_paint(
            &mut packet,
            &list,
            &atlas,
            &UiSurfaceImageStore::default(),
            &mut cache,
            "dpi",
            target,
            layer,
            [1000, 700],
            scale,
        )
        .expect("DPI paint is supported");

        let radius = style_at(&packet, 0).radii[0];
        assert!((radius[0] * target[0] as f32 - 6.0 * scale).abs() < 1e-3);
        assert!((radius[1] * target[1] as f32 - 6.0 * scale).abs() < 1e-3);

        let Paint::Stroke(stroke) = paint_at(&packet, 1) else {
            panic!("stroke paint expected");
        };
        let horizontal = stroke.width[0] * target[0] as f32;
        let vertical = stroke.width[1] * target[1] as f32;
        assert!(
            (horizontal - 1.5 * scale).abs() < 1e-3,
            "{percent}%: {horizontal}"
        );
        assert!(
            (vertical - 1.5 * scale).abs() < 1e-3,
            "{percent}%: {vertical}"
        );

        let Paint::Text { quad, .. } = paint_at(&packet, 2) else {
            panic!("text paint expected");
        };
        assert!((quad.rect[2] * target[0] as f32 - 8.0 * scale).abs() < 1e-3);
    }
}

#[test]
fn transparent_empty_and_unresolved_paint_is_skipped() {
    let mut list = UiSurfaceDrawList {
        solids: vec![
            UiSurfaceQuad {
                rect: rect(10.0, 10.0, 100.0, 100.0),
                clip_rect: CLIP,
                color: [0; 4],
                z_index: 0,
                radius: 0.0,
            },
            UiSurfaceQuad {
                rect: rect(0.0, 0.0, 0.0, 0.0),
                clip_rect: CLIP,
                color: [255; 4],
                z_index: 1,
                radius: 0.0,
            },
            UiSurfaceQuad {
                rect: rect(10.0, 10.0, 100.0, 100.0),
                clip_rect: CLIP,
                color: [0; 4],
                z_index: 2,
                radius: 0.0,
            },
        ],
        strokes: vec![UiSurfaceStroke {
            start: [0.0, 0.0],
            end: [10.0, 10.0],
            clip_rect: CLIP,
            color: [255; 4],
            width: 0.0,
            z_index: 3,
        }],
        images: vec![image("missing", 0.0, UiImageFit::Stretch)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![
        solid_command(0, 0),
        solid_command(1, 1),
        solid_command(2, 2),
        stroke_command(0, 3),
        image_command(0, 4),
    ];
    // A hollow border is visible even when the fill is fully transparent.
    list.effects.insert(
        UiPaintKey::Solid(2),
        UiPaintEffects {
            border: [255, 255, 255, 255],
            border_widths: [1.0; 4],
            ..Default::default()
        },
    );
    let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
    let mut packet = ProductPacket::default();
    let metrics = append_paint(
        &mut packet,
        &list,
        &UiTextAtlas::new([16, 16]),
        &UiSurfaceImageStore::default(),
        &mut cache,
        "test",
        TARGET,
        FULL,
        LOGICAL,
        1.0,
    )
    .unwrap();
    assert_eq!(metrics.solids, 1);
    assert_eq!(metrics.styles, 1);
    assert_eq!(metrics.paint_commands(), 1);
    assert!(packet.textures.is_empty());
}

#[test]
fn unsupported_effect_combinations_name_the_surface_and_paint() {
    let mut list = UiSurfaceDrawList {
        strokes: vec![mark_stroke()],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![stroke_command(0, 0)];
    list.effects.insert(
        UiPaintKey::Stroke(0),
        UiPaintEffects {
            border: [255, 0, 0, 255],
            border_widths: [1.0, 0.0, 0.0, 0.0],
            ..Default::default()
        },
    );
    let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
    let error = append_paint(
        &mut ProductPacket::default(),
        &list,
        &UiTextAtlas::new([16, 16]),
        &UiSurfaceImageStore::default(),
        &mut cache,
        "editor.inspector",
        TARGET,
        FULL,
        LOGICAL,
        1.0,
    )
    .expect_err("a bordered stroke is rejected");
    assert!(
        error.contains("RafUI surface 'editor.inspector': stroke paint #0"),
        "{error}"
    );
    assert!(error.contains("per-side borders"), "{error}");

    let mut list = UiSurfaceDrawList {
        images: vec![image("sliced", 10.0, UiImageFit::Contain)],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![image_command(0, 0)];
    list.effects.insert(
        UiPaintKey::Image(0),
        UiPaintEffects {
            softness: 2.0,
            image: Some(UiImageOptions {
                nine_slice: Some(UiImageInsets::new(4.0, 4.0, 4.0, 4.0)),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let mut images = UiSurfaceImageStore::default();
    images
        .insert_rgba("sliced", [8, 8], vec![255; 8 * 8 * 4])
        .unwrap();
    let error = append_paint(
        &mut ProductPacket::default(),
        &list,
        &UiTextAtlas::new([16, 16]),
        &images,
        &mut cache,
        "editor.menu",
        TARGET,
        FULL,
        LOGICAL,
        1.0,
    )
    .expect_err("feather on a sliced image is rejected");
    assert!(error.contains("editor.menu"), "{error}");
    assert!(
        error.contains("feather with a fitted or sliced image"),
        "{error}"
    );
}

#[test]
fn one_image_revision_is_uploaded_once_per_packet() {
    let mut list = UiSurfaceDrawList {
        images: vec![
            image("icon", 0.0, UiImageFit::Contain),
            UiSurfaceImageQuad {
                tint: [180, 180, 180, 255],
                ..image("icon", 20.0, UiImageFit::Contain)
            },
        ],
        ..UiSurfaceDrawList::default()
    };
    list.paint_order = vec![image_command(0, 0), image_command(1, 1)];
    let mut images = UiSurfaceImageStore::default();
    images
        .insert_rgba("icon", [4, 4], vec![200; 4 * 4 * 4])
        .unwrap();
    let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
    let mut packet = ProductPacket::default();
    let metrics = append_paint(
        &mut packet,
        &list,
        &UiTextAtlas::new([16, 16]),
        &images,
        &mut cache,
        "test",
        TARGET,
        FULL,
        LOGICAL,
        1.0,
    )
    .unwrap();
    assert_eq!(metrics.images, 2);
    assert_eq!(packet.textures.len(), 1);
    assert_eq!(metrics.image_bytes, 4 * 4 * 4);
    assert_eq!(cache.conversions(), 1);
}

fn text_surface() -> UiSurface {
    UiSurface::new(
        "host",
        StudioUiPalette::IndustrialDark,
        UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("label", UiNodeKind::Label)
                .with_layout(UiLayout::fixed(180.0, 24.0))
                .with_text_key("app.title"),
        ),
    )
}

#[test]
fn host_reuses_the_atlas_texture_and_reports_paint_revisions() {
    let mut host =
        DirectUiSurfaceHost::with_budget(text_surface(), [0; 4], GraphicsMemoryBudget::potato());
    let mut first = ProductPacket::default();
    let first_metrics = host
        .append_product_paint(
            &mut first,
            [320, 80],
            [0.0, 0.0, 320.0, 80.0],
            [320, 80],
            1.0,
            |_| "RafUI".to_string(),
        )
        .expect("host paint");
    assert_eq!(first_metrics.text, 1);
    assert!(first_metrics.atlas_bytes > 0);
    assert_eq!(host.texture_conversions(), 1);
    let revision = host.paint_revision();
    let atlas = host.atlas_generation();

    let mut second = ProductPacket::default();
    let second_metrics = host
        .append_product_paint(
            &mut second,
            [320, 80],
            [0.0, 0.0, 320.0, 80.0],
            [320, 80],
            1.0,
            |_| "RafUI".to_string(),
        )
        .expect("host paint");
    assert_eq!(second_metrics.text, 1);
    assert_eq!(second_metrics.atlas_bytes, first_metrics.atlas_bytes);
    assert_eq!(
        host.texture_conversions(),
        1,
        "the atlas is not re-rasterized"
    );
    assert_eq!(host.paint_revision(), revision);
    assert_eq!(host.atlas_generation(), atlas);
    assert_eq!(
        host.last_produced_frame().map(|frame| frame.metrics),
        Some(second_metrics)
    );

    assert!(host.patch_text_value("label", "Zzyzx"));
    let changed = host.paint_revision();
    assert_ne!(changed, revision);
    let mut third = ProductPacket::default();
    host.append_product_paint(
        &mut third,
        [320, 80],
        [0.0, 0.0, 320.0, 80.0],
        [320, 80],
        1.0,
        |_| "Zzyzx".to_string(),
    )
    .expect("host paint");
    assert_ne!(host.atlas_generation(), atlas, "new glyphs advance the atlas");
    assert_eq!(
        host.texture_conversions(),
        2,
        "one conversion per generation"
    );
}

/// The real shared product encoder must accept the retained paint of a real
/// editor fixture. This exercises borders, per-corner radii, a gradient, a
/// shadow feather, text and an image in one packet.
#[test]
fn agb_encoder_accepts_a_craftsmanship_editor_surface() {
    let mut host = DirectUiSurfaceHost::new(
        craftsmanship_gallery(StudioUiPalette::IndustrialDark),
        [9, 9, 10, 255],
    );
    host.images_mut()
        .insert_rgba("gallery://missing", [2, 2], vec![200; 16])
        .unwrap();
    let mut packet = ProductPacket::default();
    let metrics = host
        .append_product_paint(
            &mut packet,
            [640, 360],
            [0.0, 0.0, 640.0, 360.0],
            [512, 288],
            1.25,
            |key| key.to_string(),
        )
        .expect("retained paint");
    assert!(metrics.paint_commands() >= 5, "{metrics:?}");
    assert!(metrics.text >= 4, "{metrics:?}");
    assert!(metrics.images >= 1, "{metrics:?}");
    assert!(metrics.styles >= 3, "{metrics:?}");

    let mut device =
        ProductDevice::new(Recorder::default(), GraphicsMemoryBudget::potato()).unwrap();
    device.configure([640, 360], Output::Capture).unwrap();
    device
        .prepare(PacketKey(11), 1, || packet)
        .expect("ApiGraphicBasic accepts the retained RafUI paint packet");
}
