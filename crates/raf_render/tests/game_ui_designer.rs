//! Renderer-based acceptance of the actual interface authoring surfaces.
#[path = "../../raf_editor/src/panels/interface_surface.rs"]
#[allow(dead_code)]
mod interface_surface;
#[path = "../../raf_editor/src/panels/interface_surface_host.rs"]
#[allow(dead_code)]
mod interface_surface_host;
#[path = "../../raf_editor/src/panels/interface_workspace.rs"]
#[allow(dead_code)]
mod interface_workspace;
#[path = "../../raf_editor/src/ui_document_store.rs"]
#[allow(dead_code)]
mod ui_document_store;

use interface_surface_host::InterfaceSurfaceHost;
use interface_workspace::{InterfaceView, PREVIEW_PREFIX};
use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;
use raf_ui::*;

fn action(host: &mut InterfaceSurfaceHost, command: &str, pointer: Option<[f32; 2]>) {
    assert!(host.handle(
        &UiAction::Command {
            name: format!("ui.designer.{command}")
        },
        &UiControlState::default(),
        pointer,
        None,
        raf_core::Language::Spanish
    ));
    assert!(host.diagnostic.is_none(), "{:?}", host.diagnostic);
}

fn document() -> InterfaceSurfaceHost {
    let mut controller = InterfaceSurfaceHost::default();
    controller.document.name = "HUD".into();
    controller
        .add("frame", raf_core::Language::Spanish)
        .unwrap();
    let frame = controller.selected.clone();
    controller.document.find_node_mut(&frame).unwrap().layout =
        UiLayout::absolute(UiRect::new(48.0, 64.0, 304.0, 208.0));
    controller
        .add("label", raf_core::Language::Spanish)
        .unwrap();
    let label = controller.node().unwrap().id.clone();
    let node = controller.document.find_node_mut(&label).unwrap();
    node.style.text = [32, 32, 32, 255];
    node.text_key = Some("ui.designer.title".into());
    node.text_value = None;
    controller.selected = frame.clone();
    controller
        .add("button", raf_core::Language::Spanish)
        .unwrap();
    let button = controller.selected.clone();
    let node = controller.document.find_node_mut(&button).unwrap();
    node.layout = UiLayout::absolute(UiRect::new(24.0, 112.0, 160.0, 40.0));
    node.style.fill = [232, 232, 232, 255];
    controller.selected = frame;
    controller
}

fn scene_pixels(size: [u32; 2]) -> Vec<u8> {
    use raf_core::scene::{NodeColor, Primitive, SceneGraph};
    use raf_render::api_graphic_basic::device::{BasicDevice, BasicDeviceConfig};
    use raf_render::{
        camera::Camera,
        scene_renderer::{RenderOptions, SceneRenderer},
    };
    let mut scene = SceneGraph::new();
    let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
    scene.get_mut(cube).unwrap().color = NodeColor::rgb(160, 160, 160);
    let sphere = scene.add_root_with_primitive("Sphere", Primitive::Sphere);
    scene.get_mut(sphere).unwrap().position = glam::Vec3::new(2.0, 0.0, -1.0);
    let mut renderer = SceneRenderer::new(size[0], size[1]);
    let frame = renderer.build_frame(
        &scene,
        &Camera::default(),
        size[0] as f32,
        size[1] as f32,
        &[],
        [235, 235, 235, 255],
        glam::Vec3::new(0.4, 1.0, 0.2),
        RenderOptions {
            selection_outline: false,
            ..Default::default()
        },
        None,
    );
    let mut device = BasicDevice::new(BasicDeviceConfig {
        allow_gpu: false,
        force_cpu: true,
        ..Default::default()
    });
    device.execute_scene_frame(&frame);
    device.capture_last_scene_rgba().unwrap().rgba8
}

#[test]
fn game_ui_designer_offscreen_layout_and_optional_captures() {
    for (palette, theme, size, view) in [
        (
            StudioUiPalette::IndustrialDark,
            "dark",
            [1280, 720],
            InterfaceView::Live,
        ),
        (
            StudioUiPalette::PaperLight,
            "light",
            [860, 620],
            InterfaceView::Live,
        ),
        (
            StudioUiPalette::IndustrialDark,
            "dark",
            [860, 620],
            InterfaceView::Canvas,
        ),
        (
            StudioUiPalette::PaperLight,
            "light",
            [1280, 720],
            InterfaceView::Canvas,
        ),
    ] {
        let mut controller = document();
        controller.workspace.view = view;
        let left = (size[0] as f32 * 0.19).clamp(150.0, 248.0);
        let right = (size[0] as f32 * 0.24).clamp(232.0, 304.0);
        let center = size[0] as f32 - left - right;
        controller.set_viewport([center, size[1] as f32]);
        let before = controller.document.clone();
        let mut surface = interface_surface::build_interface_workspace(
            palette,
            &controller,
            size.map(|v| v as f32),
        );
        let mut background = UiNode::image(
            "qa.scene",
            UiImage {
                source: UiImageSource::new("qa://scene"),
                fit: UiImageFit::Stretch,
                tint: None,
                options: Default::default(),
            },
        );
        background.layout = UiLayout::absolute(UiRect::new(left, 0.0, center, size[1] as f32));
        surface.root.children.insert(0, background);
        let mut ids = std::collections::BTreeSet::new();
        game_ui::visit_ui_nodes(&surface.root, &mut |node| {
            assert!(ids.insert(node.id.clone()), "Duplicate {}", node.id)
        });
        assert_eq!(
            before, controller.document,
            "Authoring decorations must not enter the document"
        );
        let mut host = CpuUiSurfaceHost::new(surface, [0; 4]);
        let scene_size = [center.round().max(1.0) as u32, size[1]];
        host.images_mut()
            .insert_rgba("qa://scene", scene_size, scene_pixels(scene_size))
            .unwrap();
        controller.seed(&mut host.session_mut().interaction.controls);
        let frame = host.render(size, |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });
        let rect = |id: &str| {
            frame
                .frame
                .layout_boxes
                .iter()
                .find(|b| b.id == id)
                .unwrap_or_else(|| panic!("Missing {id}"))
                .rect
        };
        assert!(rect("ui.designer.navigator").right() <= rect("ui.designer.workspace").x + 0.5);
        assert!(rect("ui.designer.workspace").right() <= rect("ui.designer.inspector").x + 0.5);
        assert!(rect("ui.designer.inspector").right() <= size[0] as f32 + 0.5);
        assert!(rect("ui.designer.toolbar").bottom() <= rect("ui.designer.canvas").y + 0.5);
        for handle in ["nw", "n", "ne", "w", "e", "sw", "s", "se"] {
            let r = rect(&format!(
                "ui.designer.handle.{}.{handle}",
                controller.selected
            ));
            assert!(r.width >= 7.0 && r.height >= 7.0);
        }
        assert!(frame
            .frame
            .hit_regions
            .iter()
            .all(|hit| !hit.id.starts_with("qa.") || (!hit.interactive && !hit.focusable)));
        if let Some(directory) = std::env::var_os("RAF_INTERFACE_QA_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let view = if view == InterfaceView::Live {
                "live"
            } else {
                "canvas"
            };
            let path = directory.join(format!(
                "interfaces-{view}-{theme}-{}x{}.png",
                size[0], size[1]
            ));
            image::save_buffer(
                &path,
                frame.pixels,
                size[0],
                size[1],
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("{}", path.display());
        }
        if view == InterfaceView::Live {
            let menu = interface_surface::build_interface_insert_menu(palette);
            let menu_size = interface_surface::INSERT_MENU_SIZE.map(|v| v as u32);
            let mut menu_host = CpuUiSurfaceHost::new(menu, [0; 4]);
            let menu_frame = menu_host.render(menu_size, |key| {
                raf_core::i18n::t(key, raf_core::Language::Spanish)
            });
            let mut last_bottom = 0.0;
            for kind in ["frame", "label", "image", "button", "input"] {
                let item = menu_frame
                    .frame
                    .layout_boxes
                    .iter()
                    .find(|b| b.id == format!("ui.designer.insert.{kind}"))
                    .unwrap();
                assert!(item.rect.y >= last_bottom);
                assert!(item.rect.right() <= menu_size[0] as f32);
                assert!(item.rect.bottom() <= menu_size[1] as f32);
                last_bottom = item.rect.bottom();
            }
            if let Some(directory) = std::env::var_os("RAF_INTERFACE_QA_DIR") {
                let path = std::path::PathBuf::from(directory)
                    .join(format!("interfaces-insert-{theme}.png"));
                image::save_buffer(
                    &path,
                    menu_frame.pixels,
                    menu_size[0],
                    menu_size[1],
                    image::ColorType::Rgba8,
                )
                .unwrap();
                println!("{}", path.display());
            }
        }
    }
}

#[test]
fn direct_authoring_preserves_anchor_geometry_cancels_and_undoes_one_gesture() {
    let mut host = document();
    host.set_viewport([800.0, 600.0]);
    let selected = host.selected.clone();
    let before = host.document.clone();
    action(&mut host, "anchor:4", None);
    let anchor = host.node().unwrap().layout.anchored.unwrap();
    let geometry = host
        .workspace
        .geometry(host.workspace.viewport, host.preview_size);
    let resolved = anchor.resolve(
        UiRect::new(0.0, 0.0, geometry.logical_size[0], geometry.logical_size[1]),
        &host.node().unwrap().layout,
    );
    assert_eq!(
        resolved,
        before.find_node(&selected).unwrap().layout.rect.unwrap()
    );
    let anchored = host.document.clone();
    action(&mut host, "move.start", Some([100.0, 100.0]));
    action(&mut host, "move.drag", Some([140.0, 120.0]));
    action(&mut host, "move.end", Some([140.0, 120.0]));
    assert_ne!(host.document, anchored);
    action(&mut host, "undo", None);
    assert_eq!(host.document, anchored);
    action(&mut host, "resize.start:nw", Some([100.0, 100.0]));
    action(&mut host, "move.drag", Some([50.0, 40.0]));
    assert!(host.node().unwrap().layout.rect.unwrap().width > resolved.width);
    action(&mut host, "cancel", None);
    assert_eq!(host.document, anchored);
    action(&mut host, "view:canvas", None);
    host.workspace.zoom = Some(0.5);
    action(&mut host, "move.start", Some([0.0, 0.0]));
    action(&mut host, "move.drag", Some([20.0, 10.0]));
    action(&mut host, "move.end", None);
    let moved = host.node().unwrap().layout.anchored.unwrap();
    assert_eq!(
        moved.offset,
        [anchor.offset[0] + 40.0, anchor.offset[1] + 20.0]
    );
    assert!(host
        .document
        .find_node(&format!("{PREVIEW_PREFIX}{selected}"))
        .is_none());
    let before_add = host.document.clone();
    host.add("image", raf_core::Language::Spanish).unwrap();
    action(&mut host, "undo", None);
    assert_eq!(host.document, before_add);
    assert_eq!(host.selected, host.document.root.id);
}

#[test]
fn interface_asset_open_edits_source_and_insert_is_explicit() {
    let base = std::env::var_os("RAF_INTERFACE_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let root = base.join(format!("document-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let relative = std::path::Path::new("sessions/main/ui.ron");
    let mut host = InterfaceSurfaceHost::default();
    assert!(host.sync(&root, relative));
    host.create_asset(raf_core::Language::Spanish).unwrap();
    let asset = host.editing_asset.clone().unwrap();
    host.add("label", raf_core::Language::Spanish).unwrap();
    host.save().unwrap();
    let source = host.document.clone();
    host.sync(&root, relative);
    assert_eq!(
        host.document, source,
        "Session polling must not replace an open asset"
    );
    action(&mut host, "view:scene", None);
    assert!(host.sync(&root, relative));
    assert!(host.document.root.children.is_empty());
    host.open_asset(asset.to_str().unwrap()).unwrap();
    action(&mut host, "export", None);
    action(&mut host, "place", None);
    assert!(host.editing_asset.is_none());
    assert_eq!(host.document.root.children.len(), 1);
    assert_eq!(
        host.document.root.children[0]
            .layout
            .anchored
            .unwrap()
            .relative_size,
        [1.0; 2]
    );
    assert_eq!(ui_document_store::load(&root.join(&asset)).unwrap(), source);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn inspector_commit_preserves_other_drafts_and_escape_restores_only_its_field() {
    let mut host = document();
    let mut controls = UiControlState::default();
    host.seed(&mut controls);
    let x = "ui.designer.field.x";
    let y = "ui.designer.field.y";
    let original_y = controls.text(y).to_owned();
    controls.set_text(y, "unfinished", 4096);
    controls.set_text(x, "80", 4096);
    assert!(host.handle(&UiAction::Command { name: "ui.designer.commit:x".into() },
        &controls, None, None, raf_core::Language::Spanish));
    assert!(host.diagnostic.is_none());
    host.seed(&mut controls);
    assert_eq!(controls.text(x), "80");
    assert_eq!(controls.text(y), "unfinished");
    assert!(host.handle(&UiAction::Command { name: "ui.designer.cancel-field:y".into() },
        &controls, None, None, raf_core::Language::Spanish));
    host.seed(&mut controls);
    assert_eq!(controls.text(y), original_y);
    assert_eq!(controls.text(x), "80");
}
