//! PCB synchronization, placement, routing and inspector commitments.

use super::*;

#[test]
fn entering_pcb_syncs_an_empty_layout_without_touching_the_undo_stack() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    assert!(editor.select_component(0));
    assert!(editor.apply_ui_command("electronics.inspector.value.commit:22k"));
    assert!(editor.can_undo());

    // Both flags are already set by the edits above, so the invariant is
    // that the tab switch does not change them.
    let undo_before_switch = editor.history.len();
    let dirty_before_switch = editor.is_dirty();
    editor.set_surface(CadSurfaceKind::Pcb);

    assert_eq!(editor.pcb.components.len(), 1);
    assert_eq!(editor.scene.surface, CadSurfaceKind::Pcb);
    // Entering the PCB tab is navigation, not an edit. The previous behavior
    // recorded a history entry and marked the project dirty, so the next
    // Ctrl+Z undid the derived synchronization instead of the user's work.
    assert_eq!(
        editor.history.len(),
        undo_before_switch,
        "the tab switch must not consume an undo slot"
    );
    assert_eq!(editor.is_dirty(), dirty_before_switch);
    assert!(!editor.sync_is_stale());

    // And from a clean slate the switch leaves nothing dirty at all.
    let mut fresh = NativeElectronicsEditor::empty("Test");
    fresh
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    fresh.set_surface(CadSurfaceKind::Pcb);
    assert!(!fresh.is_dirty());
    assert!(!fresh.can_undo());
}

#[test]
fn an_explicit_pcb_sync_is_an_edit_and_reports_a_clean_projection() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    editor.rebuild_scene();
    assert!(editor.sync_is_stale());

    assert!(editor.apply_ui_command("electronics.pcb.sync"));

    assert!(!editor.sync_is_stale());
    assert!(editor.can_undo());
    assert!(editor.is_dirty());
}

#[test]
fn deleting_in_the_pcb_reports_that_schematic_and_board_diverged() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    editor.set_surface(CadSurfaceKind::Pcb);
    assert!(editor.select_component(0));

    assert!(editor.delete_selected());

    // The PCB placement is removed but the schematic still owns the part, so
    // the surfaces disagree and the user is told instead of silently losing
    // the component when returning to the schematic.
    assert!(editor.sync_is_stale());
    assert_eq!(editor.schematic.components.len(), 1);
}

#[test]
fn pcb_library_placement_creates_a_linked_schematic_component() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor.set_surface(CadSurfaceKind::Pcb);
    editor.placement_template = Some(0);

    editor.place_component(Vec2::new(83.0, 117.0));

    assert_eq!(editor.schematic.components.len(), 1);
    assert_eq!(editor.pcb.components.len(), 1);
    assert_eq!(
        editor.pcb.components[0].component_id,
        editor.schematic.components[0].id
    );
    // The PCB default is 0.25 mm; these coordinates no longer round to
    // the schematic's former 20 mm spacing.
    assert_eq!(editor.grid_step(), 0.25);
    assert_eq!(editor.pcb.components[0].position, Vec2::new(83.0, 117.0));
}

#[test]
fn pcb_route_tool_turns_selected_airwire_into_a_trace() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("1k"));
    let mut second = ElectronicComponent::resistor("1k");
    second.position = Vec2::new(200., 100.);
    editor.schematic.add_component(second);
    let a = component_pin_world_position(
        &editor.schematic.components[0],
        &editor.schematic.components[0].pins[1],
    );
    let b = component_pin_world_position(
        &editor.schematic.components[1],
        &editor.schematic.components[1].pins[0],
    );
    editor.schematic.add_wire(a, b, "SIGNAL");
    editor.set_surface(CadSurfaceKind::Pcb);
    assert_eq!(editor.pcb.airwires.len(), 1);
    editor.rebuild_scene();
    editor.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
        object_id: "airwire:0".to_string(),
        source_id: None,
        kind: raf_electronics::CadObjectKind::Airwire,
        layer: CadLayerKind::Airwire,
    });

    assert!(editor.apply_ui_command("electronics.context.route"));
    assert_eq!(editor.pcb.traces.len(), 1);
    assert!(editor.pcb.airwires.is_empty());
    assert_eq!(
        editor.selection.map(|selection| selection.kind),
        Some(ElectronicsSelectionKind::Trace)
    );
}

#[test]
fn inspector_value_commit_updates_simulation_model_and_history() {
    let mut editor = NativeElectronicsEditor::empty("Test");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("10k"));
    assert!(editor.select_component(0));

    assert!(editor.apply_ui_command("electronics.inspector.value.commit:22k"));
    assert_eq!(editor.schematic.components[0].value, "22k");
    match &editor.schematic.components[0].sim_model {
        raf_electronics::component::SimModel::Resistor { ohms } => {
            assert!((*ohms - 22_000.0).abs() < f64::EPSILON)
        }
        other => panic!("expected resistor model, got {other:?}"),
    }
    assert!(editor.can_undo());
}

#[test]
fn targeted_inspector_commit_respects_locks_and_never_retargets_a_replacement() {
    use crate::electronics_action::ElectronicsAction;
    let mut editor = NativeElectronicsEditor::empty("stable edits");
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("1k"));
    let id = editor.schematic.components[0].id;
    let command = ElectronicsAction::CommitInspectorValueFor {
        source_id: id,
        value: "22k".into(),
    }
    .to_command_string();
    editor.schematic.components[0].locked = true;
    assert!(!editor.apply_ui_command(&command));
    assert_eq!(editor.schematic.components[0].value, "1k");
    editor.schematic.components.clear();
    editor
        .schematic
        .add_component(ElectronicComponent::resistor("2k"));
    assert!(!editor.apply_ui_command(&command));
    assert_eq!(editor.schematic.components[0].value, "2k");
    assert!(!editor.can_undo());
}
