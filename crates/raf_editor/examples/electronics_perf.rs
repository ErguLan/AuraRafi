//! CPU baseline for the retained Electronics CAD frame and minimap builders.
//!
//! Run with:
//! `cargo run --locked --offline -p raf_editor --example electronics_perf --target-dir D:/Proyectos/ProyectRaf/target_electronics_production_20261023`
//!
//! This measures command construction and the current source minimap rasterizer.
//! It does not create a device, submit GPU work, or open a native window.

pub use raf_editor::electronics_controller;
#[path = "../src/electronics_minimap.rs"]
mod electronics_minimap;

use std::hint::black_box;
use std::time::{Duration, Instant};

use electronics_minimap::{
    build_rgba, build_scene_rgba, world_at, viewport_rect, MinimapColors, RasterCache,
};
use glam::Vec2;
use raf_electronics::{
    pcb::{BoardOutline, PcbLayer, PcbTrace},
    CadObjectKind, CadScene, ElectronicComponent, PcbLayout, Schematic,
};
use raf_editor::electronics_controller::CadCamera;
use raf_render::api_graphic_basic::cad_surface::{
    build_cad_surface_frame, CadSurfaceOptions,
};
use raf_ui::StudioUiPalette;
use uuid::Uuid;

const VIEWPORT: [u32; 2] = [1280, 720];
const SAMPLE_COUNT: usize = 30;
const WARMUP_COUNT: usize = 8;
const CACHE_UI_REVISION: u64 = 0;
const PAN_STEP_PIXELS: Vec2 = Vec2::new(1.25, 0.75);
const SCHEMATIC_PITCH: Vec2 = Vec2::new(140.0, 120.0);
const PCB_PITCH: f32 = 100.0;

struct Fixture {
    name: &'static str,
    scene: CadScene,
    camera: CadCamera,
    component_count: usize,
}

fn main() {
    println!(
        "Electronics CPU baseline: viewport={}x{}, minimap=880x560, warmup={}, measured={}, pan={:.2}x{:.2}px/sample",
        VIEWPORT[0], VIEWPORT[1], WARMUP_COUNT, SAMPLE_COUNT, PAN_STEP_PIXELS.x, PAN_STEP_PIXELS.y
    );
    println!("Scope: CPU CAD frame and minimap builders; no GPU/native window.");

    for component_count in [2, 100] {
        let schematic = make_schematic(component_count);
        let pcb = make_pcb(&schematic);
        let schem_scene = CadScene::from_schematic(&schematic);
        let pcb_scene = CadScene::from_pcb(&pcb);
        let schem_camera = schematic_camera(component_count);
        let pcb_camera = pcb_camera(&pcb);

        let fixtures = [
            Fixture {
                name: "schematic",
                scene: schem_scene,
                camera: schem_camera,
                component_count,
            },
            Fixture {
                name: "pcb",
                scene: pcb_scene,
                camera: pcb_camera,
                component_count,
            },
        ];

        for fixture in &fixtures {
            run_fixture(fixture);
        }
    }
}

fn make_schematic(component_count: usize) -> Schematic {
    let columns = (component_count as f64).sqrt().ceil() as usize;
    let rows = component_count.div_ceil(columns);
    let mut schematic = Schematic::new("Electronics performance fixture");
    let mut positions = Vec::with_capacity(component_count);

    for index in 0..component_count {
        let column = index % columns;
        let row = index / columns;
        let position = Vec2::new(
            (column as f32 - (columns.saturating_sub(1) as f32 * 0.5)) * SCHEMATIC_PITCH.x,
            (row as f32 - (rows.saturating_sub(1) as f32 * 0.5)) * SCHEMATIC_PITCH.y,
        );
        positions.push(position);

        let mut component = ElectronicComponent::resistor("10k");
        component.position = position;
        schematic.add_component(component);
    }

    for row in 0..rows {
        let row_start = row * columns;
        let row_len = (component_count - row_start).min(columns);
        for column in 0..row_len.saturating_sub(1) {
            let left = positions[row_start + column] + Vec2::new(20.0, 0.0);
            let right = positions[row_start + column + 1] - Vec2::new(20.0, 0.0);
            schematic.add_wire(left, right, &format!("N{:03}", row + 1));
        }
    }

    schematic
}

fn make_pcb(schematic: &Schematic) -> PcbLayout {
    let component_count = schematic.components.len();
    let columns = (component_count as f64).sqrt().ceil() as usize;
    let rows = component_count.div_ceil(columns);
    let width = columns.saturating_sub(1) as f32 * PCB_PITCH + 120.0;
    let height = rows.saturating_sub(1) as f32 * PCB_PITCH + 120.0;

    let mut layout = PcbLayout::new("Electronics performance fixture");
    layout.board_outline = BoardOutline::default_rect(width, height);
    layout.sync_from_schematic(schematic);

    for (index, component) in layout.components.iter_mut().enumerate() {
        let column = index % columns;
        let row = index / columns;
        component.position = Vec2::new(
            60.0 + column as f32 * PCB_PITCH,
            60.0 + row as f32 * PCB_PITCH,
        );
    }

    for row in 0..rows {
        let row_start = row * columns;
        let row_len = (component_count - row_start).min(columns);
        for column in 0..row_len.saturating_sub(1) {
            let left_index = row_start + column;
            let right_index = left_index + 1;
            let left = layout.components[left_index].position + Vec2::new(18.0, 0.0);
            let right = layout.components[right_index].position - Vec2::new(18.0, 0.0);
            let net = layout.components[left_index]
                .pad_nets
                .get(1)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("N{:03}", row + 1));
            layout.traces.push(PcbTrace {
                id: Uuid::new_v4(),
                net,
                layer: PcbLayer::TopCopper,
                width: 3.0,
                points: vec![left, right],
            });
        }
    }

    layout
}

fn schematic_camera(component_count: usize) -> CadCamera {
    let columns = (component_count as f64).sqrt().ceil() as usize;
    let rows = component_count.div_ceil(columns);
    let span = Vec2::new(
        columns.saturating_sub(1) as f32 * SCHEMATIC_PITCH.x + 100.0,
        rows.saturating_sub(1) as f32 * SCHEMATIC_PITCH.y + 100.0,
    );
    CadCamera {
        center: Vec2::ZERO,
        zoom: fit_zoom(span),
    }
}

fn pcb_camera(layout: &PcbLayout) -> CadCamera {
    CadCamera {
        center: Vec2::new(layout.board_outline.size().x, layout.board_outline.size().y) * 0.5,
        zoom: fit_zoom(layout.board_outline.size()),
    }
}

fn fit_zoom(span: Vec2) -> f32 {
    let viewport = Vec2::new(VIEWPORT[0] as f32, VIEWPORT[1] as f32);
    (0.82 * (viewport.x / span.x.max(1.0)).min(viewport.y / span.y.max(1.0)))
        .clamp(0.5, 1.6)
}

fn run_fixture(fixture: &Fixture) {
    let canvas_size = Vec2::new(VIEWPORT[0] as f32, VIEWPORT[1] as f32);
    let palette = StudioUiPalette::IndustrialDark;
    let colors = MinimapColors::from_palette(palette);

    let cad_samples = measure_cad(fixture, canvas_size, palette);
    let legacy_samples = measure_legacy_full(fixture, canvas_size, colors);
    let scene_raster_samples = measure_scene_raster(fixture, colors);
    let (cached_pan_samples, pan_uploads) =
        measure_cached_pan(fixture, canvas_size, palette);
    let cad_objects = fixture.scene.objects.len();
    let components = fixture
        .scene
        .objects
        .iter()
        .filter(|object| object.kind == CadObjectKind::Component)
        .count();

    println!(
        "case={} components={} cad_objects={} zoom={:.3} | frame_build p50={:.3}ms p95={:.3}ms | legacy_full p50={:.3}ms p95={:.3}ms | scene_raster_cold p50={:.3}ms p95={:.3}ms | cached_pan p50={:.3}ms p95={:.3}ms uploads={}",
        fixture.name,
        fixture.component_count,
        cad_objects,
        fixture.camera.zoom,
        millis(percentile(&cad_samples, 0.50)),
        millis(percentile(&cad_samples, 0.95)),
        millis(percentile(&legacy_samples, 0.50)),
        millis(percentile(&legacy_samples, 0.95)),
        millis(percentile(&scene_raster_samples, 0.50)),
        millis(percentile(&scene_raster_samples, 0.95)),
        millis(percentile(&cached_pan_samples, 0.50)),
        millis(percentile(&cached_pan_samples, 0.95)),
        pan_uploads,
    );
    black_box(components);
}

fn measure_cad(
    fixture: &Fixture,
    canvas_size: Vec2,
    palette: StudioUiPalette,
) -> Vec<Duration> {
    for sample in 0..WARMUP_COUNT {
        let camera = panned_camera(fixture.camera, sample);
        let frame = build_cad_surface_frame(
            &fixture.scene,
            VIEWPORT[0],
            VIEWPORT[1],
            cad_options(camera, canvas_size, palette),
        );
        black_box(frame);
    }

    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for sample in 0..SAMPLE_COUNT {
        let camera = panned_camera(fixture.camera, sample);
        let options = cad_options(camera, canvas_size, palette);
        let started = Instant::now();
        let frame = build_cad_surface_frame(
            &fixture.scene,
            VIEWPORT[0],
            VIEWPORT[1],
            options,
        );
        let elapsed = started.elapsed();
        black_box(frame);
        samples.push(elapsed);
    }
    samples
}

fn measure_legacy_full(
    fixture: &Fixture,
    canvas_size: Vec2,
    colors: MinimapColors,
) -> Vec<Duration> {
    for sample in 0..WARMUP_COUNT {
        let image = build_rgba(
            colors,
            &fixture.scene,
            panned_camera(fixture.camera, sample),
            canvas_size,
            None,
        );
        black_box(image);
    }

    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for sample in 0..SAMPLE_COUNT {
        let camera = panned_camera(fixture.camera, sample);
        let started = Instant::now();
        let image = build_rgba(colors, &fixture.scene, camera, canvas_size, None);
        let elapsed = started.elapsed();
        black_box(image);
        samples.push(elapsed);
    }
    samples
}

fn measure_scene_raster(fixture: &Fixture, colors: MinimapColors) -> Vec<Duration> {
    for _ in 0..WARMUP_COUNT {
        black_box(build_scene_rgba(colors, &fixture.scene, None));
    }

    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        let image = build_scene_rgba(colors, &fixture.scene, None);
        let elapsed = started.elapsed();
        black_box(image);
        samples.push(elapsed);
    }
    samples
}

fn measure_cached_pan(
    fixture: &Fixture,
    canvas_size: Vec2,
    palette: StudioUiPalette,
) -> (Vec<Duration>, usize) {
    let mut cache = RasterCache::default();
    let revision = (
        fixture.scene.surface,
        fixture.scene.stable_fingerprint(),
        CACHE_UI_REVISION,
    );
    let cached_image = cache
        .update(revision, palette, &fixture.scene, None)
        .expect("the first cache update rasterizes the static overview");
    let mut uploads = 1usize;
    black_box(&cached_image);

    for sample in 0..WARMUP_COUNT {
        let camera = panned_camera(fixture.camera, sample);
        let updated = cache.update(revision, palette, &fixture.scene, None);
        if updated.is_some() {
            uploads += 1;
        }
        assert!(updated.is_none(), "camera pan must reuse cached scene ink");
        let viewport = viewport_rect(&fixture.scene, camera, canvas_size)
            .expect("fixture has a visible minimap viewport");
        let pointer = Vec2::new(
            viewport.x + viewport.width * 0.5,
            viewport.y + viewport.height * 0.5,
        );
        let world = world_at(&fixture.scene, camera, canvas_size, pointer)
            .expect("viewport pointer maps through the static overview");
        black_box((viewport, world));
    }

    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for sample in 0..SAMPLE_COUNT {
        let camera = panned_camera(fixture.camera, sample);
        let started = Instant::now();
        let updated = cache.update(revision, palette, &fixture.scene, None);
        let viewport = viewport_rect(&fixture.scene, camera, canvas_size);
        let pointer = viewport.map(|rect| {
            Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
        });
        let world = pointer.and_then(|point| {
            world_at(&fixture.scene, camera, canvas_size, point)
        });
        let elapsed = started.elapsed();

        if updated.is_some() {
            uploads += 1;
        }
        assert!(updated.is_none(), "camera pan must reuse cached scene ink");
        assert!(viewport.is_some(), "fixture has a visible minimap viewport");
        assert!(world.is_some(), "viewport pointer maps through the static overview");
        black_box((updated, viewport, world));
        samples.push(elapsed);
    }

    let pan_uploads = uploads - 1;
    assert_eq!(pan_uploads, 0, "camera-only updates must not request uploads");
    (samples, pan_uploads)
}

fn cad_options(
    camera: CadCamera,
    canvas_size: Vec2,
    palette: StudioUiPalette,
) -> CadSurfaceOptions {
    let tokens = palette.tokens();
    let mut options = CadSurfaceOptions::default();
    options.clear_color = tokens.canvas;
    options.world_bounds = Some(camera.world_bounds(canvas_size));
    options.collect_hit_regions = false;
    options.raster_scale = 1.0;
    options.grid_step = 20.0;
    options.show_grid = true;
    options.show_labels = true;
    options
}

fn panned_camera(mut camera: CadCamera, sample: usize) -> CadCamera {
    let pixel_offset = PAN_STEP_PIXELS * sample as f32;
    camera.center -= pixel_offset / camera.zoom.max(f32::EPSILON);
    camera
}

fn percentile(samples: &[Duration], percentile: f64) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
