
use super::*;
use raf_core::PointerButtonSet;

use super::canvas::wheel_zoom_factor;
use super::drag::grabbed_vertex_index;
use super::gestures::{chaining_tool, press_starts_pin_wire, wire_preview_follows_pointer};
use super::pointer::{hover_pick_required, pan_button_for_press};

/// Canvas used by the pointer tests.
///
/// The world points below are kept clear of the minimap panel, which owns the
/// bottom-right corner and swallows presses of its own.
fn test_canvas() -> EditorRect {
    EditorRect::new(0.0, 0.0, 640.0, 480.0)
}

/// Window position of a world point under a camera centred on the origin at
/// zoom 1 over `test_canvas`.
fn window_of(world: Vec2) -> [f32; 2] {
    [world.x + 320.0, world.y + 240.0]
}

fn buttons(list: &[PointerButton]) -> PointerButtonSet {
    let mut set = PointerButtonSet::default();
    for button in list {
        set.insert(*button);
    }
    set
}

fn frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
    InputSnapshot {
        pointer_position: Some(window_of(world)),
        time_seconds,
        window_focused: true,
        ..InputSnapshot::default()
    }
}

fn press_frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
    InputSnapshot {
        pointer_buttons_down: buttons(&[PointerButton::Primary]),
        pointer_pressed_buttons: buttons(&[PointerButton::Primary]),
        ..frame(world, time_seconds)
    }
}

fn hold_frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
    InputSnapshot {
        pointer_buttons_down: buttons(&[PointerButton::Primary]),
        ..frame(world, time_seconds)
    }
}

fn release_frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
    InputSnapshot {
        pointer_released_buttons: buttons(&[PointerButton::Primary]),
        ..frame(world, time_seconds)
    }
}

fn pin_position(editor: &NativeElectronicsEditor, component: usize, pin: usize) -> Vec2 {
    let placed = &editor.schematic.components[component];
    component_pin_world_position(placed, &placed.pins[pin])
}

/// Two resistors far enough apart that a drag between their pins can never be
/// mistaken for a click, and placed in the upper half of the canvas.
fn editor_with_two_resistors() -> NativeElectronicsEditor {
    let mut editor = NativeElectronicsEditor::empty("Pins");
    editor
        .schematic
        .add_component(raf_electronics::component::ElectronicComponent::resistor(
            "10k",
        ));
    editor
        .schematic
        .add_component(raf_electronics::component::ElectronicComponent::resistor(
            "4k7",
        ));
    for (index, component) in editor.schematic.components.iter_mut().enumerate() {
        component.position = if index == 0 {
            Vec2::new(-200.0, -100.0)
        } else {
            Vec2::new(80.0, -100.0)
        };
    }
    editor.rebuild_scene();
    editor.camera.center = Vec2::ZERO;
    editor.camera.zoom = 1.0;
    editor.camera_initialized = true;
    editor
}

/// A world point far from every pin in `editor_with_two_resistors`.
fn empty_world() -> Vec2 {
    Vec2::new(-40.0, 60.0)
}

#[test]
fn escape_abandons_outline_after_pointer_leaves_canvas_without_editing_board() {
    let mut editor = NativeElectronicsEditor::empty("Cancel outline");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.set_tool(ElectronicsTool::BoardOutline);
    editor.camera_initialized = true;
    editor.camera.zoom = 1.0;
    let before = editor.pcb.board_outline.clone();
    let mut router = InputRouter::default();
    for input in [
        press_frame(Vec2::new(-100.0, -80.0), 1.0),
        release_frame(Vec2::new(-100.0, -80.0), 1.05),
    ] {
        router.reconcile_input(&input);
        editor.process_input(&input, &mut router, test_canvas());
        router.finish_frame(&input);
    }
    assert!(editor.has_pending_gesture());
    let mut cancel = frame(Vec2::ZERO, 1.1);
    cancel.pointer_position = Some([700.0, 40.0]);
    cancel.keys_pressed.insert(InputKey::Escape);
    router.reconcile_input(&cancel);
    editor.process_input(&cancel, &mut router, test_canvas());
    router.finish_frame(&cancel);
    assert!(!editor.has_pending_gesture());
    assert!(editor.board_outline_vertices.is_empty());
    assert_eq!(editor.pcb.board_outline, before);
    assert!(!editor.is_dirty());
}

#[test]
fn electronics_save_refreshes_retained_status_and_failure_keeps_dirty() {
    let parent = std::env::temp_dir().join(format!("raf-save-status-{}", Uuid::new_v4()));
    let project = Project::create(
        "Status",
        raf_core::project::ProjectType::Electronics,
        &parent,
    )
    .unwrap();
    let mut editor = NativeElectronicsEditor::from_project(&project);
    editor.apply_ui_command("electronics.place.template.0");
    assert!(editor.place_component(Vec2::ZERO));
    editor.fabrication_status = Some("Previous fabrication failure".into());
    let revision = editor.ui_revision();
    editor.save(&project).unwrap();
    assert!(!editor.is_dirty());
    assert!(editor.ui_revision() > revision);
    assert!(
        editor.fabrication_status().is_some(),
        "Saving must preserve the independent fabrication notice"
    );
    assert_eq!(
        NativeElectronicsEditor::from_project(&project)
            .schematic
            .components
            .len(),
        1
    );
    assert!(editor.place_component(Vec2::new(100.0, 0.0)));
    let revision = editor.ui_revision();
    assert!(editor
        .save_to_session(&project, raf_core::session::SessionId(Uuid::new_v4()))
        .is_err());
    assert!(editor.is_dirty());
    assert_eq!(editor.ui_revision(), revision);
    assert!(parent.starts_with(std::env::temp_dir()));
    std::fs::remove_dir_all(parent).unwrap();
}

#[test]
fn fine_pcb_grid_keeps_outline_close_target_eight_pixels_wide() {
    let mut editor = NativeElectronicsEditor::empty("Outline target");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.set_tool(ElectronicsTool::BoardOutline);
    editor.grid_step = 0.25;
    editor.camera.zoom = 2.0;
    let mut router = InputRouter::default();
    let mut result = ElectronicsInputResult::default();
    for point in [
        Vec2::ZERO,
        Vec2::new(20.0, 0.0),
        Vec2::new(20.0, 20.0),
        Vec2::new(0.0, 20.0),
    ] {
        editor.handle_board_outline_click(
            point,
            &mut router,
            InputOwner::ElectronicsCanvas,
            &mut result,
        );
    }
    // Six screen pixels away from the first vertex. The old grid-capped
    // radius (half a screen pixel) added a fifth point instead of closing.
    editor.handle_board_outline_click(
        Vec2::new(3.0, 0.0),
        &mut router,
        InputOwner::ElectronicsCanvas,
        &mut result,
    );
    assert_eq!(editor.tool(), ElectronicsTool::Select);
    assert_eq!(editor.pcb.board_outline.points.len(), 5);
    assert_eq!(
        editor.pcb.board_outline.points.first(),
        editor.pcb.board_outline.points.last()
    );
}

#[test]
fn heart_outline_closes_as_polygon_and_undo_restores_previous_board() {
    let mut editor = NativeElectronicsEditor::empty("Heart regression");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.set_tool(ElectronicsTool::BoardOutline);
    editor.camera.zoom = 1.0;
    editor.camera_initialized = true;
    editor.snap_enabled = false;
    let before = editor.pcb.board_outline.clone();
    let points = [
        Vec2::new(0.0, -40.0),
        Vec2::new(40.0, -80.0),
        Vec2::new(80.0, -40.0),
        Vec2::new(80.0, 0.0),
        Vec2::new(0.0, 80.0),
        Vec2::new(-80.0, 0.0),
        Vec2::new(-80.0, -40.0),
        Vec2::new(-40.0, -80.0),
        Vec2::new(0.0, -40.0),
    ];
    let mut router = InputRouter::default();
    let mut result = ElectronicsInputResult::default();
    for point in points {
        editor.handle_board_outline_click(
            point,
            &mut router,
            InputOwner::ElectronicsCanvas,
            &mut result,
        );
    }
    assert_eq!(editor.pcb.board_outline.points, points);
    assert!(editor.is_dirty());
    assert!(editor.undo());
    assert_eq!(editor.pcb.board_outline, before);
    assert!(editor.redo());
    assert_eq!(editor.pcb.board_outline.points, points);
}

#[test]
fn library_click_then_single_canvas_press_places_and_repeats() {
    let mut editor = NativeElectronicsEditor::empty("Placement regression");
    let mut router = InputRouter::default();
    editor.camera_initialized = true;
    editor.camera.zoom = 1.0;
    for (i, at) in [Vec2::new(-200.0, -100.0), Vec2::new(-100.0, -100.0)]
        .into_iter()
        .enumerate()
    {
        assert!(editor.apply_ui_command("electronics.place.template.0"));
        editor.process_input(&press_frame(at, 1.0 + i as f64), &mut router, test_canvas());
        editor.process_input(
            &release_frame(at, 1.1 + i as f64),
            &mut router,
            test_canvas(),
        );
        assert_eq!(editor.schematic.components.len(), i + 1);
    }
}

#[test]
fn only_the_chaining_tools_can_be_left_with_a_double_click() {
    assert!(chaining_tool(
        ElectronicsTool::Wire,
        CadSurfaceKind::Schematic
    ));
    assert!(chaining_tool(
        ElectronicsTool::BoardOutline,
        CadSurfaceKind::Pcb
    ));
    // Route has no pending run to abandon, and neither tool means anything on
    // the surface it does not belong to.
    assert!(!chaining_tool(ElectronicsTool::Route, CadSurfaceKind::Pcb));
    assert!(!chaining_tool(ElectronicsTool::Wire, CadSurfaceKind::Pcb));
    assert!(!chaining_tool(
        ElectronicsTool::Select,
        CadSurfaceKind::Schematic
    ));
}

#[test]
fn the_armed_wire_preview_follows_the_pointer_in_select_too() {
    assert!(wire_preview_follows_pointer(ElectronicsTool::Wire, true));
    assert!(wire_preview_follows_pointer(ElectronicsTool::Select, true));
    assert!(!wire_preview_follows_pointer(
        ElectronicsTool::Select,
        false
    ));
    assert!(!wire_preview_follows_pointer(ElectronicsTool::Route, true));
}

#[test]
fn a_press_on_a_pin_starts_a_wire_only_in_schematic_select() {
    assert!(press_starts_pin_wire(
        ElectronicsTool::Select,
        CadSurfaceKind::Schematic,
        Some(CadObjectKind::Pin)
    ));
    // The Wire tool has its own press path, and a pin press there must not run
    // the contextual gesture on top of it.
    assert!(!press_starts_pin_wire(
        ElectronicsTool::Wire,
        CadSurfaceKind::Schematic,
        Some(CadObjectKind::Pin)
    ));
    // A pad is not a pin, and the board has no pins to route.
    assert!(!press_starts_pin_wire(
        ElectronicsTool::Select,
        CadSurfaceKind::Pcb,
        Some(CadObjectKind::Pad)
    ));
    assert!(!press_starts_pin_wire(
        ElectronicsTool::Select,
        CadSurfaceKind::Pcb,
        Some(CadObjectKind::Pin)
    ));
}

#[test]
fn a_press_on_anything_but_a_pin_keeps_selecting_and_dragging() {
    for picked in [
        Some(CadObjectKind::Component),
        Some(CadObjectKind::Wire),
        Some(CadObjectKind::Trace),
        Some(CadObjectKind::NetLabel),
        None,
    ] {
        assert!(
            !press_starts_pin_wire(ElectronicsTool::Select, CadSurfaceKind::Schematic, picked),
            "selecting or dragging {picked:?} must not be taken over by the wire gesture"
        );
    }
}

#[test]
fn native_pipeline_applies_final_drag_position_even_without_intermediate_frames() {
    let mut editor = editor_with_two_resistors();
    editor.snap_enabled = false;
    let from = editor.schematic.components[0].position;
    let to = from + Vec2::new(40.0, 40.0);
    let mut router = InputRouter::default();
    for input in [press_frame(from, 1.0), release_frame(to, 1.05)] {
        router.reconcile_input(&input);
        editor.process_input(&input, &mut router, test_canvas());
        router.finish_frame(&input);
    }
    assert_eq!(editor.schematic.components[0].position, to);
    assert!(!router.has_pointer_capture());
    assert!(editor.undo());
    assert_eq!(editor.schematic.components[0].position, from);
}

#[test]
fn dragging_from_a_pin_to_another_pin_wires_them_without_the_wire_tool() {
    let mut editor = editor_with_two_resistors();
    let mut router = InputRouter::default();
    let canvas = test_canvas();
    let from = pin_position(&editor, 0, 0);
    let to = pin_position(&editor, 1, 1);
    let before = editor.schematic.components[0].position;

    let press = press_frame(from, 1.0);
    router.reconcile_input(&press);
    editor.process_input(&press, &mut router, canvas);
    router.finish_frame(&press);

    assert!(
        editor.wire_start.is_some(),
        "a press on a pin has to arm the wire preview"
    );
    assert!(
        editor.component_drag.is_none(),
        "the gesture must not move the part it started from"
    );

    for input in [hold_frame(from.lerp(to, 0.5), 1.1), release_frame(to, 1.2)] {
        router.reconcile_input(&input);
        editor.process_input(&input, &mut router, canvas);
        router.finish_frame(&input);
    }

    assert_eq!(editor.schematic.wires.len(), 1);
    assert!(editor.can_undo(), "the wire is an undoable edit");
    assert!(editor.is_dirty());
    assert!(
        editor.wire_start.is_none(),
        "the contextual drag never chains to a second segment"
    );
    assert_eq!(editor.tool(), ElectronicsTool::Select);
    assert_eq!(
        editor.schematic.components[0].position, before,
        "neither part may move while a wire is drawn between them"
    );
}

#[test]
fn dropping_the_contextual_wire_on_empty_space_creates_nothing() {
    let mut editor = editor_with_two_resistors();
    let mut router = InputRouter::default();
    let canvas = test_canvas();
    let from = pin_position(&editor, 0, 0);

    editor.process_input(&press_frame(from, 1.0), &mut router, canvas);
    editor.process_input(
        &hold_frame(from.lerp(empty_world(), 0.5), 1.1),
        &mut router,
        canvas,
    );
    editor.process_input(&release_frame(empty_world(), 1.2), &mut router, canvas);

    assert!(editor.schematic.wires.is_empty());
    assert!(
        !editor.can_undo(),
        "a dropped wire is a dismissal, not an edit"
    );
    assert!(!editor.is_dirty());
    assert!(editor.wire_start.is_none());
}

#[test]
fn a_double_click_leaves_the_wire_tool_and_draws_nothing() {
    let mut editor = editor_with_two_resistors();
    let mut router = InputRouter::default();
    let canvas = test_canvas();
    editor.set_tool(ElectronicsTool::Wire);
    let at = empty_world();

    editor.process_input(&press_frame(at, 1.0), &mut router, canvas);
    editor.process_input(&release_frame(at, 1.05), &mut router, canvas);
    assert_eq!(editor.tool(), ElectronicsTool::Wire);

    editor.process_input(&press_frame(at, 1.15), &mut router, canvas);
    editor.process_input(&release_frame(at, 1.2), &mut router, canvas);

    assert_eq!(editor.tool(), ElectronicsTool::Select);
    assert!(editor.wire_start.is_none());
    assert!(
        editor.schematic.wires.is_empty(),
        "the second click of the pair must not confirm a zero-length segment"
    );
    assert!(!editor.is_dirty());
}

#[test]
fn a_double_click_abandons_a_half_drawn_board_outline() {
    let mut editor = NativeElectronicsEditor::empty("Outline");
    let mut router = InputRouter::default();
    let canvas = test_canvas();
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.set_tool(ElectronicsTool::BoardOutline);
    editor.camera.center = Vec2::ZERO;
    editor.camera.zoom = 1.0;
    editor.camera_initialized = true;
    let at = empty_world();

    editor.process_input(&press_frame(at, 1.0), &mut router, canvas);
    assert!(editor.board_outline_start.is_some());
    editor.process_input(&release_frame(at, 1.05), &mut router, canvas);

    editor.process_input(&press_frame(at, 1.15), &mut router, canvas);
    editor.process_input(&release_frame(at, 1.2), &mut router, canvas);

    assert_eq!(editor.tool(), ElectronicsTool::Select);
    assert!(editor.board_outline_start.is_none());
    // A new PCB already owns a default outline, so the invariant is that the
    // abandoned rectangle left it untouched rather than replacing it.
    assert_eq!(
        editor.pcb.board_outline.points,
        raf_electronics::pcb::layout::BoardOutline::default_rect(420.0, 280.0).points,
        "the abandoned rectangle must not reach the document"
    );
}

#[test]
fn clicking_two_pins_in_a_row_is_a_wire_and_stays_in_the_tool() {
    let mut editor = editor_with_two_resistors();
    let mut router = InputRouter::default();
    let canvas = test_canvas();
    editor.set_tool(ElectronicsTool::Wire);
    let from = pin_position(&editor, 0, 0);
    let to = pin_position(&editor, 1, 1);

    editor.process_input(&press_frame(from, 1.0), &mut router, canvas);
    editor.process_input(&release_frame(from, 1.05), &mut router, canvas);
    editor.process_input(&press_frame(to, 1.1), &mut router, canvas);
    editor.process_input(&release_frame(to, 1.15), &mut router, canvas);

    assert_eq!(editor.schematic.wires.len(), 1);
    assert_eq!(
        editor.tool(),
        ElectronicsTool::Wire,
        "two clicks on two pins are the route, not a double click"
    );
    assert!(
        editor.wire_start.is_some(),
        "and the run continues from the confirmed endpoint"
    );
}

#[test]
fn a_press_on_a_component_body_still_moves_the_component() {
    let mut editor = editor_with_two_resistors();
    let mut router = InputRouter::default();
    let canvas = test_canvas();
    let body = editor.schematic.components[0].position;
    let moved = body + Vec2::new(40.0, 0.0);

    editor.process_input(&press_frame(body, 1.0), &mut router, canvas);
    assert!(editor.component_drag.is_some());
    assert!(
        editor.wire_start.is_none(),
        "a body press must not arm the wire gesture"
    );
    editor.process_input(
        &InputSnapshot {
            pointer_delta: [
                window_of(moved)[0] - window_of(body)[0],
                window_of(moved)[1] - window_of(body)[1],
            ],
            ..hold_frame(moved, 1.1)
        },
        &mut router,
        canvas,
    );
    editor.process_input(&release_frame(moved, 1.2), &mut router, canvas);

    assert_eq!(editor.schematic.components[0].position, moved);
    assert!(editor.schematic.wires.is_empty());
    assert!(editor.component_drag.is_none());
}

#[test]
fn middle_drag_remains_pan_in_select_and_pan_modes() {
    assert_eq!(
        pan_button_for_press(ElectronicsTool::Select, false, false, true),
        Some(PointerButton::Middle)
    );
    assert_eq!(
        pan_button_for_press(ElectronicsTool::Pan, false, false, true),
        Some(PointerButton::Middle)
    );
}

#[test]
fn space_pan_uses_primary_without_stealing_middle_drag() {
    assert_eq!(
        pan_button_for_press(ElectronicsTool::Select, true, true, false),
        Some(PointerButton::Primary)
    );
    assert_eq!(
        pan_button_for_press(ElectronicsTool::Select, true, false, true),
        Some(PointerButton::Middle)
    );
}

#[test]
fn one_wheel_notch_is_a_reproducible_zoom_step() {
    // The platform adapter reports a whole mouse notch as 24.0 units, so
    // after normalization a notch is the same step everywhere instead of the
    // saturated 1.8 the raw 24.0 produced.
    assert!((wheel_zoom_factor(SCROLL_UNITS_PER_NOTCH) - ZOOM_STEP_PER_NOTCH).abs() < 1e-5);
    assert!((wheel_zoom_factor(-SCROLL_UNITS_PER_NOTCH) - 1.0 / ZOOM_STEP_PER_NOTCH).abs() < 1e-5);
}

#[test]
fn a_trackpad_reports_a_fraction_of_a_notch() {
    let fraction = wheel_zoom_factor(SCROLL_UNITS_PER_NOTCH / 4.0);
    assert!(fraction > 1.0 && fraction < ZOOM_STEP_PER_NOTCH);
}

#[test]
fn a_fast_wheel_flick_cannot_jump_across_the_zoom_range() {
    let factor = wheel_zoom_factor(SCROLL_UNITS_PER_NOTCH * 20.0);
    assert!(factor <= ZOOM_STEP_MAX);
    assert!(
        factor < 1.8,
        "one flick must stay far below the old saturation"
    );
}

#[test]
fn hover_pick_is_skipped_until_the_cursor_travels_a_pixel() {
    let world = Vec2::new(12.0, 8.0);
    // No previous sample: re-entering the canvas always repicks.
    assert!(hover_pick_required(None, world, 1.0));
    assert!(!hover_pick_required(Some(world), world, 1.0));
    assert!(!hover_pick_required(
        Some(Vec2::new(12.0, 8.0)),
        Vec2::new(12.0, 8.4),
        1.0
    ));
    assert!(hover_pick_required(
        Some(Vec2::new(12.0, 8.0)),
        Vec2::new(12.0, 9.5),
        1.0
    ));
}

#[test]
fn the_vertex_grab_is_frozen_by_the_press_and_not_by_the_drag() {
    let points = [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 80.0),
        Vec2::new(80.0, 80.0),
    ];
    let press = Vec2::new(-2.0, 76.0);
    let offset = points[1] - press;
    assert_eq!(grabbed_vertex_index(&points, offset, press), Some(1));

    // Resolving against the live pointer would hand the gesture to the
    // neighbouring vertex as soon as the drag travelled far enough, and the
    // run would start following the wrong point.
    let dragged = press + Vec2::new(120.0, 0.0);
    assert_ne!(grabbed_vertex_index(&points, Vec2::ZERO, dragged), Some(1));
    // The original press keeps resolving to the same vertex.
    assert_eq!(grabbed_vertex_index(&points, offset, press), Some(1));
}

#[test]
fn a_pin_inside_the_grid_ceiling_snaps_and_one_outside_it_does_not() {
    let mut editor = NativeElectronicsEditor::empty("Snap");
    editor.snap_enabled = true;
    editor.grid_step = 20.0;
    editor.camera.zoom = 0.2;
    editor
        .schematic
        .add_component(raf_electronics::component::ElectronicComponent::resistor(
            "10k",
        ));
    let pin = component_pin_world_position(
        &editor.schematic.components[0],
        &editor.schematic.components[0].pins[0],
    );
    // At this zoom the pure screen radius spans many grid cells, so only the
    // grid ceiling keeps a distant route from jumping onto the pin.
    assert!(editor.pin_endpoint_at(pin + Vec2::new(10.0, 0.0)).is_some());
    assert!(editor.pin_endpoint_at(pin + Vec2::new(18.0, 0.0)).is_none());
}

#[test]
fn disabling_grid_snap_keeps_electrical_endpoints_connectable() {
    let mut editor = NativeElectronicsEditor::empty("Snap off");
    editor.snap_enabled = false;
    editor
        .schematic
        .add_component(raf_electronics::component::ElectronicComponent::resistor(
            "10k",
        ));
    let pin = component_pin_world_position(
        &editor.schematic.components[0],
        &editor.schematic.components[0].pins[0],
    );
    assert!(editor.pin_endpoint_at(pin).is_some());
}
