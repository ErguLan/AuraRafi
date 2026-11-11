use raf_render::api_graphic_basic::ui_surface::*;
use raf_ui::*;

#[test]
fn responsive_components_reserve_height_minima_and_image_geometry_before_residency() {
    let image = |id: &str| {
        UiImageView::new("test://image")
            .build(id, UiLayout::default().fill_width().with_aspect_ratio(2.0))
    };
    let card = |id: &str| {
        UiNode::column(id, [image(&format!("{id}.image"))])
            .fit_content()
            .fill_width()
            .map_layout(|mut layout| {
                layout.grow = 1.0;
                layout.min_size[0] = 120.0;
                layout
            })
    };
    let cards = UiNode::row("cards", [card("a"), card("b")])
        .fit_content()
        .fill_width()
        .with_gap(8.0)
        .with_compact(UiCompactMode::Stack);
    let footer = UiNode::new("footer", UiNodeKind::Panel).with_layout(UiLayout::fixed(40.0, 20.0));
    let surface = UiSurface::new(
        "layout",
        StudioUiPalette::IndustrialDark,
        UiNode::column("root", [cards, footer]).with_gap(8.0),
    );
    let mut host = CpuUiSurfaceHost::new(surface.clone(), [0; 4]);
    for width in [360, 200] {
        let before = host.render([width, 360], str::to_owned).frame.clone();
        let rect = |id: &str| {
            before
                .layout_boxes
                .iter()
                .find(|b| b.id == id)
                .unwrap()
                .rect
        };
        assert!((rect("a.image").height * 2.0 - rect("a.image").width).abs() < 0.5);
        assert!(rect("cards").bottom() <= rect("footer").y + 0.5);
        assert!(rect("a.image").bottom() <= rect("cards").bottom() + 0.5);
        assert!(rect("b.image").bottom() <= rect("cards").bottom() + 0.5);
        if width == 200 {
            assert!(rect("a").bottom() <= rect("b").y + 0.5);
        }
        host.images_mut()
            .insert_rgba("test://image", [1, 1], vec![255; 4])
            .unwrap();
        let after = host.render([width, 360], str::to_owned).frame.clone();
        assert_eq!(
            before
                .layout_boxes
                .iter()
                .map(|b| b.rect)
                .collect::<Vec<_>>(),
            after
                .layout_boxes
                .iter()
                .map(|b| b.rect)
                .collect::<Vec<_>>()
        );
        let inspection = inspect_surface(&surface, &after, Some(host.images()), 64);
        assert!(!inspection.truncated);
        assert!(inspection.issues.is_empty(), "{:?}", inspection.issues);
    }
    let flexible = |id: &str| {
        UiNode::new(id, UiNodeKind::Panel).with_layout(UiLayout {
            basis: [100.0, 20.0],
            min_size: [60.0, 20.0],
            ..UiLayout::default()
        })
    };
    let row = UiSurface::new(
        "minimum",
        StudioUiPalette::IndustrialDark,
        UiNode::row("minimum.root", [flexible("one"), flexible("two")]).with_gap(8.0),
    );
    let frame = row.build_frame(100, 40, [0; 4]);
    let a = frame
        .layout_boxes
        .iter()
        .find(|b| b.id == "one")
        .unwrap()
        .rect;
    let b = frame
        .layout_boxes
        .iter()
        .find(|b| b.id == "two")
        .unwrap()
        .rect;
    assert!(a.width >= 60.0 && b.width >= 60.0 && a.right() <= b.x);
}

#[test]
fn actual_component_gallery_exposes_resources_actions_and_optional_cpu_captures() {
    for (palette, name, logical, scale) in [
        (StudioUiPalette::IndustrialDark, "dark", [640, 560], 1.0),
        (StudioUiPalette::PaperLight, "light", [640, 560], 1.0),
        (
            StudioUiPalette::IndustrialDark,
            "dark-narrow",
            [360, 720],
            1.25,
        ),
        (
            StudioUiPalette::PaperLight,
            "light-narrow",
            [360, 720],
            1.25,
        ),
    ] {
        let surface = component_gallery(palette);
        let mut host = CpuUiSurfaceHost::new(surface.clone(), palette.tokens().surface);
        let physical = logical.map(|v| (v as f32 * scale).round() as u32);
        let frame = host.render_at_scale(physical, logical, scale, |key| {
            let translated = raf_core::i18n::t(key, raf_core::Language::Spanish);
            assert_ne!(translated, key, "Missing translation: {key}");
            translated
        });
        let frame_data = frame.frame.clone();
        let pixels = frame.pixels.to_vec();
        let inspection = inspect_surface(&surface, &frame_data, Some(host.images()), 128);
        assert!(inspection.issues.is_empty(), "{:?}", inspection.issues);
        let ready = inspection
            .nodes
            .iter()
            .find(|n| n.id == "components.preview.image")
            .unwrap();
        assert_eq!(
            ready.image.as_ref().unwrap().status,
            Some(UiImageResourceStatus::Ready)
        );
        // Resource residency alone must not accept white artwork disappearing
        // into a light background. Count visible ink inside the actual image.
        let bounds = ready.rect;
        let mut visible_ink = 0;
        for y in ((bounds.y * scale).ceil() as u32)
            ..((bounds.bottom() * scale).floor() as u32).min(physical[1])
        {
            for x in ((bounds.x * scale).ceil() as u32)
                ..((bounds.right() * scale).floor() as u32).min(physical[0])
            {
                let index = ((y * physical[0] + x) * 4) as usize;
                if (0..3).any(|channel| {
                    pixels[index + channel].abs_diff(palette.tokens().surface[channel]) > 32
                }) {
                    visible_ink += 1;
                }
            }
        }
        assert!(
            visible_ink > 64,
            "The registered artwork needs visible contrast in {name}"
        );
        let missing = inspection
            .nodes
            .iter()
            .find(|n| n.id == "components.missing.image")
            .unwrap();
        assert_eq!(
            missing.image.as_ref().unwrap().status,
            Some(UiImageResourceStatus::Missing)
        );
        let save = surface.root.find("components.save").unwrap();
        assert!(save.focusable && !save.event_handlers.is_empty());
        assert!(!frame_data
            .focus_order
            .iter()
            .any(|id| id == "components.disabled"));
        let save_rect = frame_data
            .layout_boxes
            .iter()
            .find(|b| b.id == "components.save")
            .unwrap()
            .rect;
        host.session_mut().interaction.focus.focused = Some("components.save".into());
        let focused = host.render_at_scale(physical, logical, scale, |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });
        assert_eq!(
            focused
                .frame
                .layout_boxes
                .iter()
                .find(|b| b.id == "components.save")
                .unwrap()
                .rect,
            save_rect
        );
        if let Some(directory) = std::env::var_os("RAF_INTERFACE_QA_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join(format!("rafui-components-{name}.png"));
            image::save_buffer(
                &path,
                &pixels,
                physical[0],
                physical[1],
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("{}", path.display());
        }
        assert!(inspect_surface(&surface, &frame_data, None, 1).truncated);
    }
}
