use std::{env, fs::File, io::BufWriter};

use glam::Vec2;
use raf_core::{i18n::t, Language};
use raf_electronics::{CadScene, ElectronicComponent, PcbLayer, PcbLayout, PcbTrace, Schematic};
use raf_render::api_graphic_basic::{
    cad_surface::{build_cad_surface_frame, CadSurfaceOptions},
    ui_surface::{
        craftsmanship_gallery,
        native_workload::{append_cad, append_ui_fixture},
        UiSurfaceDrawList, UiSurfaceImageStore, UiSurfaceSession,
    },
};
use raf_ui::{StudioUiPalette, UiRect};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let output = args
        .next()
        .ok_or("usage: agb_workload FILE [WIDTHxHEIGHT] [en|es] [DPI percent]")?;
    let size = args.next().unwrap_or_else(|| "1280x720".into());
    let (width, height) = size.split_once('x').ok_or("invalid extent")?;
    let size: [u32; 2] = [width.parse()?, height.parse()?];
    if !(640..=3840).contains(&size[0]) || !(360..=2160).contains(&size[1]) {
        return Err("fixture size is outside 640x360–3840x2160".into());
    }
    let language = match args.next().as_deref() {
        Some("es") => Language::Spanish,
        Some("en") | None => Language::English,
        _ => return Err("choose en or es".into()),
    };
    let scale = args
        .next()
        .map(|percent| percent.parse::<u16>())
        .transpose()?
        .unwrap_or(100);
    if ![100, 125, 150, 200].contains(&scale) {
        return Err("choose 100, 125, 150 or 200 percent".into());
    }

    let mut workload = raf_agb::fixtures::scene(true, 4);
    let scale = f32::from(scale) / 100.;
    let palette = StudioUiPalette::IndustrialDark;
    let tokens = palette.tokens();
    let [w, h] = size.map(|dim| dim as f32);
    let gallery = craftsmanship_gallery(palette);
    let padding = gallery.root.layout.padding;
    let content_width = gallery
        .root
        .children
        .iter()
        .map(|child| child.layout.basis[0])
        .fold(0., f32::max);
    let gallery_width = (w * 0.40)
        .max((content_width + padding.left + padding.right) * scale)
        .min(w)
        .floor();
    let mut session = UiSurfaceSession::default();
    let layout = session.build_frame_with_resolved_text_at_scale(
        &gallery,
        (gallery_width / scale).round() as u32,
        (h / scale).round() as u32,
        tokens.background,
        scale,
        |key| t(key, language),
    );
    let strings = session.resolve_text_requests(&layout, |key| t(key, language));
    let draw = UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
        &layout,
        &session.text_atlas,
        scale,
        &strings,
    )
    .scaled_for_output(scale, scale);
    append_ui_fixture(
        &mut workload,
        &draw,
        &session.text_atlas,
        &mut UiSurfaceImageStore::default(),
        UiRect::new(0., 0., gallery_width, h),
        size,
    );

    let cad_width = (w * 0.28).floor();
    let cad_height = (h * 0.29).floor();
    for (index, scene) in [schematic(), pcb()].into_iter().enumerate() {
        let world_bounds = if index == 0 {
            [-12., 252., -15., 165.]
        } else {
            [-12., 242., -12., 152.]
        };
        let options = CadSurfaceOptions {
            clear_color: tokens.canvas,
            world_bounds: Some(world_bounds),
            show_grid: true,
            dotted_grid: true,
            collect_hit_regions: false,
            raster_scale: scale,
            ..Default::default()
        };
        let frame = build_cad_surface_frame(&scene, cad_width as u32, cad_height as u32, options);
        let viewport = UiRect::new(
            w - cad_width - 16.,
            h - (index as f32 + 1.) * (cad_height + 12.),
            cad_width,
            cad_height,
        );
        append_cad(&mut workload, &frame.frame, viewport, size);
    }

    serde_json::to_writer(BufWriter::new(File::create(output)?), &workload)?;
    Ok(())
}

fn schematic() -> CadScene {
    let mut schematic = Schematic::new("RC");
    let mut resistor = ElectronicComponent::resistor("1 kΩ");
    resistor.designator = "R1".into();
    resistor.position = Vec2::new(100., 56.);
    schematic.add_component(resistor);
    let mut capacitor = ElectronicComponent::capacitor("100 nF");
    capacitor.designator = "C1".into();
    capacitor.position = Vec2::new(175., 106.);
    schematic.add_component(capacitor);
    schematic.add_wire_path(&[[28., 56.], [76., 56.]].map(Vec2::from_array), "IN");
    schematic.add_wire_path(
        &[[124., 56.], [205., 56.], [205., 106.], [199., 106.]].map(Vec2::from_array),
        "OUT",
    );
    schematic.add_wire_path(&[[151., 106.], [28., 106.]].map(Vec2::from_array), "GND");
    CadScene::from_schematic(&schematic)
}

fn pcb() -> CadScene {
    let mut board = PcbLayout::new("RC");
    board.board_outline = raf_electronics::BoardOutline::default_rect(230., 140.);
    let routes = [
        (
            [[22., 35.], [94., 35.], [144., 85.], [206., 85.]],
            "IN",
            PcbLayer::TopCopper,
        ),
        (
            [[22., 75.], [88., 75.], [132., 119.], [206., 119.]],
            "GND",
            PcbLayer::BottomCopper,
        ),
        (
            [[125., 22.], [168., 65.], [186., 65.], [206., 45.]],
            "OUT",
            PcbLayer::TopCopper,
        ),
    ];
    for (index, (points, net, layer)) in routes.into_iter().enumerate() {
        board.traces.push(PcbTrace {
            id: uuid::Uuid::from_u128(index as u128 + 1),
            net: net.into(),
            layer,
            width: 2.8,
            points: points.map(Vec2::from_array).to_vec(),
        });
    }
    CadScene::from_pcb(&board)
}
