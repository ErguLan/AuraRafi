use std::sync::Arc;

use glam::Vec2;
use raf_electronics::cad_scene::CadShape;
use raf_electronics::{
    CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadRect, CadScene, CadSurfaceKind,
    ElectronicComponent, PcbComponentPlacement, PcbLayer, PcbLayout, PcbTrace, Schematic,
};

use super::overlays::{drc_shape, DrcShape, HOVER_COLOR, NET_DIM_ALPHA};
use super::paint::{disc_mesh, unit_quad_mesh};
use super::pcb::{object_line_width, PCB_BODY_ALPHA};
use super::schematic::{visual_grid_step, NET_LABEL_ANCHOR_ALPHA};
use super::*;

use crate::api_graphic_basic::command_list::BasicLine;
use crate::api_graphic_basic::device::{BasicDevice, BasicDeviceConfig};

fn options_at(width: u32, height: u32) -> CadSurfaceOptions {
    CadSurfaceOptions {
        world_bounds: Some([0.0, width as f32, 0.0, height as f32]),
        ..CadSurfaceOptions::default()
    }
}

/// Grid off, so a submitted frame contains only document geometry. Line
/// work merges into one batch while it stays adjacent, so a grid batch would
/// otherwise absorb the copper lines and hide the ordering under test.
fn bare_options(width: u32, height: u32) -> CadSurfaceOptions {
    CadSurfaceOptions {
        show_grid: false,
        ..options_at(width, height)
    }
}

fn lines_of(frame: &CadSurfaceFrame) -> Vec<BasicLine> {
    frame
        .frame
        .commands
        .commands()
        .iter()
        .filter_map(|command| match command {
            crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                lines,
                ..
            } => Some(lines.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn mesh_colors(frame: &CadSurfaceFrame) -> Vec<[u8; 4]> {
    frame
        .frame
        .commands
        .commands()
        .iter()
        .flat_map(|command| match command {
            crate::api_graphic_basic::command_list::GraphicCommand::DrawMesh {
                color, ..
            } => {
                vec![*color]
            }
            crate::api_graphic_basic::command_list::GraphicCommand::DrawMeshBatch {
                instances,
                ..
            } => instances.iter().map(|instance| instance.color).collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn manual_object(id: &str, kind: CadObjectKind, color: [u8; 4]) -> CadObject {
    CadObject {
        id: id.to_string(),
        source_id: None,
        kind,
        layer: CadLayerKind::Overlay,
        pick_priority: CadPickPriority::Overlay,
        rect: None,
        shape: CadShape::Rectangle,
        rotation_degrees: 0.0,
        drill: None, stroke_width_world: None,
        points: Vec::new(),
        line_paths: Vec::new(),
        label: None,
        net: None,
        net_id: None,
        color_rgba: color,
    }
}

#[test]
fn physical_conductor_width_tracks_camera_scale_and_dpi() {
    let mut object=manual_object("trace",CadObjectKind::Trace,[255;4]);
    object.stroke_width_world=Some(0.25);
    for scale in [1.0,2.0] {
        let options=CadSurfaceOptions{raster_scale:scale,..Default::default()};
        let density=Density::new(&options,(1000.0*scale) as u32,(1000.0*scale) as u32,[0.0,10.0,0.0,10.0]);
        let pixels=density.pixels(object_line_width(&object,&options,density));
        assert!((pixels-25.0*scale).abs()<0.001);
    }
}

fn physical_capture_scene() -> CadScene {
    let asset = raf_electronics::pcb::parse_kicad_footprint(
        r#"(footprint "CanvasCapture"
              (pad "1" thru_hole circle (at -16 0) (size 7 7) (drill 3) (layers "*.Cu" "*.Mask"))
              (pad "2" smd roundrect (at 16 0 25) (size 7 5) (roundrect_rratio 0.22) (layers "F.Cu" "F.Mask"))
              (pad "3" smd oval (at 0 17 90) (size 5 12) (layers "F.Cu" "F.Mask")))"#,
    )
    .expect("the physical PCB capture footprint parses");
    let footprint_id = asset.id.clone();
    let component_id = ElectronicComponent::resistor("10k").id;
    let mut layout = PcbLayout::new("CAD offscreen evidence");
    layout.footprint_assets.insert(footprint_id.clone(), asset);
    layout.components.push(PcbComponentPlacement {
        component_id,
        designator: "J1".to_string(),
        value: "Header".to_string(),
        footprint: footprint_id,
        position: Vec2::new(90.0, 55.0),
        rotation: 0.0,
        layer: PcbLayer::TopCopper,
        locked: false,
        image_asset: None,
        pad_nets: vec!["GND".to_string(), "SIG".to_string(), "VCC".to_string()],
    });
    layout.traces.push(PcbTrace {
        id: component_id,
        net: "VCC".to_string(),
        layer: PcbLayer::BottomCopper,
        width: 2.0,
        points: vec![Vec2::new(28.0, 84.0), Vec2::new(60.0, 84.0)],
    });
    CadScene::from_pcb(&layout)
}

#[test]
fn cad_surface_records_rects_and_lines() {
    let mut schematic = Schematic::new("Render CAD");
    let mut resistor = ElectronicComponent::resistor("10k");
    resistor.position = Vec2::new(60.0, 64.0);
    let resistor_id = resistor.id;
    let resistor_id_text = resistor_id.to_string();
    schematic.add_component(resistor);
    schematic.add_wire(Vec2::new(12.0, 20.0), Vec2::new(80.0, 20.0), "N001");

    let scene = CadScene::from_schematic(&schematic);
    let frame = build_cad_surface_frame(&scene, 320, 200, CadSurfaceOptions::default());

    assert_eq!(frame.frame.width, 320);
    assert_eq!(frame.frame.height, 200);
    assert!(frame
        .objects
        .iter()
        .any(|object| object.kind == CadObjectKind::Component));
    assert!(frame
        .hit_regions
        .iter()
        .any(|region| region.kind == CadObjectKind::Component));
    assert_eq!(
        frame
            .hit_test(Vec2::new(60.0, 64.0))
            .map(|region| region.kind),
        Some(CadObjectKind::Component)
    );
    assert_eq!(
        frame
            .hit_test(Vec2::new(60.0, 64.0))
            .and_then(|region| region.source_id.as_deref()),
        Some(resistor_id_text.as_str())
    );
    assert!(!frame.frame.commands.commands().is_empty());
    assert!(frame.frame.commands.commands().iter().any(|command| {
        matches!(
            command,
            crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                lines,
                ..
            } if lines.len() >= 8
        )
    }));
    assert!(frame.frame.commands.commands().iter().any(|command| {
        matches!(
            command,
            crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                lines,
                ..
            } if lines.iter().any(|line| line.color == [112, 224, 136, 255])
        )
    }));

    let selected_frame = build_cad_surface_frame(
        &scene,
        320,
        200,
        CadSurfaceOptions {
            selected_source_ids: vec![*resistor_id.as_bytes()],
            ..CadSurfaceOptions::default()
        },
    );
    assert!(selected_frame
        .frame
        .commands
        .commands()
        .iter()
        .any(|command| {
            matches!(
                command,
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                    lines,
                    ..
                } if lines.iter().any(|line| line.color == [255, 172, 64, 255])
            )
        }));
}

#[test]
fn cad_surface_culls_offscreen_objects_before_recording_geometry() {
    let mut schematic = Schematic::new("CAD Culling");
    let mut visible = ElectronicComponent::resistor("10k");
    visible.position = Vec2::new(40.0, 40.0);
    let mut offscreen = ElectronicComponent::resistor("1M");
    offscreen.position = Vec2::new(900.0, 900.0);
    schematic.add_component(visible);
    schematic.add_component(offscreen);

    let scene = CadScene::from_schematic(&schematic);
    let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));

    assert!(frame.objects.iter().all(|object| object
        .rect
        .map(|rect| rect.center.x < 200.0)
        .unwrap_or(true)));
    assert_eq!(frame.frame.stats.visible_entities, 3);
}

#[test]
fn cad_surface_keeps_every_visible_schematic_wire_in_the_line_stream() {
    let mut schematic = Schematic::new("Wire visibility");
    schematic.add_wire(Vec2::new(10.0, 10.0), Vec2::new(90.0, 10.0), "N1");
    schematic.add_wire(Vec2::new(20.0, 20.0), Vec2::new(90.0, 70.0), "N2");
    schematic.add_wire(Vec2::new(30.0, 90.0), Vec2::new(110.0, 90.0), "N3");

    let scene = CadScene::from_schematic(&schematic);
    let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));

    let wire_segments = lines_of(&frame)
        .into_iter()
        .filter(|line| line.color == [216, 221, 227, 255])
        .count();

    // The diagonal wire is represented by the same deterministic elbow
    // route used by the editor, so it contributes two GPU segments. None of
    // these three wires shares an endpoint, so no junction mark is added.
    assert_eq!(wire_segments, 4);
}

#[test]
fn cad_surface_paints_compact_vector_schematic_symbols() {
    let mut schematic = Schematic::new("Vector schematic symbols");
    let mut resistor = ElectronicComponent::resistor("10k");
    resistor.position = Vec2::new(80.0, 60.0);
    schematic.add_component(resistor);

    let scene = CadScene::from_schematic(&schematic);
    let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));

    let symbol_lines = lines_of(&frame)
        .into_iter()
        .filter(|line| line.color == [245, 245, 246, 255])
        .collect::<Vec<_>>();

    assert!(symbol_lines.len() >= 3, "the resistor is rendered from model paths");
    assert!(symbol_lines.iter().all(|line| {
        (line.start.truncate() - Vec2::new(80.0, 60.0)).length() < 32.0
            && (line.end.truncate() - Vec2::new(80.0, 60.0)).length() < 32.0
    }), "symbol ink stays compact around the component center");
}

#[test]
fn visual_grid_step_uses_stable_density_levels() {
    assert_eq!(visual_grid_step(0.25, 25.0), 0.25);
    assert_eq!(visual_grid_step(0.05, 20.0), 0.05);
    assert_eq!(visual_grid_step(20.0, 20.0), 20.0);
    assert_eq!(visual_grid_step(20.0, 6.0), 40.0);
    assert_eq!(visual_grid_step(20.0, 2.5), 100.0);
    assert!(visual_grid_step(20.0, 0.4) >= 200.0);
}

#[test]
fn raster_scale_multiplies_every_emitted_stroke_width() {
    let mut schematic = Schematic::new("Raster scale");
    schematic.add_wire(Vec2::new(10.0, 10.0), Vec2::new(90.0, 10.0), "N1");

    let scene = CadScene::from_schematic(&schematic);
    let single = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));
    let double = build_cad_surface_frame(
        &scene,
        160,
        120,
        CadSurfaceOptions {
            raster_scale: 2.0,
            ..options_at(160, 120)
        },
    );

    let single_max = lines_of(&single)
        .into_iter()
        .map(|line| line.width)
        .fold(0.0f32, f32::max);
    let double_max = lines_of(&double)
        .into_iter()
        .map(|line| line.width)
        .fold(0.0f32, f32::max);
    assert!(single_max > 1.0, "the frame must contain real strokes");
    assert!(
        (double_max - single_max * 2.0).abs() < 0.001,
        "every stroke must scale, got {single_max} then {double_max}"
    );
}

#[test]
fn airwires_render_as_dashed_guides_instead_of_solid_conductor() {
    let scene = CadScene {
        surface: CadSurfaceKind::Pcb,
        objects: vec![CadObject {
            points: vec![Vec2::new(10.0, 10.0), Vec2::new(300.0, 10.0)],
            label: Some("N1".to_string()),
            net: Some("N1".to_string()),
            ..manual_object("airwire:0", CadObjectKind::Airwire, [245, 245, 246, 180])
        }],
    };
    let frame = build_cad_surface_frame(&scene, 400, 200, options_at(400, 200));
    let airwire_lines = lines_of(&frame)
        .into_iter()
        .filter(|line| line.color == [245, 245, 246, 180])
        .count();

    assert!(
        airwire_lines >= 4,
        "a long pending connection must be dashed, got {airwire_lines} segments"
    );
}

#[test]
fn pcb_traces_paint_after_the_component_body_that_used_to_hide_them() {
    let seed = ElectronicComponent::resistor("10k").id;
    let mut layout = PcbLayout::new("Z order");
    layout.components.push(PcbComponentPlacement {
        component_id: seed,
        designator: "R1".to_string(),
        value: "10k".to_string(),
        footprint: "0805".to_string(),
        position: Vec2::new(100.0, 80.0),
        rotation: 0.0,
        layer: PcbLayer::TopCopper,
        locked: false,
        image_asset: None,
        pad_nets: vec!["A".to_string(), "B".to_string()],
    });
    layout.traces.push(PcbTrace {
        id: seed,
        net: "A".to_string(),
        layer: PcbLayer::TopCopper,
        width: 6.0,
        points: vec![Vec2::new(60.0, 80.0), Vec2::new(140.0, 80.0)],
    });

    let scene = CadScene::from_pcb(&layout);
    let frame = build_cad_surface_frame(&scene, 320, 240, bare_options(320, 240));
    let body_index = frame
        .frame
        .commands
        .commands()
        .iter()
        .position(|command| {
            matches!(
                command,
                crate::api_graphic_basic::command_list::GraphicCommand::DrawMesh {
                    color: [34, 34, 38, alpha],
                    ..
                } if *alpha == PCB_BODY_ALPHA
            )
        })
        .expect("component body mesh");
    let trace_index = frame
        .frame
        .commands
        .commands()
        .iter()
        .position(|command| {
            matches!(
                command,
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch { lines, .. }
                if lines.iter().any(|line| line.color == [207, 150, 91, 255])
            )
        })
        .expect("copper trace lines");

    assert!(
        body_index < trace_index,
        "the silhouette must be the backdrop of the copper, not its lid"
    );
}

#[test]
fn pcb_component_bodies_are_no_longer_opaque_copper_lids() {
    let seed = ElectronicComponent::resistor("10k").id;
    let mut layout = PcbLayout::new("Body alpha");
    layout.components.push(PcbComponentPlacement {
        component_id: seed,
        designator: "R1".to_string(),
        value: "10k".to_string(),
        footprint: "0805".to_string(),
        position: Vec2::new(100.0, 80.0),
        rotation: 0.0,
        layer: PcbLayer::TopCopper,
        locked: false,
        image_asset: None,
        pad_nets: vec!["A".to_string(), "B".to_string()],
    });

    let scene = CadScene::from_pcb(&layout);
    let frame = build_cad_surface_frame(&scene, 320, 240, options_at(320, 240));
    assert!(mesh_colors(&frame)
        .iter()
        .any(|color| *color == [34, 34, 38, PCB_BODY_ALPHA]));
    assert!(!mesh_colors(&frame)
        .iter()
        .any(|color| *color == [34, 34, 38, 255]));
}

#[test]
fn pcb_offscreen_renderer_captures_physical_pad_geometry() {
    let scene = physical_capture_scene();
    let frame = build_cad_surface_frame(
        &scene,
        840,
        560,
        CadSurfaceOptions {
            world_bounds: Some([0.0, 420.0, 0.0, 280.0]),
            raster_scale: 2.0,
            ..CadSurfaceOptions::default()
        },
    );
    assert!(mesh_colors(&frame).contains(&[207, 150, 91, 255]));
    assert!(lines_of(&frame)
        .iter()
        .any(|line| line.color == [104, 158, 194, 255]));

    let mut device = BasicDevice::new(BasicDeviceConfig {
        allow_gpu: false,
        force_cpu: true,
        ..BasicDeviceConfig::default()
    });
    device.execute_scene_frame(&frame.frame);
    let capture = device
        .capture_last_scene_rgba()
        .expect("capture comes from the executed CAD renderer frame");
    assert_eq!((capture.width, capture.height), (840, 560));
    let pixel = |x:usize,y:usize| &capture.rgba8[(y*840+x)*4..(y*840+x)*4+4];
    assert_eq!(pixel(212,110), &[207,150,91,255], "The pad interior must be filled, not just an outline");
    assert_eq!(pixel(148,110), &[10,10,11,255], "The plated bore remains a real gap");
    let non_background = capture
        .rgba8
        .chunks_exact(4)
        .filter(|pixel| *pixel != [10, 10, 11, 255])
        .count();
    assert!(non_background > 3_000, "captured board is visibly rendered");

    if let Some(directory) = std::env::var_os("RAF_PCB_CAD_CAPTURE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("capture folder is writable");
        image::save_buffer(
            directory.join("pcb-cad-offscreen.png"),
            &capture.rgba8,
            capture.width,
            capture.height,
            image::ColorType::Rgba8,
        )
        .expect("genuine renderer output saves as PNG");
    }
}

#[test]
fn net_highlight_emphasises_one_net_and_recedes_the_rest() {
    let mut schematic = Schematic::new("Net highlight");
    schematic.add_wire(Vec2::new(10.0, 10.0), Vec2::new(90.0, 10.0), "FOCUS");
    schematic.add_wire(Vec2::new(10.0, 60.0), Vec2::new(90.0, 60.0), "OTHER");
    let scene = CadScene::from_schematic(&schematic);
    let focus_net = scene
        .objects
        .iter()
        .find(|object| {
            object.kind == CadObjectKind::Wire && object.net.as_deref() == Some("FOCUS")
        })
        .and_then(|object| object.net_id)
        .expect("focus net id");

    let frame = build_cad_surface_frame(
        &scene,
        160,
        120,
        CadSurfaceOptions {
            highlight_net_id: Some(focus_net),
            ..options_at(160, 120)
        },
    );
    let lines = lines_of(&frame);
    let focused = lines
        .iter()
        .filter(|line| line.color[3] == 255 && line.color == [216, 221, 227, 255])
        .count();
    let receded = lines
        .iter()
        .filter(|line| line.color[0] == 216 && line.color[3] < 255)
        .count();

    assert!(focused >= 1, "the focused net keeps full contrast");
    assert!(receded >= 1, "the other net must recede");
    assert!(
        lines
            .iter()
            .filter(|line| line.color[0] == 216)
            .all(|line| line.color[3] == 255 || line.color[3] == NET_DIM_ALPHA),
        "only the focused net may keep its full alpha"
    );
}

#[test]
fn hover_uses_a_cold_outline_and_selection_a_warm_halo() {
    let mut schematic = Schematic::new("Hover vs selection");
    let mut resistor = ElectronicComponent::resistor("10k");
    resistor.position = Vec2::new(60.0, 60.0);
    let resistor_id = resistor.id;
    schematic.add_component(resistor);

    let scene = CadScene::from_schematic(&schematic);
    let hovered = build_cad_surface_frame(
        &scene,
        160,
        120,
        CadSurfaceOptions {
            hovered_source_ids: vec![*resistor_id.as_bytes()],
            ..options_at(160, 120)
        },
    );
    let selected = build_cad_surface_frame(
        &scene,
        160,
        120,
        CadSurfaceOptions {
            selected_source_ids: vec![*resistor_id.as_bytes()],
            ..options_at(160, 120)
        },
    );

    assert!(lines_of(&hovered)
        .iter()
        .any(|line| line.color == HOVER_COLOR));
    assert!(!lines_of(&hovered)
        .iter()
        .any(|line| line.color == [255, 172, 64, 255]));
    assert!(lines_of(&selected)
        .iter()
        .any(|line| line.color == [255, 172, 64, 255]));
    assert!(!lines_of(&selected)
        .iter()
        .any(|line| line.color == HOVER_COLOR));
}

#[test]
fn drc_marks_encode_severity_in_shape_not_only_color() {
    let mark = |color: [u8; 4]| CadScene {
        surface: CadSurfaceKind::Pcb,
        objects: vec![CadObject {
            rect: Some(CadRect::new(Vec2::new(40.0, 40.0), Vec2::new(16.0, 16.0))),
            label: Some("violation".to_string()),
            ..manual_object("drc:rule", CadObjectKind::DrcMarker, color)
        }],
    };
    assert_eq!(
        drc_shape(&mark([220, 66, 58, 255]).objects[0]),
        DrcShape::ErrorSquare
    );
    assert_eq!(
        drc_shape(&mark([245, 180, 65, 255]).objects[0]),
        DrcShape::WarningDiamond
    );
    assert_eq!(
        drc_shape(&mark([170, 170, 178, 255]).objects[0]),
        DrcShape::InfoDisc
    );
    // An unreadable severity must never be downgraded to an advisory.
    assert_eq!(
        drc_shape(&mark([0, 0, 0, 255]).objects[0]),
        DrcShape::ErrorSquare
    );

    let frame =
        build_cad_surface_frame(&mark([220, 66, 58, 255]), 160, 120, options_at(160, 120));
    assert!(!frame.frame.commands.commands().is_empty());
}

#[test]
fn shared_wire_endpoints_get_exactly_one_junction_mark() {
    let junction_scene = |shared: bool| CadScene {
        surface: CadSurfaceKind::Schematic,
        objects: vec![
            CadObject {
                points: vec![Vec2::new(20.0, 20.0), Vec2::new(60.0, 20.0)],
                net: Some("N1".to_string()),
                ..manual_object("wire:a", CadObjectKind::Wire, [216, 221, 227, 255])
            },
            CadObject {
                points: if shared {
                    vec![Vec2::new(60.0, 20.0), Vec2::new(60.0, 60.0)]
                } else {
                    vec![Vec2::new(80.0, 20.0), Vec2::new(80.0, 60.0)]
                },
                net: Some("N1".to_string()),
                ..manual_object("wire:b", CadObjectKind::Wire, [216, 221, 227, 255])
            },
        ],
    };

    let shared_frame =
        build_cad_surface_frame(&junction_scene(true), 160, 120, options_at(160, 120));
    let split_frame =
        build_cad_surface_frame(&junction_scene(false), 160, 120, options_at(160, 120));

    let shared_meshes = shared_frame.frame.commands.stats().mesh_instances;
    let split_meshes = split_frame.frame.commands.stats().mesh_instances;
    assert_eq!(
        shared_meshes - split_meshes,
        1,
        "one shared endpoint paints one disc; a near miss paints none"
    );
}

#[test]
fn net_labels_use_a_short_leader_without_painting_a_plate() {
    let mut schematic = Schematic::new("Net label plate");
    schematic.add_wire(Vec2::new(10.0, 20.0), Vec2::new(80.0, 20.0), "SIGNAL_A");
    schematic.add_wire(Vec2::new(80.0, 20.0), Vec2::new(140.0, 20.0), "SIGNAL_A");
    let scene = CadScene::from_schematic(&schematic);
    let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));
    let leaders = lines_of(&frame)
        .into_iter()
        .filter(|line| {
            line.color == [212, 119, 26, NET_LABEL_ANCHOR_ALPHA]
                && (line.start.truncate() - line.end.truncate()).length() <= 4.01
        })
        .count();
    assert_eq!(leaders, 1, "a net with multiple wire segments gets one short leader");
    assert!(!mesh_colors(&frame).iter().any(|color| color[3] == 40));

    let labels_hidden = build_cad_surface_frame(
        &scene,
        160,
        120,
        CadSurfaceOptions {
            show_labels: false,
            ..options_at(160, 120)
        },
    );
    assert!(!lines_of(&labels_hidden)
        .iter()
        .any(|line| line.color == [212, 119, 26, NET_LABEL_ANCHOR_ALPHA]));
}

#[test]
fn hit_regions_are_skippable_without_touching_the_painted_frame() {
    let mut schematic = Schematic::new("Hit regions");
    let mut resistor = ElectronicComponent::resistor("10k");
    resistor.position = Vec2::new(40.0, 40.0);
    schematic.add_component(resistor);

    let scene = CadScene::from_schematic(&schematic);
    let with_regions = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));
    let without_regions = build_cad_surface_frame(
        &scene,
        160,
        120,
        CadSurfaceOptions {
            collect_hit_regions: false,
            ..options_at(160, 120)
        },
    );

    assert!(!with_regions.hit_regions.is_empty());
    assert!(without_regions.hit_regions.is_empty());
    assert_eq!(
        with_regions.frame.commands.stats(),
        without_regions.frame.commands.stats(),
        "skipping pick regions must not change a single submitted command"
    );
}

#[test]
fn unit_quad_and_disc_meshes_are_shared_across_frames() {
    assert!(Arc::ptr_eq(&unit_quad_mesh(), &unit_quad_mesh()));
    assert!(Arc::ptr_eq(&disc_mesh(), &disc_mesh()));
}
