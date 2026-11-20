use super::*;

use std::collections::HashMap;

use raf_ui::{UiImagePlaceholder, UiImageResourceStatus, UiImageSampling};

use super::super::effects::{UiPaintEffects, UiPaintKey, UiRoundedClip, MAX_ROUNDED_CLIPS};
use super::super::{UiRect, UiSurfaceDrawList, UiSurfaceImageStore, UiSurfacePaintCommand};
use super::buffers::{
    build_gpu_effect_table, padded_rgba_rows, GpuPaintEffect, UiSurfaceGpuGeometryCache,
    UiSurfaceGpuGeometryKey,
};
use super::diagnostics::estimate_translucent_solids;
use super::draw::{build_paint_runs, solid_vertices, stroke_vertices, ui_scissor_rect};
use super::images::{
    downsample_rgba_premultiplied, image_mip_chain_bytes, image_mip_level_count, image_vertices,
    missing_placeholder_mode, resolved_sampling,
};
use super::pipelines::UI_SURFACE_WGSL;
use super::text::text_vertices;

#[test]
fn ui_surface_shader_parses_and_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(UI_SURFACE_WGSL)
        .expect("UI surface WGSL should parse");
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("UI surface WGSL should validate");
}

#[test]
fn translucent_coverage_metrics_scale_to_the_physical_target() {
    let list = UiSurfaceDrawList {
        solids: vec![super::super::UiSurfaceQuad {
            rect: super::super::UiRect::new(5.0, 5.0, 10.0, 8.0),
            clip_rect: super::super::UiRect::new(0.0, 0.0, 20.0, 20.0),
            color: [25, 31, 38, 220],
            z_index: 0,
            radius: 0.0,
        }],
        ..UiSurfaceDrawList::default()
    };

    assert_eq!(
        estimate_translucent_solids(&list, [20, 20], [40, 40]),
        (1, 320)
    );
}

#[test]
fn rounded_solid_uses_one_quad_and_carries_analytic_corner_data() {
    let list = UiSurfaceDrawList {
        solids: vec![super::super::UiSurfaceQuad {
            rect: UiRect::new(0.0, 0.0, 120.0, 40.0),
            clip_rect: UiRect::new(0.0, 0.0, 120.0, 40.0),
            color: [255, 255, 255, 255],
            z_index: 0,
            radius: 12.0,
        }],
        ..UiSurfaceDrawList::default()
    };
    let table = build_gpu_effect_table(&list, 120, 40, 120, 40, true);
    let batch = solid_vertices(&list, 120, 40, 120, 40, true, &table);

    assert_eq!(batch.ranges[0], 0..6);
    assert_eq!(table.paints[0].corners, [12.0; 4]);
}

#[test]
fn stroke_batch_uses_connected_caps_and_one_range_per_stroke() {
    let list = UiSurfaceDrawList {
        strokes: vec![super::super::UiSurfaceStroke {
            start: [4.0, 5.0],
            end: [20.0, 17.0],
            clip_rect: super::super::UiRect::new(0.0, 0.0, 40.0, 30.0),
            color: [237, 239, 242, 255],
            width: 3.0,
            z_index: 0,
        }],
        ..UiSurfaceDrawList::default()
    };

    let table = build_gpu_effect_table(&list, 40, 30, 80, 60, true);
    let batch = stroke_vertices(&list, 40, 30, 80, 60, true, &table);

    assert_eq!(batch.ranges.len(), 1);
    assert_eq!(batch.ranges[0].start, 0);
    assert_eq!(batch.ranges[0].end - batch.ranges[0].start, 66);
}

#[test]
fn vertex_batches_keep_one_draw_range_per_surface_item() {
    let list = UiSurfaceDrawList {
        solids: vec![
            super::super::UiSurfaceQuad {
                rect: super::super::UiRect::new(0.0, 0.0, 40.0, 20.0),
                clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                color: [255, 255, 255, 255],
                z_index: 0,
                radius: 0.0,
            },
            super::super::UiSurfaceQuad {
                rect: super::super::UiRect::new(40.0, 0.0, 40.0, 20.0),
                clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                color: [255, 128, 0, 255],
                z_index: 1,
                radius: 6.0,
            },
        ],
        ..UiSurfaceDrawList::default()
    };
    let table = build_gpu_effect_table(&list, 160, 80, 160, 80, true);
    let batch = solid_vertices(&list, 160, 80, 160, 80, true, &table);

    assert_eq!(batch.ranges.len(), 2);
    assert_eq!(batch.ranges[0], 0..6);
    assert!(batch.ranges[1].end > batch.ranges[1].start);
}

#[test]
fn physical_target_scales_logical_geometry_before_ndc_conversion() {
    let list = UiSurfaceDrawList {
        solids: vec![super::super::UiSurfaceQuad {
            rect: super::super::UiRect::new(40.0, 20.0, 40.0, 20.0),
            clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
            color: [255, 255, 255, 255],
            z_index: 0,
            radius: 0.0,
        }],
        ..UiSurfaceDrawList::default()
    };

    let table = build_gpu_effect_table(&list, 160, 80, 320, 160, true);
    let batch = solid_vertices(&list, 160, 80, 320, 160, true, &table);

    // One physical pixel of raster padding contains the analytic AA edge;
    // the effect's shape remains on the original snapped rectangle.
    assert_eq!(batch.vertices[0].position, ndc_point(79.0, 39.0, 320, 160));
    assert_eq!(batch.vertices[1].position, ndc_point(161.0, 39.0, 320, 160));
    assert_eq!(table.paints[0].rect, [80.0, 40.0, 80.0, 40.0]);
}

#[test]
fn physical_geometry_snaps_small_controls_to_pixel_boundaries() {
    let snapped = physical_snap_rect(
        super::super::UiRect::new(10.24, 4.76, 13.52, 9.48),
        1.0,
        1.0,
    );

    assert_eq!(snapped, super::super::UiRect::new(10.0, 5.0, 14.0, 9.0));
}

#[test]
fn rounded_clip_chains_are_deduplicated_and_bounded() {
    let clip = UiRoundedClip {
        rect: UiRect::new(2.0, 3.0, 20.0, 12.0),
        corners: [5.0; 4],
    };
    let mut list = UiSurfaceDrawList {
        solids: vec![
            super::super::UiSurfaceQuad {
                rect: UiRect::new(0.0, 0.0, 20.0, 12.0),
                clip_rect: UiRect::new(0.0, 0.0, 30.0, 20.0),
                color: [20, 30, 40, 255],
                z_index: 0,
                radius: 0.0,
            },
            super::super::UiSurfaceQuad {
                rect: UiRect::new(0.0, 0.0, 20.0, 12.0),
                clip_rect: UiRect::new(0.0, 0.0, 30.0, 20.0),
                color: [20, 30, 40, 255],
                z_index: 1,
                radius: 0.0,
            },
        ],
        ..UiSurfaceDrawList::default()
    };
    for index in 0..2 {
        list.effects.insert(
            UiPaintKey::Solid(index),
            UiPaintEffects {
                clips: vec![clip.clone()],
                ..UiPaintEffects::default()
            },
        );
    }
    let table = build_gpu_effect_table(&list, 30, 20, 30, 20, true);

    assert_eq!(table.clips.len(), 1);
    assert_eq!(
        table.paints[table.indices[&UiPaintKey::Solid(0)] as usize].clip_range_flags[0..2],
        [0, 1]
    );
    assert_eq!(
        table.paints[table.indices[&UiPaintKey::Solid(1)] as usize].clip_range_flags[0..2],
        [0, 1]
    );

    list.effects.get_mut(&UiPaintKey::Solid(0)).unwrap().clips =
        vec![clip; MAX_ROUNDED_CLIPS + 1];
    let table = build_gpu_effect_table(&list, 30, 20, 30, 20, true);
    let flags = table.paints[table.indices[&UiPaintKey::Solid(0)] as usize].clip_range_flags[2];
    assert_ne!(flags & GpuPaintEffect::INVALID_CLIPS, 0);
}

#[test]
fn automatic_missing_placeholders_distinguish_pending_from_error() {
    assert_eq!(
        missing_placeholder_mode(
            UiImagePlaceholder::Automatic,
            UiImageResourceStatus::Pending,
        ),
        Some(0.0)
    );
    assert_eq!(
        missing_placeholder_mode(
            UiImagePlaceholder::Automatic,
            UiImageResourceStatus::Missing,
        ),
        Some(2.0)
    );
    assert_eq!(
        missing_placeholder_mode(UiImagePlaceholder::Automatic, UiImageResourceStatus::Error,),
        Some(2.0)
    );
    assert_eq!(
        missing_placeholder_mode(UiImagePlaceholder::Hidden, UiImageResourceStatus::Missing,),
        None
    );
}

#[test]
fn automatic_image_sampling_preserves_the_linear_gpu_default() {
    assert_eq!(
        resolved_sampling(UiImageSampling::Auto),
        UiImageSampling::Linear
    );
    assert_eq!(
        resolved_sampling(UiImageSampling::Nearest),
        UiImageSampling::Nearest
    );
}

#[test]
fn adjacent_compatible_paint_commands_merge_into_one_gpu_draw() {
    let list = UiSurfaceDrawList {
        solids: vec![
            super::super::UiSurfaceQuad {
                rect: super::super::UiRect::new(0.0, 0.0, 40.0, 20.0),
                clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                color: [255, 255, 255, 255],
                z_index: 0,
                radius: 0.0,
            },
            super::super::UiSurfaceQuad {
                rect: super::super::UiRect::new(40.0, 0.0, 40.0, 20.0),
                clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                color: [255, 128, 0, 255],
                z_index: 0,
                radius: 0.0,
            },
        ],
        paint_order: vec![
            UiSurfacePaintCommand::Solid {
                index: 0,
                z_index: 0,
                sequence: 0,
            },
            UiSurfacePaintCommand::Solid {
                index: 1,
                z_index: 0,
                sequence: 1,
            },
        ],
        ..UiSurfaceDrawList::default()
    };
    let image_store = UiSurfaceImageStore::default();
    let effect_table = build_gpu_effect_table(&list, 160, 80, 160, 80, true);
    let geometry = UiSurfaceGpuGeometryCache {
        key: UiSurfaceGpuGeometryKey {
            draw_list_identity: std::ptr::from_ref(&list) as usize,
            draw_list_revision: 0,
            logical_size: [160, 80],
            target_size: [160, 80],
            atlas_revision: 0,
            atlas_size: [0, 0],
            image_signature: 0,
            icon_atlas_revision: 0,
        },
        solid_batch: solid_vertices(&list, 160, 80, 160, 80, true, &effect_table),
        stroke_batch: stroke_vertices(&list, 160, 80, 160, 80, true, &effect_table),
        text_batch: text_vertices(&list, 160, 80, 160, 80, true, &effect_table),
        image_batch: image_vertices(
            &list,
            160,
            80,
            160,
            80,
            &image_store,
            true,
            &list.effects,
            &effect_table,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        ),
        effect_table,
        paint_runs: Vec::new(),
    };

    let runs = build_paint_runs(
        &list,
        &geometry,
        [160, 80],
        [160, 80],
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].range, 0..12);
}

#[test]
fn srgb_surface_colors_are_linearized_before_gpu_compositing() {
    let dark = color_to_f32([8, 8, 8, 255], true);
    let raw = color_to_f32([8, 8, 8, 255], false);

    assert!(dark[0] < raw[0]);
    assert!((raw[0] - 8.0 / 255.0).abs() < f32::EPSILON);
    assert!((color_from_bytes([255, 255, 255, 255], true).r - 1.0).abs() < f64::EPSILON);
}

#[test]
fn scissor_uses_physical_pixels_for_a_hidpi_target() {
    let clip = super::super::UiRect::new(10.0, 20.0, 30.0, 40.0);

    assert_eq!(
        ui_scissor_rect(Some(clip), [200, 160], [100, 80]),
        Some((20, 40, 60, 80))
    );
}

#[test]
fn image_mip_levels_cover_small_icon_minification() {
    assert_eq!(image_mip_level_count([1, 1]), 1);
    assert_eq!(image_mip_level_count([16, 8]), 5);
    assert_eq!(image_mip_level_count([64, 64]), 7);

    let (pixels, row_bytes) = padded_rgba_rows(&[255; 4 * 3 * 2], [3, 2]);
    assert_eq!(row_bytes, 256);
    assert_eq!(pixels.len(), 256 * 2);
    assert_eq!(&pixels[..12], &[255; 12]);

    let mip = downsample_rgba_premultiplied(&[255, 0, 0, 255, 0, 0, 0, 0], [2, 1], [1, 1]);
    assert_eq!(mip, vec![188, 0, 0, 128]);
}

#[test]
fn image_budget_counts_the_complete_mip_chain() {
    assert_eq!(image_mip_chain_bytes([1, 1]), 4);
    assert_eq!(image_mip_chain_bytes([2, 2]), 20);
    assert_eq!(image_mip_chain_bytes([4, 2]), 44);
}
