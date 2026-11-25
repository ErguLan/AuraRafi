
use super::*;
use raf_electronics::component::ElectronicComponent;

#[test]
fn camera_zoom_keeps_cursor_world_point_stable() {
    let camera = CadCamera {
        center: Vec2::new(100.0, 50.0),
        zoom: 2.0,
    };
    let size = Vec2::new(800.0, 600.0);
    let cursor = Vec2::new(140.0, 220.0);
    let world = camera.world_from_screen(cursor, size);
    let mut changed = camera;
    changed.zoom_at(2.0, cursor, size);
    assert!(changed.world_from_screen(cursor, size).distance(world) < 0.001);
}

#[test]
fn electronics_context_panels_keep_geometry_at_window_coordinates() {
    let mut editor = NativeElectronicsEditor::empty("Camera continuity");
    editor.camera_initialized = true;
    editor.camera = CadCamera {
        center: Vec2::new(60.0, 40.0),
        zoom: 2.0,
    };
    let old = EditorRect::new(260.0, 90.0, 1340.0, 750.0);
    let new = EditorRect::new(260.0, 90.0, 1040.0, 750.0);
    let point = Vec2::new(80.0, 100.0);
    let before = editor
        .camera
        .screen_from_world(point, Vec2::new(old.width, old.height))
        + Vec2::new(old.x, old.y);
    editor.preserve_camera_for_canvas_change(old, new);
    let after = editor
        .camera
        .screen_from_world(point, Vec2::new(new.width, new.height))
        + Vec2::new(new.x, new.y);
    assert!(before.distance(after) < 0.001);
}

#[test]
fn electronics_context_panel_does_not_cover_a_selection_at_the_right_edge() {
    let mut editor = NativeElectronicsEditor::empty("Context inspector");
    let mut component = ElectronicComponent::resistor("10k");
    component.position = Vec2::new(600.0, 0.0);
    editor.schematic.add_component(component);
    editor.rebuild_scene();
    editor.select_component(0);
    editor.camera_initialized = true;
    editor.camera = CadCamera {
        center: Vec2::ZERO,
        zoom: 1.0,
    };
    let old = EditorRect::new(260.0, 90.0, 1340.0, 750.0);
    let new = EditorRect::new(260.0, 90.0, 1040.0, 750.0);
    editor.preserve_camera_for_canvas_change(old, new);
    let local = editor
        .camera
        .screen_from_world(Vec2::new(600.0, 0.0), Vec2::new(new.width, new.height));
    assert!(local.x >= 40.0 && local.x <= new.width - 40.0);
    assert_eq!(
        editor.schematic.components[0].position,
        Vec2::new(600.0, 0.0)
    );
}

#[test]
fn empty_editor_starts_with_schematic_scene() {
    let editor = NativeElectronicsEditor::empty("Test");
    assert_eq!(editor.scene().surface, CadSurfaceKind::Schematic);
    assert!(editor.scene().objects.is_empty());
}

#[test]
fn electronics_fit_view_frames_small_boards_in_millimeters() {
    let mut editor = NativeElectronicsEditor::empty("Small physical PCB");
    editor.pcb.board_outline = raf_electronics::pcb::BoardOutline::default_rect(20.0, 12.0);
    editor.set_surface(CadSurfaceKind::Pcb);
    let canvas = EditorRect::new(52.0, 80.0, 928.0, 660.0);
    editor.render_options(
        canvas,
        raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
        1.0,
    );
    let displayed_width = 20.0 * editor.camera.zoom;
    assert!(displayed_width > canvas.width * 0.5 && displayed_width < canvas.width);
}

#[test]
fn simulation_values_are_visible_targeted_and_retained_as_stale() {
    let mut editor = NativeElectronicsEditor::empty("measurements");
    let mut source = ElectronicComponent::dc_source(5.0);
    source.position = Vec2::ZERO;
    let mut load = ElectronicComponent::resistor("1k");
    load.position = Vec2::new(200., 100.);
    editor.schematic.add_component(source);
    editor.schematic.add_component(load);
    for pin in 0..2 {
        let a = component_pin_world_position(
            &editor.schematic.components[0],
            &editor.schematic.components[0].pins[pin],
        );
        let b = component_pin_world_position(
            &editor.schematic.components[1],
            &editor.schematic.components[1].pins[pin],
        );
        editor
            .schematic
            .add_wire(a, b, if pin == 0 { "VCC" } else { "GND" });
    }
    editor.rebuild_scene();
    editor.run_simulation();
    assert!(editor.simulation_results().unwrap().converged);
    let row = editor
        .simulation_lines
        .iter()
        .position(|line| line.text.contains(" A | ") && line.text.contains("R1"))
        .unwrap();
    assert!(editor.focus_analysis_issue("simulation", row));
    assert_eq!(
        editor.selection.unwrap().source_id,
        editor.schematic.components[1].id
    );
    editor.schematic.components[1].value = "2k".into();
    editor.rebuild_scene();
    assert!(editor.simulation_is_stale());
    assert!(editor.simulation_results().is_some());
}

#[test]
fn pcb_drc_report_and_markers_belong_to_the_pcb() {
    let mut editor = NativeElectronicsEditor::empty("DRC");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.pcb.board_outline.points.clear();
    editor.rebuild_scene();
    editor.run_drc();
    assert!(editor
        .drc_report()
        .unwrap()
        .errors
        .iter()
        .any(|issue| issue.rule == "invalid_board_outline"));
    assert!(!editor.drc_is_stale());
    editor.set_surface(CadSurfaceKind::Schematic);
    assert!(editor.drc_is_stale());
    assert!(!editor
        .scene
        .objects
        .iter()
        .any(|object| object.kind == CadObjectKind::DrcMarker));
}

fn parsed_electronics_command(
    name: &str,
    args: &[(&str, &str)],
) -> crate::commands::parser::ParsedCommand {
    crate::commands::parser::ParsedCommand {
        raw: name.into(),
        name: name.into(),
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        positional: Vec::new(),
        structured_args: None,
    }
}

#[test]
fn catalog_drc_matches_the_active_pcb_report() {
    let mut editor = NativeElectronicsEditor::empty("command DRC");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.pcb.board_outline.points.clear();
    let output = editor.execute_catalog_command(
        "electronics.drc",
        &parsed_electronics_command("electronics.drc", &[]),
    );
    assert_eq!(output.json["passed"], false);
    assert_eq!(
        output.json["errors"].as_u64(),
        Some(editor.drc_report().unwrap().errors.len() as u64)
    );
}

#[test]
fn catalog_mutations_respect_locks_and_reject_nonfinite_geometry() {
    let mut editor = NativeElectronicsEditor::empty("locked");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("1k"));
    editor.schematic.components[0].locked = true;
    editor.select_component(0);
    editor.rebuild_scene();
    for name in [
        "electronics.rotate",
        "electronics.set_value",
        "electronics.delete",
    ] {
        let output = editor
            .execute_catalog_command(name, &parsed_electronics_command(name, &[("value", "2k")]));
        assert!(!output.changed);
        assert_eq!(output.json["ok"], false);
    }
    assert_eq!(editor.schematic.components.len(), 1);
    assert_eq!(editor.schematic.components[0].value, "1k");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.pcb.components[0].locked = true;
    editor.select_component(0);
    let position = editor.pcb.components[0].position;
    for name in ["pcb.move", "pcb.rotate"] {
        let output = editor
            .execute_catalog_command(name, &parsed_electronics_command(name, &[("dx", "10")]));
        assert_eq!(output.json["ok"], false);
        assert_eq!(editor.pcb.components[0].position, position);
    }
    editor.pcb.components[0].locked = false;
    let output = editor.execute_catalog_command(
        "pcb.move",
        &parsed_electronics_command("pcb.move", &[("x", "NaN")]),
    );
    assert_eq!(output.json["ok"], false);
    assert_eq!(editor.pcb.components[0].position, position);
    assert!(!editor.can_undo());
}

#[test]
fn preview_rebuild_reuses_geometry_without_growing_overlays() {
    let mut editor = NativeElectronicsEditor::empty("preview");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("1k"));
    editor.rebuild_scene();
    let base = editor.base_scene_object_count;
    editor.placement_template = Some(0);
    editor.placement_preview = Some(Vec2::new(100., 100.));
    editor.rebuild_scene();
    let count = editor.scene.objects.len();
    assert!(count > base);
    let fingerprint = editor.base_scene_fingerprint;
    for i in 0..10 {
        editor.placement_preview = Some(Vec2::new(i as f32 * 20., 100.));
        editor.rebuild_scene();
    }
    assert_eq!(editor.scene.objects.len(), count);
    assert_eq!(editor.base_scene_fingerprint, fingerprint);
    editor.placement_preview = None;
    editor.rebuild_scene();
    assert_eq!(editor.scene.objects.len(), base);
}

#[test]
fn camera_motion_does_not_invalidate_retained_ui() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    let ui_revision = editor.ui_revision();
    let render_revision = editor.revision();

    editor.zoom_in();

    assert_eq!(editor.ui_revision(), ui_revision);
    assert_ne!(editor.revision(), render_revision);
}

#[test]
fn analysis_commands_start_a_background_task_the_dock_can_cancel() {
    let mut editor = NativeElectronicsEditor::empty("Test");

    assert!(editor.apply_ui_command("electronics.analysis.drc"));
    // The command now uses the same background handle as the attached
    // executor, so the UI keeps painting and can offer Cancel.
    assert!(editor.analysis_running());

    assert!(editor.apply_ui_command("electronics.analysis.simulation"));
    assert!(editor.analysis_running());
}

#[test]
fn cancelled_analysis_keeps_the_previous_result_and_says_so() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor.run_drc();
    let report_lines = editor.drc_lines().len();
    assert!(editor.drc_report().is_some());

    editor.start_drc_analysis();
    assert!(editor.analysis_running());
    assert!(
        editor.drc_report().is_some(),
        "starting a new run must not hide the previous report"
    );

    editor.cancel_analysis();

    assert!(!editor.analysis_running());
    assert!(editor.drc_lines().len() > report_lines);
    assert!(editor.drc_lines().iter().any(|line| {
        line.kind == ElectronicsReportLineKind::Status
            && line.text == i18n::t("electronics.analysis.cancelled", Language::English)
    }));
}

#[test]
fn a_document_edit_marks_the_drc_report_stale_without_deleting_it() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor.run_drc();
    assert!(!editor.drc_is_stale());
    let lines_before = editor.drc_lines().len();

    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    editor.rebuild_scene();

    assert!(editor.drc_is_stale());
    assert!(editor.simulation_is_stale());
    assert!(
        editor.drc_report().is_some(),
        "the previous report must stay available for dimmed markers"
    );
    assert_eq!(editor.drc_lines().len(), lines_before);
}

#[test]
fn a_preview_only_rebuild_does_not_invalidate_the_report() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor.run_drc();
    assert!(!editor.drc_is_stale());

    editor.rebuild_scene();
    editor.set_tool(ElectronicsTool::Wire);

    assert!(
        !editor.drc_is_stale(),
        "rebuilding a preview is not a document edit"
    );
}

#[test]
fn running_drc_again_clears_the_stale_flag() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    editor.rebuild_scene();
    assert!(editor.drc_is_stale());

    editor.run_drc();

    assert!(!editor.drc_is_stale());
}

#[test]
fn net_names_never_reuse_a_live_name_after_a_wire_is_removed() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_wire(Vec2::ZERO, Vec2::new(20.0, 0.0), "N007");
    assert_eq!(editor.next_net_name(), "N008");

    editor.schematic.wires.clear();
    // The counter went down but the name is still referenced by the PCB
    // projection, so it must not be handed to a different circuit.
    editor.pcb.airwires.push(raf_electronics::PcbAirwire {
        net: "N007".to_string(),
        from_component_id: Uuid::new_v4(),
        from: Vec2::ZERO,
        to_component_id: Uuid::new_v4(),
        to: Vec2::new(20.0, 0.0),
    });

    assert_eq!(editor.next_net_name(), "N008");
}

#[test]
fn net_names_start_at_one_and_grow_past_the_padding() {
    let editor = NativeElectronicsEditor::empty("Test");
    assert_eq!(editor.next_net_name(), "N001");

    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_wire(Vec2::ZERO, Vec2::new(20.0, 0.0), "N999");
    assert_eq!(editor.next_net_name(), "N1000");
}

#[test]
fn pcb_rotation_is_independent_and_undoable() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    let component_id = editor.schematic.components[0].id;
    editor.set_surface(CadSurfaceKind::Pcb);
    assert!(editor.select_component(0));

    assert!(editor.rotate_selected());
    editor.rebuild_scene();

    assert_eq!(editor.rotation_degrees(component_id), 90.0);
    assert_eq!(editor.pcb.components[0].rotation, 90.0);
    assert_eq!(editor.schematic.components[0].rotation, 0.0);
    assert!(editor.undo());
    assert_eq!(editor.pcb.components[0].rotation, 0.0);
    assert_eq!(editor.schematic.components[0].rotation, 0.0);
    assert!(editor.redo());
    assert_eq!(editor.pcb.components[0].rotation, 90.0);
    editor.set_surface(CadSurfaceKind::Schematic);
    assert_eq!(editor.rotation_degrees(component_id), 0.0);
}

#[test]
fn delete_is_confirmation_gated() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    assert!(editor.select_component(0));

    editor.request_delete_selected();
    assert!(editor.delete_confirm_pending());
    assert_eq!(editor.schematic.components.len(), 1);

    editor.cancel_pending_action();
    assert!(!editor.delete_confirm_pending());
    assert_eq!(editor.schematic.components.len(), 1);

    editor.request_delete_selected();
    editor.confirm_delete();
    assert!(!editor.delete_confirm_pending());
    assert!(editor.schematic.components.is_empty());
}

#[test]
fn surface_errors_are_reported_and_can_be_cleared() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    assert!(editor.surface_errors().is_empty());

    editor.push_surface_error_key("electronics.error.drc_failed");
    editor.push_surface_error_key("electronics.error.drc_failed");
    assert_eq!(editor.surface_errors().len(), 1);

    editor.clear_surface_errors();
    assert!(editor.surface_errors().is_empty());
}

#[test]
fn surface_switch_preserves_component_selection_across_views() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    let component_id = editor.schematic.components[0].id;

    assert!(editor.select_component(0));
    editor.set_surface(CadSurfaceKind::Pcb);
    assert_eq!(
        editor.selection().map(|selection| selection.source_id),
        Some(component_id)
    );
    assert_eq!(
        editor.selection().map(|selection| selection.kind),
        Some(ElectronicsSelectionKind::Component)
    );
    assert_eq!(
        editor
            .interaction()
            .selected
            .as_ref()
            .map(|selected| selected.source_id),
        Some(Some(component_id))
    );

    editor.set_surface(CadSurfaceKind::Schematic);
    assert_eq!(
        editor.selection().map(|selection| selection.source_id),
        Some(component_id)
    );
    assert_eq!(
        editor.selection().map(|selection| selection.kind),
        Some(ElectronicsSelectionKind::Component)
    );
    assert_eq!(
        editor
            .interaction()
            .selected
            .as_ref()
            .map(|selected| selected.source_id),
        Some(Some(component_id))
    );
}

#[test]
fn library_selection_switches_to_placement_without_rebuilding_ui_per_frame() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    let ui_revision = editor.ui_revision();

    assert!(editor.apply_ui_command("electronics.place.template.0"));
    assert_eq!(editor.tool(), ElectronicsTool::Place);
    assert!(editor.ui_revision() > ui_revision);
}

#[test]
fn library_drag_only_commits_inside_the_canvas() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    let canvas = EditorRect::new(10.0, 20.0, 640.0, 480.0);

    assert!(editor.begin_library_drag(0));
    assert_eq!(
        editor.placement_preview_asset_key(),
        Some("electronics://symbols/resistor.png")
    );
    assert!(!editor.finish_library_drag_at(Some([2000.0, 2000.0]), canvas));
    assert!(editor.schematic.components.is_empty());
    assert_eq!(editor.placement_preview_asset_key(), None);

    assert!(editor.begin_library_drag(0));
    // Top-left of a large canvas is clear of the minimap panel, which
    // lives in the bottom-right corner and rejects placement.
    assert!(editor.finish_library_drag_at(Some([40.0, 50.0]), canvas));
    assert_eq!(editor.schematic.components.len(), 1);
    assert!(!editor.placement_drag_active);
}

#[path = "tests/pcb_and_inspector.rs"]
mod pcb_and_inspector;
