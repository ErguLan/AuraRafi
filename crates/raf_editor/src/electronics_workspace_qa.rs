//! CPU-only integrated captures for the Electronics workbench composition.
//!
//! These checks compose the same retained panel builders as the native
//! workbench and pair them with a software-rendered ApiGraphicBasic CAD frame.

use glam::Vec2;
use raf_core::config::Language;
use raf_core::project::ProjectType;
use raf_electronics::schematic::component_pin_world_position;
use raf_electronics::{CadObjectKind, CadSurfaceKind};
use raf_render::api_graphic_basic::cad_surface::build_cad_surface_frame;
use raf_render::api_graphic_basic::device::{BasicDevice, BasicDeviceConfig};
use raf_render::api_graphic_basic::ui_surface::{
    CpuUiSurfaceHost, StudioUiPalette, UiImage, UiImageFit, UiImageSource, UiSurface,
};
use raf_ui::{
    UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiStyle, UiStyleRule, UiStyleSheet,
};

use crate::application_bar_host::{register_bar_images, register_electronics_images};
use crate::application_bar_surface::build_application_bar_surface;
use crate::editor_layout::{
    EditorFrameLayout, EditorLayoutRequest, EditorRect, EditorWorkbenchKind, ELECTRONICS_RAIL_WIDTH,
};
use crate::electronics_controller::NativeElectronicsEditor;
use crate::electronics_minimap::{self, MinimapColors};
use crate::native_workbench::electronics_model::{inspector_data, ElectronicsWorkbenchData};
use crate::panels::editor_bottom_dock_host::EditorBottomDockHost;
use crate::panels::editor_bottom_dock_surface::build_tab_strip_surface_sized;
use crate::panels::editor_status_surface::build_status_surface;
use crate::panels::electronics_canvas_overlay_surface::{
    build_electronics_framed_overlay_surface, ElectronicsCanvasOverlayParams,
};
use crate::panels::electronics_inspector_surface::{
    build_electronics_inspector_surface_with_presentation, ElectronicsInspectorPresentation,
};
use crate::panels::electronics_navigator_surface::build_electronics_navigator_surface;
use crate::panels::electronics_toolbar_surface::build_electronics_toolbar_surface;
use crate::panels::pcb_tool_options_surface::build_pcb_tool_options_surface_with_zoom;

const TEST_CLEAR: [u8; 4] = [10, 10, 11, 255];
const QA_CAPTURE_ENV: &str = "RAF_ELECTRONICS_WORKSPACE_QA_DIR";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrated_workspace_cpu_capture_composes_schematic_and_pcb_fixtures() {
        // The first frame follows the default open schematic library. PCB uses
        // the compact rail. Both retain the selected component inspector.
        for (surface, size) in [
            (CadSurfaceKind::Schematic, [1600, 900]),
            (CadSurfaceKind::Pcb, [1600, 1000]),
        ] {
            let pixels = capture_workspace(surface, size);
            assert_eq!(pixels.len(), size[0] as usize * size[1] as usize * 4);
        }
    }

    fn capture_workspace(surface: CadSurfaceKind, size: [u32; 2]) -> Vec<u8> {
        let palette = StudioUiPalette::IndustrialDark;
        let language = Language::Spanish;
        let mut editor = electronics_fixture(surface);
        editor.apply_engine_settings(&raf_core::EngineSettings {
            language,
            ..Default::default()
        });
        editor.apply_project_settings(&raf_core::project::ProjectSettings::default());
        let left_width = 260.0;
        let layout = EditorFrameLayout::compute(EditorLayoutRequest {
            logical_size: [size[0] as f32, size[1] as f32],
            kind: EditorWorkbenchKind::Electronics,
            left_visible: true,
            right_visible: true,
            left_width,
            right_width: 326.0,
            bottom_expanded: true,
            bottom_height: 220.0,
        });
        let left = layout.left_panel.expect("Electronics navigator/rail");
        let right = layout.right_panel.expect("selected item inspector");
        assert!((left.width - left_width).abs() < f32::EPSILON);
        assert!((right.width - 326.0).abs() < f32::EPSILON);
        assert!((layout.bottom_dock.height - 220.0).abs() < f32::EPSILON);

        let cad_rect = layout.electronics_canvas_for_pcb(surface == CadSurfaceKind::Pcb);
        // Use the same camera initialization and theme-derived render options
        // as the native canvas. Calling camera() before this leaves a new editor
        // uninitialized and produces a real scene rendered outside the frame.
        let _ = editor.render_options(cad_rect, palette, 1.0);
        if surface == CadSurfaceKind::Pcb {
            editor.set_tool(crate::electronics_controller::ElectronicsTool::Route);
            let start = editor.pcb().pad_world_position(0, 1).unwrap();
            let point = editor
                .camera()
                .screen_from_world(start, Vec2::new(cad_rect.width, cad_rect.height));
            let mut down = raf_core::PointerButtonSet::default();
            down.insert(raf_core::PointerButton::Primary);
            let input = raf_core::InputSnapshot {
                pointer_position: Some([point.x + cad_rect.x, point.y + cad_rect.y]),
                pointer_pressed_buttons: down,
                pointer_buttons_down: down,
                window_focused: true,
                ..Default::default()
            };
            let mut router = raf_core::InputRouter::default();
            router.reconcile_input(&input);
            editor.process_input(&input, &mut router, cad_rect);
            assert!(
                editor.pcb().traces.is_empty(),
                "The capture arms a real route without committing copper"
            );
            assert!(editor.pcb_routing_hint().unwrap().0.contains("RC_OUT"));
        }
        let cad_options = editor.render_options(cad_rect, palette, 1.0);
        let cad_clear = cad_options.clear_color;
        let selection = editor.selection().expect("fixture selection");
        let cad = build_cad_surface_frame(
            editor.scene(),
            cad_rect.width.round() as u32,
            cad_rect.height.round() as u32,
            cad_options,
        );
        let expected_kind = if surface == CadSurfaceKind::Schematic {
            CadObjectKind::Wire
        } else {
            CadObjectKind::Pad
        };
        assert!(
            editor
                .scene()
                .objects
                .iter()
                .any(|object| object.kind == expected_kind),
            "fixture must contain real {expected_kind:?} geometry"
        );
        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: false,
            force_cpu: true,
            ..BasicDeviceConfig::default()
        });
        let _ = device.execute_scene_frame(&cad.frame);
        let cad_capture = device
            .capture_last_scene_rgba()
            .expect("BasicDevice CPU CAD frame");
        assert_eq!(cad_capture.width, cad_rect.width.round() as u32);
        assert_eq!(cad_capture.height, cad_rect.height.round() as u32);
        if surface == CadSurfaceKind::Pcb {
            assert!(
                cad_capture
                    .rgba8
                    .chunks_exact(4)
                    .any(|p| p[1] > 170 && p[0] < 130 && p[2] > 140),
                "The routed net keeps visible green target rings in the executed CAD capture"
            );
        }
        assert!(
            cad_capture
                .rgba8
                .chunks_exact(4)
                .filter(|pixel| *pixel != cad_clear)
                .count()
                > 500,
            "CAD fixture must render visible geometry"
        );

        let workbench = ElectronicsWorkbenchData::from_editor(Some(&editor));
        let inspector = inspector_data(&editor).expect("selected component projection");
        let footprint_key = editor
            .schematic()
            .components
            .iter()
            .find(|component| component.id == selection.source_id)
            .map(|component| component.footprint.as_str())
            .or_else(|| {
                editor
                    .pcb()
                    .components
                    .iter()
                    .find(|component| component.component_id == selection.source_id)
                    .map(|component| component.footprint.as_str())
            })
            .expect("selected component footprint reference");
        let footprint = editor
            .pcb()
            .footprint_assets
            .get(footprint_key)
            .expect("fixture's parsed physical footprint");
        let footprint_bounds = footprint.bounds_mm().expect("physical pad bounds");
        let preview_key = format!("electronics://qa/footprint/{}", footprint.id);
        let presentation = ElectronicsInspectorPresentation {
            component_symbol_image_key: Some(
                editor.component_asset_key(selection.source_id).into(),
            ),
            component_name: Some("Resistor".into()),
            board_label_visibility: Some(editor.pcb_label_visibility()),
            footprint_name: Some(footprint.name.clone()),
            footprint_dimensions_mm: Some([
                footprint_bounds.width_mm() as f32,
                footprint_bounds.height_mm() as f32,
            ]),
            footprint_pad_count: Some(footprint.pads.len()),
            footprint_preview_image_key: Some(preview_key.clone()),
            pin_pad_mapping: inspector_pin_pad_mapping(&editor, selection.source_id, footprint),
        };

        let mut root = UiNode::new("electronics.workspace.qa", UiNodeKind::Root)
            .with_layout(UiLayout {
                overflow: UiOverflow::Clip,
                ..UiLayout::fill(UiFlow::None)
            })
            .with_style(UiStyle::transparent())
            .with_child(
                UiNode::image(
                    "electronics.workspace.qa.cad-image",
                    UiImage {
                        options: Default::default(),
                        source: UiImageSource::new("electronics://qa/cad-frame"),
                        fit: UiImageFit::Stretch,
                        tint: None,
                    },
                )
                .with_layout(UiLayout::absolute(UiRect::new(
                    cad_rect.x,
                    cad_rect.y,
                    cad_rect.width,
                    cad_rect.height,
                ))),
            );
        let mut rules: Vec<UiStyleRule> = Vec::new();

        root = append_surface(
            root,
            &mut rules,
            compact_electronics_application_bar(
                build_application_bar_surface(palette, "RC Sensor", ProjectType::Electronics, None),
                size[0] as f32,
            ),
            layout.application_bar,
        );

        let toolbar = build_electronics_toolbar_surface(
            palette,
            surface,
            editor.tool(),
            editor.grid_visible(),
            editor.labels_visible(),
            editor.snap_enabled(),
            editor.can_undo(),
            editor.can_redo(),
            editor.selection().is_some(),
            editor.can_rotate_selection(),
            layout.window.width,
        );
        let toolbar_rect = layout.electronics_toolbar_rect();
        root = append_surface(root, &mut rules, toolbar, toolbar_rect);

        root = append_surface(
            root,
            &mut rules,
            crate::panels::electronics_desktop_surface::build_electronics_document_chrome(
                palette,
                &editor,
                layout.canvas,
            ),
            layout.canvas,
        );
        if surface == CadSurfaceKind::Pcb {
            let width = (layout.canvas.width
                - crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH)
                .max(0.0);
            let height = crate::panels::pcb_tool_options_surface::pcb_tool_options_height(width);
            root = append_surface(
                root,
                &mut rules,
                build_pcb_tool_options_surface_with_zoom(
                    palette,
                    editor.pcb_route_layer(),
                    editor.pcb_route_width(),
                    editor.grid_step(),
                    editor.camera().zoom * 25.4 / 96.0 * 100.0,
                    width,
                    language,
                    None,
                ),
                EditorRect::new(
                    layout.canvas.x + crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH,
                    layout.canvas.y + crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT,
                    width,
                    height,
                ),
            );
        }

        root = append_surface(
            root,
            &mut rules,
            build_electronics_framed_overlay_surface(
                palette,
                &editor,
                cad_rect,
                ElectronicsCanvasOverlayParams::new(1.0, language),
            ),
            crate::panels::electronics_canvas_overlay_surface::electronics_overlay_rect(
                surface == CadSurfaceKind::Pcb,
                cad_rect,
            ),
        );

        if left.width <= ELECTRONICS_RAIL_WIDTH + 0.5 {
            root = append_surface(
                root,
                &mut rules,
                crate::native_workbench::electronics_layout::build_electronics_rail(
                    palette,
                    surface == CadSurfaceKind::Pcb,
                ),
                left,
            );
        } else {
            root = append_surface(
                root,
                &mut rules,
                build_electronics_navigator_surface(
                    palette,
                    "library",
                    "",
                    &workbench.schematic_name,
                    workbench.counts,
                    &workbench.components,
                    &workbench.wires,
                    &workbench.library,
                    surface,
                    workbench.layer_visibility,
                ),
                left,
            );
        }

        root = append_surface(
            root,
            &mut rules,
            build_electronics_inspector_surface_with_presentation(
                palette,
                &inspector.title,
                &inspector.fields,
                &inspector.pins,
                &raf_core::session::ProjectSessionRegistry::new(ProjectType::Electronics),
                crate::panels::inspector_surface::InspectorTab::Properties,
                editor.fabrication_status(),
                Some(&presentation),
            ),
            right,
        );

        let mut dock = EditorBottomDockHost::default();
        dock.reset_group(ProjectType::Electronics);
        for (group_id, column) in dock.layout().resolve_columns(layout.bottom_dock.width, 4.0) {
            let group = dock
                .groups()
                .iter()
                .find(|group| group.id == group_id)
                .expect("resolved Electronics dock group");
            root = append_surface(
                root,
                &mut rules,
                build_tab_strip_surface_sized(
                    palette,
                    group,
                    layout.bottom_dock.height <= 34.0,
                    None,
                    column.width,
                ),
                EditorRect::new(
                    layout.bottom_dock.x + column.x,
                    layout.bottom_dock.y,
                    column.width,
                    layout.bottom_dock.height.min(34.0).max(1.0),
                ),
            );
        }

        let console = crate::console::ConsolePanel::default();
        root = append_surface(
            root,
            &mut rules,
            crate::panels::console_surface::build_console_table_surface(
                palette,
                &console,
                false,
                &[],
                None,
            ),
            EditorRect::new(
                layout.bottom_dock.x,
                layout.bottom_dock.y + 34.0,
                layout.bottom_dock.width,
                layout.bottom_dock.height - 34.0,
            ),
        );

        let translate = |key: &str| raf_core::i18n::t(key, language);
        let selected_kind = editor
            .selection()
            .map(|selection| {
                let key = match selection.kind {
                    crate::electronics_controller::ElectronicsSelectionKind::Component => {
                        "app.electronics_component"
                    }
                    crate::electronics_controller::ElectronicsSelectionKind::Pin => {
                        "app.electronics_pin"
                    }
                    crate::electronics_controller::ElectronicsSelectionKind::Wire => {
                        "app.electronics_wire"
                    }
                    crate::electronics_controller::ElectronicsSelectionKind::Trace => {
                        "app.electronics_trace"
                    }
                    crate::electronics_controller::ElectronicsSelectionKind::Other => {
                        "app.electronics_other"
                    }
                };
                format!(
                    "{}: {}",
                    translate("app.electronics_selection"),
                    translate(key)
                )
            })
            .unwrap_or_else(|| translate("app.electronics_no_selection"));
        let notice = editor
            .surface_errors()
            .last()
            .cloned()
            .or_else(|| {
                editor
                    .sync_is_stale()
                    .then(|| translate("electronics.sync.pcb_stale"))
            })
            .or_else(|| editor.fabrication_status().map(str::to_string))
            .unwrap_or_else(|| {
                translate(if editor.is_dirty() {
                    "app.electronics_modified"
                } else {
                    "app.electronics_saved"
                })
            });
        let status_items = [
            workbench.schematic_name.clone(),
            translate(if surface == CadSurfaceKind::Schematic {
                "app.electronics_schematic_tab"
            } else {
                "app.electronics_pcb_tab"
            }),
            selected_kind,
            format!(
                "{}: {}",
                translate("app.schematic_components"),
                workbench.counts.0
            ),
            format!(
                "{}: {}",
                translate("app.electronics_connections"),
                workbench.counts.1
            ),
            format!(
                "{}: {}",
                translate("app.schematic_nets"),
                workbench.counts.2
            ),
            format!(
                "{}: {:.2} mm",
                translate("app.electronics_grid_status"),
                editor.grid_step()
            ),
            notice,
        ];
        root = append_surface(
            root,
            &mut rules,
            build_status_surface(palette, &status_items),
            layout.status_bar,
        );

        let mut surface_document = UiSurface::new("electronics.workspace.qa", palette, root);
        surface_document.style_sheet = UiStyleSheet { rules };
        let mut host = CpuUiSurfaceHost::new(surface_document, TEST_CLEAR);
        // Match NativeGameWorkbench::sync: the live value is placed in RafUI's
        // control state, otherwise a correct model value renders as an empty
        // input with only the placeholder visible in a CPU capture.
        let selected_value = editor
            .schematic()
            .components
            .iter()
            .find(|component| component.id == selection.source_id)
            .map(|component| component.value.as_str())
            .expect("selected schematic component value");
        host.session_mut().interaction.controls.set_text(
            "electronics.inspector.value",
            selected_value,
            256,
        );
        assert_eq!(
            host.session()
                .interaction
                .controls
                .text("electronics.inspector.value"),
            selected_value
        );
        if let Some((target, position, rotation, _)) = editor.inspector_transform() {
            for (axis, value) in [("x", position.x), ("y", position.y), ("rotation", rotation)] {
                host.session_mut().interaction.controls.set_text(
                    &format!("electronics.inspector.transform.{target}:{axis}"),
                    format!("{value:.3}"),
                    24,
                );
            }
        }
        register_bar_images(host.images_mut());
        register_electronics_images(host.images_mut());
        host.images_mut()
            .insert_rgba(
                "electronics://qa/cad-frame",
                [cad_capture.width, cad_capture.height],
                cad_capture.rgba8,
            )
            .expect("register software-rendered CAD frame");
        let (minimap_size, minimap_pixels) = electronics_minimap::build_scene_rgba(
            MinimapColors::from_tokens(&palette.tokens()),
            editor.scene(),
            editor.selection(),
        );
        host.images_mut()
            .insert_rgba(electronics_minimap::IMAGE_KEY, minimap_size, minimap_pixels)
            .expect("register scene-derived minimap image");
        let (footprint_preview_size, footprint_preview_pixels) =
            electronics_minimap::footprint_preview_rgba(footprint, palette);
        host.images_mut()
            .insert_rgba(
                &preview_key,
                footprint_preview_size,
                footprint_preview_pixels,
            )
            .expect("register real physical footprint preview");

        let frame = host.render(size, |key| raf_core::i18n::t(key, language));
        let required_nodes: &[&str] = if left.width <= ELECTRONICS_RAIL_WIDTH + 0.5 {
            &[
                "electronics.workspace.qa.cad-image",
                "electronics.rail.library",
                "electronics.pcb.options",
                "electronics.inspector.footprint.preview",
                "electronics.canvas.minimap.image",
                "bottom.collapse",
            ]
        } else {
            &[
                "electronics.workspace.qa.cad-image",
                "electronics.navigator.header",
                "electronics.library.list",
                "electronics.inspector.footprint.preview",
                "electronics.canvas.minimap.image",
                "bottom.collapse",
            ]
        };
        for id in required_nodes {
            assert!(
                frame.frame.layout_boxes.iter().any(|node| node.id == *id),
                "integrated CPU capture contains {id}"
            );
        }
        let inspector_preview = frame
            .frame
            .layout_boxes
            .iter()
            .find(|node| node.id == "electronics.inspector.footprint.preview")
            .expect("inspector physical preview layout");
        assert!(inspector_preview.rect.width > 0.0 && inspector_preview.rect.height > 0.0);
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel != TEST_CLEAR),
            "composed workbench must paint visible content"
        );

        if let Some(directory) = std::env::var_os(QA_CAPTURE_ENV) {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).expect("create integrated QA capture folder");
            let view_name = if surface == CadSurfaceKind::Schematic {
                "schematic"
            } else {
                "pcb"
            };
            image::save_buffer(
                directory.join(format!(
                    "electronics-workspace-{view_name}-{}x{}.png",
                    size[0], size[1]
                )),
                frame.pixels,
                size[0],
                size[1],
                image::ColorType::Rgba8,
            )
            .expect("save optional integrated Electronics CPU capture");
        }

        frame.pixels.to_vec()
    }

    fn electronics_fixture(surface: CadSurfaceKind) -> NativeElectronicsEditor {
        // A test-only 0805 KiCad footprint supplies genuine copper-pad geometry
        // for the inspector preview and physical PCB surface.
        let asset = raf_electronics::pcb::parse_kicad_footprint(
            r#"(footprint "R_0805_2012Metric"
                (pad "1" smd rect (at -1 0) (size 1 1.4) (layers "F.Cu" "F.Mask"))
                (pad "2" smd roundrect (at 1 0) (size 1 1.4)
                    (roundrect_rratio 0.2) (layers "F.Cu" "F.Mask")))"#,
        )
        .expect("test-only valid KiCad footprint");
        let footprint_id = asset.id.clone();
        let mut editor = NativeElectronicsEditor::empty("RC Sensor");
        let mut resistor = raf_electronics::component::ElectronicComponent::resistor("10k");
        resistor.designator = "R1".to_string();
        resistor.position = Vec2::new(-60.0, 0.0);
        resistor.footprint = footprint_id.clone();
        let mut capacitor = raf_electronics::component::ElectronicComponent::capacitor("100nF");
        capacitor.designator = "C1".to_string();
        capacitor.position = Vec2::new(70.0, 0.0);
        capacitor.footprint = footprint_id.clone();
        editor.schematic.add_component(resistor);
        editor.schematic.add_component(capacitor);
        let resistor_pin = component_pin_world_position(
            &editor.schematic.components[0],
            &editor.schematic.components[0].pins[1],
        );
        let capacitor_pin = component_pin_world_position(
            &editor.schematic.components[1],
            &editor.schematic.components[1].pins[0],
        );
        editor
            .schematic
            .add_wire(resistor_pin, capacitor_pin, "RC_OUT");
        editor.pcb.footprint_assets.insert(footprint_id, asset);
        editor.rebuild_scene();
        let _ = editor.sync_pcb_from_schematic();
        if surface == CadSurfaceKind::Pcb {
            // Explicit small-board fixture in millimeters. This is real test
            // document geometry, not a zoom-only illustration of a large PCB.
            editor.pcb.board_outline = raf_electronics::pcb::BoardOutline::default_rect(40.0, 22.0);
            editor.pcb.components[0].position = Vec2::new(6.0, 6.0);
            editor.pcb.components[1].position = Vec2::new(24.0, 6.0);
            editor.pcb.rebuild_airwires();
            editor.set_surface(CadSurfaceKind::Pcb);
        }
        assert!(editor.select_component(0), "select fixture component");
        editor
    }

    fn inspector_pin_pad_mapping(
        editor: &NativeElectronicsEditor,
        selected_id: uuid::Uuid,
        footprint: &raf_electronics::pcb::FootprintAsset,
    ) -> Vec<(String, String)> {
        let Some(component) = editor
            .schematic()
            .components
            .iter()
            .find(|component| component.id == selected_id)
        else {
            return Vec::new();
        };
        component
            .pins
            .iter()
            .filter_map(|pin| {
                let pad_number = if component.footprint_pin_map.is_empty() {
                    pin.name.as_str()
                } else {
                    component.footprint_pin_map.get(&pin.name)?.as_str()
                };
                footprint
                    .pads
                    .iter()
                    .find(|pad| pad.number.trim() == pad_number.trim())
                    .map(|pad| (pin.name.clone(), pad.number.clone()))
            })
            .collect()
    }

    fn append_surface(
        root: UiNode,
        rules: &mut Vec<UiStyleRule>,
        surface: UiSurface,
        rect: EditorRect,
    ) -> UiNode {
        let root_id = format!("electronics.workspace.qa.host.{}", surface.id);
        let wrapper = UiNode::new(root_id, UiNodeKind::Root)
            .with_layout(UiLayout {
                overflow: UiOverflow::Clip,
                ..UiLayout::absolute(UiRect::new(rect.x, rect.y, rect.width, rect.height))
            })
            .with_style(UiStyle::transparent())
            .with_child(surface.root);
        rules.extend(surface.style_sheet.rules);
        root.with_child(wrapper)
    }
}

fn compact_electronics_application_bar(surface: UiSurface, width: f32) -> UiSurface {
    crate::native_workbench::electronics_layout::compact_application_bar(surface, width)
}
