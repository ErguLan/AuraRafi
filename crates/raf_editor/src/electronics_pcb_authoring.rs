//! Physical PCB tool parameters and click-by-click routing gestures.
use super::*;
use raf_electronics::PcbLayer;

#[derive(Debug, Clone)]
pub(super) struct PcbRouteDraft {
    net: String,
    points: Vec<Vec2>,
    layer: PcbLayer,
    width: f32,
    document_revision: u64,
    source: String,
    error: Option<String>,
    legs: Vec<raf_electronics::pcb::PcbTrace>,
    vias: Vec<raf_electronics::pcb::PcbVia>,
    via_pending: bool,
}

#[derive(Clone)]
struct RoutePad {
    point: Vec2,
    net: String,
    net_id: Option<usize>,
    label: String,
    available: bool,
}

fn nearest_on_segment(point: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let delta = b - a;
    let length = delta.length_squared();
    if length <= f32::EPSILON {
        a
    } else {
        a + delta * ((point - a).dot(delta) / length).clamp(0.0, 1.0)
    }
}

fn split_trace_at(layout: &mut PcbLayout, net: &str, layer: PcbLayer, point: Vec2) {
    for trace in layout
        .traces
        .iter_mut()
        .filter(|trace| trace.net == net && trace.layer == layer)
    {
        if trace.points.iter().any(|p| p.distance(point) <= 0.0001) {
            continue;
        }
        if let Some(index) = trace
            .points
            .windows(2)
            .position(|p| nearest_on_segment(point, p[0], p[1]).distance(point) <= 0.0001)
        {
            trace.points.insert(index + 1, point);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_electronics::component::ElectronicComponent;
    use raf_electronics::pcb::{parse_kicad_footprint, BoardOutline};

    fn routing_editor() -> NativeElectronicsEditor {
        let asset = parse_kicad_footprint(
            r#"(footprint "R_route"
          (pad "1" smd rect (at -1.2 0) (size 1 1.4) (layers "F.Cu" "F.Mask"))
          (pad "2" smd rect (at 1.2 0) (size 1 1.4) (layers "F.Cu" "F.Mask")))"#,
        )
        .unwrap();
        let mut editor = NativeElectronicsEditor::empty("route");
        for x in [0.0, 200.0] {
            let mut c = ElectronicComponent::resistor("1k");
            c.position = Vec2::new(x, 0.0);
            c.footprint = asset.id.clone();
            editor.schematic.add_component(c);
        }
        let from = component_pin_world_position(
            &editor.schematic.components[0],
            &editor.schematic.components[0].pins[1],
        );
        let to = component_pin_world_position(
            &editor.schematic.components[1],
            &editor.schematic.components[1].pins[0],
        );
        editor.schematic.add_wire(from, to, "LINK");
        editor.pcb.footprint_assets.insert(asset.id.clone(), asset);
        editor.pcb.sync_from_schematic(&editor.schematic);
        editor.pcb.components[0].position = Vec2::new(10.0, 10.0);
        editor.pcb.components[1].position = Vec2::new(30.0, 10.0);
        editor.pcb.board_outline = BoardOutline::default_rect(40.0, 20.0);
        editor.pcb.rebuild_airwires();
        editor.set_surface(CadSurfaceKind::Pcb);
        editor.camera.zoom = 40.0;
        editor.grid_step = 0.25;
        editor.set_tool(ElectronicsTool::Route);
        editor.rebuild_scene();
        editor
    }

    #[test]
    fn electronics_two_face_route_is_atomic_and_undo_restores_vias_and_copper() {
        let mut editor = routing_editor();
        let start = editor.pcb.pad_world_position(0, 1).unwrap();
        let end = editor.pcb.pad_world_position(1, 0).unwrap();
        let before = pcb_fingerprint(&editor.pcb);
        let history = editor.history.len();
        assert!(editor.route_physical_click(start));
        assert!(editor.apply_pcb_tool_command("electronics.pcb.via"));
        assert!(editor.route_physical_click(Vec2::new(16.0, 10.0)));
        assert_eq!(editor.pcb_route_layer, PcbLayer::BottomCopper);
        assert!(editor.apply_pcb_tool_command("electronics.pcb.via"));
        assert!(editor.route_physical_click(Vec2::new(24.0, 10.0)));
        assert_eq!(editor.pcb_route_layer, PcbLayer::TopCopper);
        assert_eq!(
            before,
            pcb_fingerprint(&editor.pcb),
            "draft must not mutate copper"
        );
        assert!(editor.route_physical_click(end));
        assert!(
            editor.pcb_route_draft.is_none(),
            "{:?}",
            editor.surface_errors
        );
        assert_eq!(editor.pcb.vias.len(), 2);
        assert_eq!(editor.pcb.traces.len(), 3);
        assert!(editor.pcb.airwires.is_empty());
        assert_eq!(editor.history.len(), history + 1);
        let completed = pcb_fingerprint(&editor.pcb);
        assert!(editor.undo());
        assert_eq!(pcb_fingerprint(&editor.pcb), before);
        assert!(editor.redo());
        assert_eq!(pcb_fingerprint(&editor.pcb), completed);
    }

    #[test]
    fn electronics_route_can_branch_and_land_on_existing_copper() {
        let mut editor = routing_editor();
        let start = editor.pcb.pad_world_position(0, 1).unwrap();
        let end = editor.pcb.pad_world_position(1, 0).unwrap();
        let net = editor.pcb.components[0].pad_nets[1].clone();
        editor
            .pcb
            .try_add_trace(&net, PcbLayer::TopCopper, 0.25, vec![start, end])
            .unwrap();
        editor.rebuild_scene();
        assert!(editor.route_physical_click(Vec2::new(16.0, 10.0)));
        assert!(editor.route_physical_click(Vec2::new(20.0, 5.0)));
        assert!(editor.route_physical_click(Vec2::new(24.0, 10.0)));
        assert_eq!(editor.pcb.traces.len(), 2);
        assert!(editor.pcb.traces[0].points.contains(&Vec2::new(16.0, 10.0)));
        assert!(editor.pcb.traces[0].points.contains(&Vec2::new(24.0, 10.0)));
    }

    #[test]
    fn electronics_partial_route_keeps_via_and_remains_explicitly_unrouted() {
        let mut editor = routing_editor();
        let start = editor.pcb.pad_world_position(0, 1).unwrap();
        assert!(editor.route_physical_click(start));
        assert!(editor.apply_pcb_tool_command("electronics.pcb.via"));
        assert!(editor.route_physical_click(Vec2::new(16.0, 10.0)));
        assert!(editor.apply_pcb_tool_command("electronics.pcb.route.finish"));
        assert_eq!(editor.pcb.vias.len(), 1);
        assert_eq!(editor.pcb.traces.len(), 1);
        assert_eq!(editor.pcb.airwires.len(), 1);
        assert!(editor
            .pcb
            .run_drc()
            .warnings
            .iter()
            .any(|issue| issue.rule == "unrouted_net"));
        assert!(editor.undo());
        assert!(editor.pcb.vias.is_empty());
        assert!(editor.pcb.traces.is_empty());
    }

    #[test]
    fn electronics_rules_are_undoable_and_invalid_geometry_is_rejected() {
        let mut editor = routing_editor();
        let before = pcb_fingerprint(&editor.pcb);
        assert!(editor.apply_pcb_tool_command("electronics.pcb.rule:clearance::0.35"));
        assert_eq!(editor.pcb.rules.default.clearance, 0.35);
        assert!(editor.undo());
        assert_eq!(pcb_fingerprint(&editor.pcb), before);
        assert!(editor.apply_pcb_tool_command("electronics.pcb.rule:via-drill::0.65"));
        assert_eq!(
            pcb_fingerprint(&editor.pcb),
            before,
            "invalid annular ring must not change rules"
        );
    }

    #[test]
    fn electronics_native_gateway_preserves_case_sensitive_net_payloads() {
        let mut editor = routing_editor();
        let mut ledger = raf_core::TransactionLedger::default();
        let response = crate::native_attached_executor::execute_native_ui_intent(
            &mut editor,
            &mut ledger,
            "electronics.pcb.rule:clearance:LINK:0.45",
        );
        assert!(response.ok, "{:?}", response.lines);
        assert_eq!(editor.pcb.rules.nets["LINK"].clearance, 0.45);
        assert!(!editor.pcb.rules.nets.contains_key("link"));
        let response = crate::native_attached_executor::execute_native_ui_intent(
            &mut editor,
            &mut ledger,
            "electronics.net.highlight:LINK",
        );
        assert!(response.ok, "{:?}", response.lines);
        assert!(editor.highlight_net.is_some());
        for wire in &mut editor.schematic.wires {
            wire.net = "Power Rail".into();
        }
        editor.pcb.sync_from_schematic(&editor.schematic);
        editor.rebuild_scene();
        let response = crate::native_attached_executor::execute_native_ui_intent(
            &mut editor,
            &mut ledger,
            "electronics.pcb.rule:clearance:Power Rail:0.4",
        );
        assert!(response.ok, "{:?}", response.lines);
        assert_eq!(editor.pcb.rules.nets["Power Rail"].clearance, 0.4);
    }

    #[test]
    fn enabling_one_legend_after_hiding_all_keeps_the_other_hidden() {
        let mut editor = NativeElectronicsEditor::empty("Legend");
        editor.toggle_labels();
        assert_eq!(editor.pcb_label_visibility(), (false, false));
        assert!(editor.apply_pcb_tool_command("electronics.pcb.labels:reference:true"));
        assert_eq!(editor.pcb_label_visibility(), (true, false));
        assert!(editor.apply_pcb_tool_command("electronics.pcb.labels:value:true"));
        assert_eq!(editor.pcb_label_visibility(), (true, true));
        assert!(editor.apply_pcb_tool_command("electronics.pcb.labels:reference:false"));
        assert_eq!(editor.pcb_label_visibility(), (false, true));
        assert!(!editor.is_dirty());
    }

    #[test]
    fn fit_keeps_physical_board_outside_the_expanded_overview() {
        let mut editor = NativeElectronicsEditor::empty("Clear drawing area");
        editor.pcb.board_outline = raf_electronics::pcb::BoardOutline::default_rect(80.0, 50.0);
        editor.set_surface(CadSurfaceKind::Pcb);
        let canvas = crate::editor_layout::EditorRect::new(0.0, 0.0, 900.0, 600.0);
        editor.render_options(
            canvas,
            raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
            1.0,
        );
        let size = Vec2::new(canvas.width, canvas.height);
        let board_right = editor
            .camera()
            .screen_from_world(Vec2::new(80.0, 50.0), size)
            .x;
        assert!(board_right < editor.minimap_rects(size).unwrap().panel.x);
        let camera = editor.camera();
        assert!(editor.apply_pcb_tool_command("electronics.minimap.toggle"));
        assert_eq!(editor.camera().center, camera.center);
        assert_eq!(editor.camera().zoom, camera.zoom);
    }

    #[test]
    fn component_lock_is_scoped_undoable_and_zoom_remains_presentation_only() {
        let mut editor = routing_editor();
        let id = editor.pcb.components[0].component_id;
        assert!(
            !editor.apply_ui_command(&format!("electronics.component.lock:schematic:{id}:true"))
        );
        let history = editor.history.len();
        assert!(editor.apply_ui_command(&format!("electronics.component.lock:pcb:{id}:true")));
        assert!(editor.pcb.components[0].locked);
        assert_eq!(editor.history.len(), history + 1);
        assert!(!editor.apply_ui_command(&format!("electronics.transform:pcb:{id}:x:9")));
        assert!(editor.undo());
        assert!(!editor.pcb.components[0].locked);
        let before = (
            schematic_fingerprint(&editor.schematic),
            pcb_fingerprint(&editor.pcb),
            editor.history.len(),
        );
        assert!(editor.apply_ui_command("electronics.pcb.zoom:400"));
        assert!((editor.camera.zoom - 4.0 * 96.0 / 25.4).abs() < 0.001);
        assert!(!editor.apply_ui_command("electronics.pcb.zoom:NaN"));
        assert_eq!(
            before,
            (
                schematic_fingerprint(&editor.schematic),
                pcb_fingerprint(&editor.pcb),
                editor.history.len()
            )
        );
    }

    #[test]
    fn inspector_numeric_edits_keep_the_original_target_and_one_undo_step() {
        let mut editor = routing_editor();
        let original = editor.pcb.components[0].clone();
        let other = editor.pcb.components[1].clone();
        let id = original.component_id;
        editor.select_component(1);
        let history = editor.history.len();
        let command = format!("electronics.transform:pcb:{id}:x:16.125");
        assert!(editor.apply_ui_command(&command));
        assert_eq!(editor.pcb.components[0].position.x, 16.125);
        assert_eq!(editor.pcb.components[1], other);
        assert_eq!(editor.history.len(), history + 1);
        assert!(editor.apply_ui_command(&command));
        assert_eq!(
            editor.history.len(),
            history + 1,
            "Unchanged values are not edits"
        );
        assert!(editor.undo());
        assert_eq!(editor.pcb.components[0], original);
        assert!(editor.redo());
        assert_eq!(editor.pcb.components[0].position.x, 16.125);

        let snapshot = (
            schematic_fingerprint(&editor.schematic),
            pcb_fingerprint(&editor.pcb),
        );
        for payload in [
            format!("pcb:{id}:x:NaN"),
            format!("pcb:{id}:y:inf"),
            format!("pcb:{id}:rotation:1000001"),
            format!("schematic:{id}:x:3"),
            format!("pcb:{}:x:3", Uuid::new_v4()),
        ] {
            assert!(!editor.apply_ui_command(&format!("electronics.transform:{payload}")));
            assert_eq!(
                (
                    schematic_fingerprint(&editor.schematic),
                    pcb_fingerprint(&editor.pcb)
                ),
                snapshot
            );
        }
        editor.pcb.components[0].locked = true;
        assert!(!editor.apply_ui_command(&format!("electronics.transform:pcb:{id}:x:5")));
        assert_eq!(editor.pcb.components[0].position.x, 16.125);
    }

    #[test]
    fn schematic_numeric_position_moves_wire_anchors_and_preserves_the_board() {
        let mut editor = routing_editor();
        editor.set_surface(CadSurfaceKind::Schematic);
        let id = editor.schematic.components[0].id;
        editor.schematic.wires[0].start_anchor =
            Some(raf_electronics::schematic::WireAnchor::Pin {
                component_id: id,
                pin_id: editor.schematic.components[0].pins[1].id,
            });
        let wire = editor.schematic.wires[0].clone();
        let board = pcb_fingerprint(&editor.pcb);
        assert!(editor.apply_ui_command(&format!("electronics.transform:schematic:{id}:y:25")));
        assert_eq!(
            editor.schematic.wires[0].start,
            wire.start + Vec2::new(0.0, 25.0)
        );
        assert_eq!(pcb_fingerprint(&editor.pcb), board);
        assert!(editor.undo());
        assert_eq!(editor.schematic.wires[0].start, wire.start);
        assert_eq!(editor.schematic.wires[0].end, wire.end);
        assert_eq!(editor.schematic.wires[0].net, wire.net);
    }

    #[test]
    fn beta_preview_is_presentation_only() {
        let mut editor = routing_editor();
        let before = (
            schematic_fingerprint(&editor.schematic),
            pcb_fingerprint(&editor.pcb),
        );
        let history = editor.history.len();
        let dirty = editor.is_dirty();
        assert!(editor.apply_ui_command("electronics.pcb.preview.toggle"));
        assert!(editor.pcb_preview_open());
        assert_eq!(
            (
                schematic_fingerprint(&editor.schematic),
                pcb_fingerprint(&editor.pcb)
            ),
            before
        );
        assert_eq!(editor.history.len(), history);
        assert_eq!(editor.is_dirty(), dirty);
        assert!(editor.apply_ui_command("electronics.pcb.preview.toggle"));
        assert!(!editor.pcb_preview_open());
    }

    #[test]
    fn discarding_parked_traces_is_undoable_and_leaves_live_routes_alone() {
        let mut editor = routing_editor();
        let trace = raf_electronics::pcb::PcbTrace {
            id: Uuid::new_v4(),
            net: "old-net".into(),
            layer: PcbLayer::TopCopper,
            width: 0.25,
            points: vec![Vec2::new(4.0, 4.0), Vec2::new(5.0, 4.0)],
        };
        editor.pcb.removed_traces.push(trace.clone());
        let live = editor.pcb.traces.clone();
        let history = editor.history.len();
        assert!(editor.apply_pcb_tool_command("electronics.pcb.discard_removed_traces"));
        assert!(editor.pcb.removed_traces.is_empty());
        assert_eq!(editor.pcb.traces, live);
        assert_eq!(editor.history.len(), history + 1);
        assert!(editor.apply_pcb_tool_command("electronics.pcb.discard_removed_traces"));
        assert_eq!(
            editor.history.len(),
            history + 1,
            "Empty discard is a no-op"
        );
        assert!(editor.undo());
        assert_eq!(editor.pcb.removed_traces, vec![trace]);
        assert!(editor.redo());
        assert!(editor.pcb.removed_traces.is_empty());
    }

    #[test]
    fn manual_route_commits_once_and_undo_restores_ratsnest() {
        let mut editor = routing_editor();
        let start = editor.pcb.pad_world_position(0, 1).unwrap();
        let end = editor.pcb.pad_world_position(1, 0).unwrap();
        assert!(!editor.pcb.airwires.is_empty());
        let history = editor.history.len();
        assert!(editor.route_physical_click(start));
        assert!(editor.route_physical_click(Vec2::new(20.0, 5.0)));
        assert!(editor.pcb.traces.is_empty());
        assert!(editor
            .scene
            .objects
            .iter()
            .any(|o| o.id == "pcb-route-preview" && o.stroke_width_world == Some(0.25)));
        assert!(editor.route_physical_click(end));
        assert_eq!(editor.pcb.traces.len(), 1, "{:?}", editor.surface_errors);
        assert_eq!(editor.history.len(), history + 1);
        assert!(editor.pcb.airwires.is_empty());
        assert!(editor.pcb_route_draft.is_none());
        assert!(editor.undo());
        assert!(editor.pcb.traces.is_empty());
        assert!(!editor.pcb.airwires.is_empty());
        assert!(editor.redo());
        assert_eq!(editor.pcb.traces.len(), 1);
    }

    #[test]
    fn manual_route_rejects_wrong_net_and_undo_cancels_without_document_history() {
        let mut editor = routing_editor();
        let start = editor.pcb.pad_world_position(0, 1).unwrap();
        let wrong = editor.pcb.pad_world_position(1, 1).unwrap();
        let before = pcb_fingerprint(&editor.pcb);
        let history = editor.history.len();
        assert!(editor.route_physical_click(start));
        editor.push_surface_error("Unrelated document error".into());
        assert!(editor.route_physical_click(wrong));
        assert!(editor.route_error.is_some());
        assert_eq!(pcb_fingerprint(&editor.pcb), before);
        assert!(editor.undo());
        assert!(editor.pcb_route_draft.is_none());
        assert!(editor.route_error.is_none());
        assert_eq!(editor.surface_errors(), &["Unrelated document error".to_string()]);
        assert_eq!(editor.history.len(), history);
        assert_eq!(pcb_fingerprint(&editor.pcb), before);
    }

    #[test]
    fn routing_can_start_in_the_drill_center_on_either_copper_face() {
        let mut editor = routing_editor();
        let asset = parse_kicad_footprint(r#"(footprint "PTH_route"
            (pad "1" thru_hole rect (at -1.2 0) (size 1.4 1.4) (drill 0.6) (layers "*.Cu" "*.Mask"))
            (pad "2" thru_hole circle (at 1.2 0) (size 1.4 1.4) (drill 0.6) (layers "*.Cu" "*.Mask")))"#).unwrap();
        for component in &mut editor.pcb.components {
            component.footprint = asset.id.clone();
        }
        editor.pcb.footprint_assets.insert(asset.id.clone(), asset);
        editor.camera.zoom = 160.0;
        editor.apply_pcb_tool_command("electronics.pcb.layer:bottom");
        let start = editor.pcb.pad_world_position(0, 1).unwrap();
        let end = editor.pcb.pad_world_position(1, 0).unwrap();
        assert!(editor.route_physical_click(start));
        assert!(
            editor.pcb_route_draft.is_some(),
            "A plated drill center is a valid route origin"
        );
        assert!(editor.route_physical_click(end));
        assert!(
            editor.pcb_route_draft.is_none(),
            "{:?}",
            editor.surface_errors
        );
        assert_eq!(editor.pcb.traces[0].layer, PcbLayer::BottomCopper);
        assert!(editor.pcb.airwires.is_empty());
    }

    #[test]
    fn surface_mount_pad_on_the_other_face_explains_why_it_cannot_start() {
        let mut editor = routing_editor();
        editor.apply_pcb_tool_command("electronics.pcb.layer:bottom");
        let point = editor.pcb.pad_world_position(0, 1).unwrap();
        editor.pointer_world = Some(point);
        assert!(editor.pcb_routing_hint().unwrap().2);
        assert!(editor.route_physical_click(point));
        assert!(editor.pcb_route_draft.is_none());
        assert!(editor.pcb.traces.is_empty());
    }

    #[test]
    fn adjacent_pads_choose_the_nearest_center_at_board_scale() {
        let mut editor = routing_editor();
        editor.camera.zoom = 1.0;
        editor.pcb.components[0].pad_nets[0] = "OTHER".into();
        editor.rebuild_scene();
        let point = editor.pcb.pad_world_position(0, 1).unwrap();
        let target = editor.route_pad_at(point).unwrap();
        assert!(target.label.ends_with("/ 2"));
        assert_eq!(target.point, point);
    }

    #[test]
    fn clicking_an_airwire_arms_a_route_without_authoring_automatic_copper() {
        let mut editor = routing_editor();
        editor.camera.center = Vec2::new(20.0, 10.0);
        editor.camera.zoom = 20.0;
        editor.camera_initialized = true;
        let wire = &editor.pcb.airwires[0];
        let at = editor
            .camera
            .screen_from_world((wire.from + wire.to) * 0.5, Vec2::new(1000.0, 600.0));
        let mut down = raf_core::PointerButtonSet::default();
        down.insert(PointerButton::Primary);
        let input = InputSnapshot {
            pointer_position: Some(at.to_array()),
            pointer_pressed_buttons: down,
            pointer_buttons_down: down,
            window_focused: true,
            ..Default::default()
        };
        let history = editor.history.len();
        let mut router = InputRouter::default();
        router.reconcile_input(&input);
        editor.process_input(
            &input,
            &mut router,
            EditorRect::new(0.0, 0.0, 1000.0, 600.0),
        );
        assert!(editor.pcb.traces.is_empty());
        assert_eq!(editor.history.len(), history);
        assert!(editor.pcb_route_draft.is_some());
    }
}

fn extend_route(points: &mut Vec<Vec2>, end: Vec2) {
    let Some(start) = points.last().copied() else {
        points.push(end);
        return;
    };
    let delta = end - start;
    let diagonal = delta.abs().min_element();
    let corner = start + delta.signum() * diagonal;
    if corner.distance(start) > 0.0001 && corner.distance(end) > 0.0001 {
        points.push(corner);
    }
    if end.distance(*points.last().unwrap()) > 0.0001 {
        points.push(end);
    }
}

impl NativeElectronicsEditor {
    /// Projection used by inspector inputs; values retain native document units.
    pub fn inspector_transform(&self) -> Option<(String, Vec2, f32, bool)> {
        let selection = self.selection?;
        if !matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            return None;
        }
        if self.active_surface == CadSurfaceKind::Pcb {
            let c = self
                .pcb
                .components
                .iter()
                .find(|c| c.component_id == selection.source_id)?;
            Some((
                format!("pcb:{}", c.component_id),
                c.position,
                c.rotation,
                c.locked,
            ))
        } else {
            let c = self
                .schematic
                .components
                .iter()
                .find(|c| c.id == selection.source_id)?;
            Some((
                format!("schematic:{}", c.id),
                c.position,
                c.rotation,
                c.locked,
            ))
        }
    }

    fn apply_inspector_transform(&mut self, payload: &str) -> bool {
        let parts: Vec<_> = payload.split(':').collect();
        let [surface, id, axis, raw] = parts.as_slice() else {
            return false;
        };
        let Ok(id) = Uuid::parse_str(id) else {
            return false;
        };
        let Ok(value) = raw.parse::<f32>() else {
            return false;
        };
        if !value.is_finite()
            || value.abs() > 1_000_000.0
            || !matches!(*axis, "x" | "y" | "rotation")
        {
            return false;
        }
        // Never retarget a delayed field edit to a different component/surface.
        let pcb = match *surface {
            "pcb" => true,
            "schematic" => false,
            _ => return false,
        };
        if pcb != (self.active_surface == CadSurfaceKind::Pcb) {
            return false;
        }
        let before = self.snapshot();
        let (position, rotation, locked) = if pcb {
            let Some(c) = self
                .pcb
                .components
                .iter_mut()
                .find(|c| c.component_id == id)
            else {
                return false;
            };
            (&mut c.position, &mut c.rotation, c.locked)
        } else {
            let Some(c) = self.schematic.components.iter_mut().find(|c| c.id == id) else {
                return false;
            };
            (&mut c.position, &mut c.rotation, c.locked)
        };
        if locked {
            return false;
        }
        let field = match *axis {
            "x" => &mut position.x,
            "y" => &mut position.y,
            _ => rotation,
        };
        let value = if *axis == "rotation" {
            value.rem_euclid(360.0)
        } else {
            value
        };
        if (*field - value).abs() <= f32::EPSILON {
            return true;
        }
        *field = value;
        self.pcb_route_draft = None;
        if pcb {
            self.pcb.rebuild_airwires();
        } else {
            self.schematic.sync_wire_anchors();
        }
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    pub fn pcb_label_visibility(&self) -> (bool, bool) {
        (
            self.labels_visible && self.pcb_reference_visible,
            self.labels_visible && self.pcb_values_visible,
        )
    }

    pub fn pcb_preview_open(&self) -> bool {
        self.pcb_preview_open
    }

    pub fn pcb_route_layer(&self) -> PcbLayer {
        self.pcb_route_layer
    }
    pub fn pcb_route_width(&self) -> f32 {
        self.pcb_route_width
    }

    pub fn pcb_rule_net(&self) -> Option<String> {
        if let Some(draft) = &self.pcb_route_draft {
            return Some(draft.net.clone());
        }
        if let Some(id) = self.highlight_net {
            if let Some(net) = self
                .scene
                .objects
                .iter()
                .find(|object| object.net_id == Some(id))
                .and_then(|object| object.net.clone())
            {
                return Some(net);
            }
        }
        self.selection.and_then(|selection| {
            self.scene
                .objects
                .iter()
                .find(|object| {
                    object.source_id == Some(selection.source_id) && object.net.is_some()
                })
                .and_then(|object| object.net.clone())
        })
    }

    pub(super) fn apply_pcb_tool_command(&mut self, command: &str) -> bool {
        if let Some(raw) = command.strip_prefix("electronics.pcb.select:") {
            if self.active_surface != CadSurfaceKind::Pcb {
                return false;
            }
            let Ok(id) = Uuid::parse_str(raw) else {
                return false;
            };
            if !self.select_component_by_id(id) {
                return false;
            }
            self.recenter_on_selection();
            return true;
        }
        if command == "electronics.pcb.route.finish" {
            self.clear_route_error();
            let Some(mut draft) = self.pcb_route_draft.clone() else {
                return false;
            };
            if draft.via_pending {
                self.set_route_error(i18n::t("electronics.pcb.via_place_hint", self.language));
                self.touch_ui();
                return true;
            }
            if let Some(pointer) = self.pointer_world {
                let end = self
                    .route_anchor_at(pointer)
                    .filter(|pad| pad.available && pad.net == draft.net)
                    .map(|pad| pad.point)
                    .unwrap_or_else(|| self.snap_world(pointer));
                extend_route(&mut draft.points, end);
            }
            let before = self.snapshot();
            match self.commit_route_geometry(&draft) {
                Ok(id) => {
                    self.history.record(before, &self.snapshot());
                    self.dirty = true;
                    self.pcb_route_draft = None;
                    self.highlight_net = None;
                    self.selection = Some(ElectronicsSelection {
                        source_id: id,
                        kind: if self.pcb.vias.iter().any(|via| via.id == id) {
                            ElectronicsSelectionKind::Pin
                        } else {
                            ElectronicsSelectionKind::Trace
                        },
                    });
                }
                Err(error) => {
                    self.set_route_error(error);
                }
            }
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        if let Some(payload) = command.strip_prefix("electronics.pcb.rule:") {
            if self.active_surface != CadSurfaceKind::Pcb {
                return false;
            }
            let Some((target, raw)) = payload.rsplit_once(':') else {
                return false;
            };
            let Some((field, net)) = target.split_once(':') else {
                return false;
            };
            let Ok(value) = raw.parse::<f32>() else {
                return false;
            };
            let mut rules = self.pcb.rules.clone();
            if !net.is_empty() {
                if !self.pcb.net_pins.contains_key(net) {
                    return false;
                }
                let mut entry = rules.for_net(net);
                match field {
                    "width" => entry.min_width = value,
                    "clearance" => entry.clearance = value,
                    _ => return false,
                };
                rules.nets.insert(net.into(), entry);
            } else {
                match field {
                    "width" => rules.default.min_width = value,
                    "clearance" => rules.default.clearance = value,
                    "edge" => rules.edge_clearance = value,
                    "drill" => rules.min_drill = value,
                    "ring" => rules.min_annular_ring = value,
                    "via-diameter" => rules.via_diameter = value,
                    "via-drill" => rules.via_drill = value,
                    _ => return false,
                }
            }
            if !rules.is_valid() {
                self.push_surface_error(i18n::t("electronics.pcb.rules_invalid", self.language));
                self.touch_ui();
                return true;
            }
            let before = self.snapshot();
            self.pcb.rules = rules;
            self.pcb_route_draft = None;
            if self.history.record(before, &self.snapshot()) {
                self.dirty = true;
            }
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        if command == "electronics.pcb.via" {
            if self.active_surface != CadSurfaceKind::Pcb {
                return false;
            }
            self.set_tool(ElectronicsTool::Route);
            if let Some(draft) = &mut self.pcb_route_draft {
                draft.via_pending = !draft.via_pending;
                draft.error = None;
            } else {
                self.set_route_error(i18n::t("electronics.pcb.via_start", self.language));
            }
            self.touch_ui();
            return true;
        }
        if command == "electronics.minimap.toggle" {
            self.minimap_collapsed = !self.minimap_collapsed;
            self.minimap_drag = false;
            self.touch_ui();
            return true;
        }
        if let Some(payload) = command.strip_prefix("electronics.pcb.labels:") {
            let Some((kind, raw)) = payload.split_once(':') else {
                return false;
            };
            let Ok(visible) = raw.parse::<bool>() else {
                return false;
            };
            if !matches!(kind, "reference" | "value") {
                return false;
            }
            if !self.labels_visible {
                self.pcb_reference_visible = false;
                self.pcb_values_visible = false;
            }
            match kind {
                "reference" => self.pcb_reference_visible = visible,
                "value" => self.pcb_values_visible = visible,
                _ => return false,
            };
            if visible {
                self.labels_visible = true;
            }
            self.touch_ui();
            return true;
        }
        if let Some(payload) = command.strip_prefix("electronics.component.lock:") {
            let parts: Vec<_> = payload.split(':').collect();
            let [surface, id, raw] = parts.as_slice() else {
                return false;
            };
            let (Ok(id), Ok(locked)) = (Uuid::parse_str(id), raw.parse::<bool>()) else {
                return false;
            };
            let pcb = match *surface {
                "pcb" => true,
                "schematic" => false,
                _ => return false,
            };
            if pcb != (self.active_surface == CadSurfaceKind::Pcb) {
                return false;
            }
            let before = self.snapshot();
            let field = if pcb {
                self.pcb
                    .components
                    .iter_mut()
                    .find(|c| c.component_id == id)
                    .map(|c| &mut c.locked)
            } else {
                self.schematic
                    .components
                    .iter_mut()
                    .find(|c| c.id == id)
                    .map(|c| &mut c.locked)
            };
            let Some(field) = field else {
                return false;
            };
            if *field == locked {
                return true;
            }
            *field = locked;
            self.pcb_route_draft = None;
            self.history.record(before, &self.snapshot());
            self.dirty = true;
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        if let Some(raw) = command.strip_prefix("electronics.pcb.zoom:") {
            let Ok(percent) = raw.parse::<f32>() else {
                return false;
            };
            if !percent.is_finite() || percent <= 0.0 {
                return false;
            }
            self.camera.zoom = (percent * 0.01 * 96.0 / 25.4).clamp(MIN_ZOOM, MAX_ZOOM);
            self.touch();
            return true;
        }
        if let Some(name) = command.strip_prefix("electronics.net.highlight:") {
            let Some(id) = self
                .scene
                .objects
                .iter()
                .find(|o| o.net.as_deref() == Some(name))
                .and_then(|o| o.net_id)
            else {
                return false;
            };
            self.set_highlight_net(if self.highlight_net == Some(id) {
                None
            } else {
                Some(id)
            });
            return true;
        }
        if let Some(payload) = command.strip_prefix("electronics.transform:") {
            return self.apply_inspector_transform(payload);
        }
        if command == "electronics.pcb.preview.close" {
            self.pcb_preview_open = false;
            self.touch_ui();
            return true;
        }
        if command == "electronics.pcb.preview.toggle" {
            self.pcb_preview_open = !self.pcb_preview_open;
            self.touch_ui();
            return true;
        }
        if command == "electronics.pcb.discard_removed_traces" {
            if !self.pcb.removed_traces.is_empty() {
                let before = self.snapshot();
                self.pcb.removed_traces.clear();
                self.history.record(before, &self.snapshot());
                self.dirty = true;
                self.rebuild_scene();
                self.touch_ui();
            }
            return true;
        }
        if let Some(raw) = command.strip_prefix("electronics.pcb.grid:") {
            if let Ok(value) = raw.parse::<f32>() {
                if value.is_finite() && (0.05..=100.0).contains(&value) {
                    self.pcb_grid_step = value;
                    if self.active_surface == CadSurfaceKind::Pcb {
                        self.grid_step = value;
                    }
                    self.touch_ui();
                    self.touch();
                }
            }
            return true;
        }
        if let Some(raw) = command.strip_prefix("electronics.pcb.width:") {
            if let Ok(value) = raw.parse::<f32>() {
                if value.is_finite()
                    && (raf_electronics::pcb::drc::MIN_TRACE_WIDTH_MM..=10.0).contains(&value)
                {
                    self.pcb_route_width = value;
                    if let Some(draft) = &mut self.pcb_route_draft {
                        draft.width = value;
                    }
                    self.rebuild_scene();
                    self.touch_ui();
                }
            }
            return true;
        }
        if let Some(raw) = command.strip_prefix("electronics.pcb.layer:") {
            let layer = match raw {
                "top" => PcbLayer::TopCopper,
                "bottom" => PcbLayer::BottomCopper,
                _ => return false,
            };
            if self.pcb_route_draft.is_some() {
                self.push_surface_error(i18n::t(
                    "electronics.pcb.finish_route_before_layer",
                    self.language,
                ));
            } else {
                self.pcb_route_layer = layer;
                match layer {
                    PcbLayer::TopCopper => self.pcb_layer_visibility.top_traces = true,
                    PcbLayer::BottomCopper => self.pcb_layer_visibility.bottom_traces = true,
                }
                self.rebuild_scene();
                self.touch_ui();
            }
            return true;
        }
        false
    }

    fn route_pad_at(&self, world: Vec2) -> Option<RoutePad> {
        let tolerance = PIN_SNAP_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        self.scene
            .objects
            .iter()
            .filter(|object| {
                object.kind == CadObjectKind::Pad
                    && object.net.as_ref().is_some_and(|n| !n.is_empty())
            })
            .map(|object| {
                (
                    object,
                    raf_electronics::cad_scene::cad_object_distance(object, world).min(
                        object
                            .rect
                            .map_or(f32::MAX, |rect| rect.center.distance(world)),
                    ),
                )
            })
            .filter(|(_, distance)| *distance <= tolerance)
            // At board scale two pads can share the same hit radius. The
            // closest center breaks that tie instead of always choosing pad 1.
            .min_by(|a, b| {
                a.1.total_cmp(&b.1).then_with(|| {
                    a.0.rect
                        .map_or(f32::MAX, |r| r.center.distance_squared(world))
                        .total_cmp(
                            &b.0.rect
                                .map_or(f32::MAX, |r| r.center.distance_squared(world)),
                        )
                })
            })
            .and_then(|(object, _)| {
                let component_id = object.source_id?;
                if let Some(via) = self
                    .pcb
                    .vias
                    .iter()
                    .find(|via| via.id == component_id && !via.orphaned)
                {
                    return Some(RoutePad {
                        point: via.position,
                        net: via.net.clone(),
                        net_id: object.net_id,
                        label: i18n::t("electronics.pcb.via_label", self.language),
                        available: true,
                    });
                }
                let pad_index = object.id.rsplit(':').next()?.parse::<usize>().ok()?;
                let component = self
                    .pcb
                    .components
                    .iter()
                    .find(|c| c.component_id == component_id)?;
                Some(RoutePad {
                    point: object.rect?.center,
                    net: object.net.clone()?,
                    net_id: object.net_id,
                    label: format!(
                        "{} / {}",
                        component.designator,
                        object.label.as_deref().unwrap_or("?")
                    ),
                    available: self.pcb.pad_supports_layer(
                        component_id,
                        pad_index,
                        self.pcb_route_layer,
                    ),
                })
            })
    }

    fn route_anchor_at(&self, world: Vec2) -> Option<RoutePad> {
        self.route_pad_at(world).or_else(|| {
            self.pcb
                .traces
                .iter()
                .filter(|trace| trace.layer == self.pcb_route_layer)
                .flat_map(|trace| {
                    trace
                        .points
                        .windows(2)
                        .map(move |p| (trace, nearest_on_segment(world, p[0], p[1])))
                })
                .filter(|(trace, p)| {
                    p.distance(world) <= trace.width * 0.5 + 6.0 / self.camera.zoom.max(MIN_ZOOM)
                })
                .min_by(|a, b| {
                    a.1.distance_squared(world)
                        .total_cmp(&b.1.distance_squared(world))
                })
                .map(|(trace, point)| RoutePad {
                    point,
                    net: trace.net.clone(),
                    net_id: self
                        .scene
                        .objects
                        .iter()
                        .find(|o| o.source_id == Some(trace.id))
                        .and_then(|o| o.net_id),
                    label: trace.net.clone(),
                    available: true,
                })
        })
    }

    pub(super) fn clear_route_error(&mut self) {
        if let Some(previous) = self.route_error.take() {
            self.surface_errors.retain(|message| message != &previous);
            self.touch_ui();
        }
    }

    fn set_route_error(&mut self, message: String) {
        self.clear_route_error();
        self.route_error = Some(message.clone());
        self.push_surface_error(message);
    }

    fn queue_route_via(&mut self, world: Vec2) -> bool {
        let Some(mut draft) = self.pcb_route_draft.clone() else {
            return false;
        };
        self.clear_route_error();
        let position = self.snap_world(world);
        let rules = &self.pcb.rules;
        let mut candidate = self.pcb.clone();
        for via in &draft.vias {
            if let Err(error) =
                candidate.try_add_via(&via.net, via.position, via.diameter, via.drill)
            {
                self.set_route_error(error);
                return true;
            }
        }
        if let Err(error) =
            candidate.try_add_via(&draft.net, position, rules.via_diameter, rules.via_drill)
        {
            self.set_route_error(error);
            return true;
        }
        extend_route(&mut draft.points, position);
        draft.legs.push(raf_electronics::pcb::PcbTrace {
            id: Uuid::new_v4(),
            net: draft.net.clone(),
            layer: draft.layer,
            width: draft.width,
            points: draft.points,
        });
        draft.vias.push(raf_electronics::pcb::PcbVia {
            id: Uuid::new_v4(),
            net: draft.net.clone(),
            position,
            diameter: rules.via_diameter,
            drill: rules.via_drill,
            orphaned: false,
        });
        draft.layer = if draft.layer == PcbLayer::TopCopper {
            PcbLayer::BottomCopper
        } else {
            PcbLayer::TopCopper
        };
        self.pcb_route_layer = draft.layer;
        draft.points = vec![position];
        draft.via_pending = false;
        draft.error = None;
        self.pcb_route_draft = Some(draft);
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    /// Returns false only if there is no armed route and no pad to start from.
    pub(super) fn route_physical_click(&mut self, world: Vec2) -> bool {
        if self
            .pcb_route_draft
            .as_ref()
            .is_some_and(|draft| draft.via_pending)
        {
            return self.queue_route_via(world);
        }
        let pad = self.route_anchor_at(world);
        if self.pcb_route_draft.is_none() {
            let Some(pad) = pad else {
                return false;
            };
            if !pad.available {
                self.set_route_error(i18n::t("electronics.pcb.pad_wrong_layer", self.language));
                self.touch_ui();
                return true;
            }
            self.clear_route_error();
            self.pcb_route_width = self
                .pcb_route_width
                .max(self.pcb.rules.for_net(&pad.net).min_width);
            self.pcb_route_draft = Some(PcbRouteDraft {
                net: pad.net.clone(),
                points: vec![pad.point],
                layer: self.pcb_route_layer,
                width: self
                    .pcb_route_width
                    .max(self.pcb.rules.for_net(&pad.net).min_width),
                document_revision: pcb_fingerprint(&self.pcb),
                source: pad.label,
                error: None,
                legs: Vec::new(),
                vias: Vec::new(),
                via_pending: false,
            });
            self.highlight_net = pad.net_id;
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        let mut draft = self.pcb_route_draft.clone().unwrap();
        if draft.document_revision != pcb_fingerprint(&self.pcb) {
            self.pcb_route_draft = None;
            self.highlight_net = None;
            self.set_route_error(i18n::t(
                "electronics.pcb.route_document_changed",
                self.language,
            ));
            self.rebuild_scene();
            return true;
        }
        let end = pad
            .as_ref()
            .map(|pad| pad.point)
            .unwrap_or_else(|| self.snap_world(world));
        if let Some(pad) = &pad {
            if pad.net != draft.net || !pad.available {
                let message = i18n::t(
                    if !pad.available {
                        "electronics.pcb.pad_wrong_layer"
                    } else {
                        "electronics.pcb.route_wrong_net"
                    },
                    self.language,
                );
                self.pcb_route_draft.as_mut().unwrap().error = Some(message.clone());
                self.set_route_error(message);
                self.touch_ui();
                return true;
            }
            if end.distance(draft.points[0]) <= 0.0001 {
                return true;
            }
        }
        extend_route(&mut draft.points, end);
        if pad.is_some() {
            let before = self.snapshot();
            let attempt = self.commit_route_geometry(&draft);
            match attempt {
                Ok(id) => {
                    self.clear_route_error();
                    self.history.record(before, &self.snapshot());
                    self.dirty = true;
                    self.pcb_route_draft = None;
                    self.highlight_net = None;
                    self.selection = Some(ElectronicsSelection {
                        source_id: id,
                        kind: ElectronicsSelectionKind::Trace,
                    });
                }
                Err(error) => {
                    self.pcb_route_draft.as_mut().unwrap().error = Some(error.clone());
                    self.set_route_error(error);
                }
            }
        } else {
            self.clear_route_error();
            draft.error = None;
            self.pcb_route_draft = Some(draft);
        }
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    fn commit_route_geometry(&mut self, draft: &PcbRouteDraft) -> Result<Uuid, String> {
        if draft.document_revision != pcb_fingerprint(&self.pcb) {
            return Err(i18n::t(
                "electronics.pcb.route_document_changed",
                self.language,
            ));
        }
        let mut candidate = self.pcb.clone();
        let mut last = None;
        for via in &draft.vias {
            last = Some(candidate.try_add_via(&via.net, via.position, via.diameter, via.drill)?);
        }
        for leg in &draft.legs {
            if leg.points.len() >= 2 {
                last = Some(candidate.try_add_trace(
                    &leg.net,
                    leg.layer,
                    leg.width,
                    leg.points.clone(),
                )?);
            }
        }
        if draft.points.len() >= 2 {
            last = Some(candidate.try_add_trace(
                &draft.net,
                draft.layer,
                draft.width,
                draft.points.clone(),
            )?);
        }
        let id = last.ok_or_else(|| i18n::t("electronics.pcb.via_start", self.language))?;
        for leg in &draft.legs {
            for &point in [leg.points.first(), leg.points.last()]
                .into_iter()
                .flatten()
            {
                split_trace_at(&mut candidate, &leg.net, leg.layer, point);
            }
        }
        for &point in [draft.points.first(), draft.points.last()]
            .into_iter()
            .flatten()
        {
            split_trace_at(&mut candidate, &draft.net, draft.layer, point);
        }
        self.pcb = candidate;
        Ok(id)
    }

    pub(super) fn undo_route_point(&mut self) -> bool {
        self.clear_route_error();
        let Some(draft) = &mut self.pcb_route_draft else {
            return false;
        };
        if draft.points.len() > 1 {
            draft.points.pop();
        } else if let Some(leg) = draft.legs.pop() {
            draft.vias.pop();
            draft.points = leg.points;
            draft.layer = leg.layer;
            self.pcb_route_layer = leg.layer;
            draft.via_pending = false;
        } else {
            self.pcb_route_draft = None;
            self.highlight_net = None;
        }
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    pub(super) fn route_hover_identity(&self, world: Vec2) -> Option<(String, bool)> {
        self.route_anchor_at(world)
            .map(|pad| (pad.label, pad.available))
    }

    /// The same resolved pad drives snapping, the halo and the readable HUD.
    pub fn pcb_routing_hint(&self) -> Option<(String, String, bool)> {
        if self.active_surface != CadSurfaceKind::Pcb || self.tool != ElectronicsTool::Route {
            return None;
        }
        let t = |key| i18n::t(key, self.language);
        let target = self
            .pointer_world
            .and_then(|world| self.route_pad_at(world));
        if let Some(draft) = &self.pcb_route_draft {
            if draft.via_pending {
                return Some((
                    t("electronics.pcb.via_label"),
                    t("electronics.pcb.via_place_hint"),
                    false,
                ));
            }
            let heading = format!(
                "{}  ·  {}  ·  {}",
                t("electronics.pcb.routing"),
                draft.source,
                draft.net
            );
            if let Some(error) = &draft.error {
                return Some((
                    heading,
                    format!("{error}  ·  {}", t("electronics.pcb.route_recover")),
                    true,
                ));
            }
            let body = match target {
                Some(pad) if !pad.available => {
                    format!("{}  ·  {}", pad.label, t("electronics.pcb.pad_wrong_layer"))
                }
                Some(pad) if pad.net != draft.net => format!(
                    "{}  ·  {}  ·  {}",
                    pad.label,
                    pad.net,
                    t("electronics.pcb.route_wrong_net")
                ),
                Some(pad) if pad.point.distance(draft.points[0]) > 0.0001 => format!(
                    "{}  →  {}  ·  {}",
                    t("electronics.pcb.route_finish"),
                    pad.label,
                    t("electronics.pcb.route_escape")
                ),
                _ => t("electronics.pcb.route_waypoint"),
            };
            return Some((heading, body, false));
        }
        Some(match target {
            Some(pad) => (
                format!(
                    "{}  ·  {}  ·  {}",
                    t("electronics.pcb.route_start"),
                    pad.label,
                    pad.net
                ),
                t(if pad.available {
                    "electronics.pcb.route_click_start"
                } else {
                    "electronics.pcb.pad_wrong_layer"
                }),
                !pad.available,
            ),
            None => (
                t("electronics.pcb.route_ready"),
                t("electronics.pcb.route_pick_pad"),
                false,
            ),
        })
    }

    fn push_route_targets(&mut self) {
        if self.tool != ElectronicsTool::Route {
            return;
        }
        let active_layer = match self.pcb_route_layer {
            PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
            PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
        };
        for object in &mut self.scene.objects {
            if matches!(object.kind, CadObjectKind::Trace | CadObjectKind::Pad)
                && object.layer != active_layer
                && !object.drill.is_some_and(|drill| drill.plated)
            {
                object.color_rgba[3] = object.color_rgba[3].min(90);
            }
        }
        let hover = self
            .pointer_world
            .and_then(|world| self.route_pad_at(world));
        let mut targets = Vec::new();
        if let Some(draft) = &self.pcb_route_draft {
            for object in &self.scene.objects {
                if object.kind != CadObjectKind::Pad || object.net.as_deref() != Some(&draft.net) {
                    continue;
                }
                let Some(rect) = object.rect else {
                    continue;
                };
                let Some(component_id) = object.source_id else {
                    continue;
                };
                let Some(index) = object.id.rsplit(':').next().and_then(|s| s.parse().ok()) else {
                    continue;
                };
                if self
                    .pcb
                    .pad_supports_layer(component_id, index, draft.layer)
                {
                    targets.push((rect.center, 6.0, [98, 214, 180, 175]));
                }
            }
        }
        if let Some(pad) = hover {
            let compatible = pad.available
                && self
                    .pcb_route_draft
                    .as_ref()
                    .is_none_or(|d| d.net == pad.net);
            targets.push((
                pad.point,
                11.0,
                if compatible {
                    [98, 232, 197, 255]
                } else {
                    [240, 104, 104, 255]
                },
            ));
        }
        for (index, (point, radius, color)) in targets.into_iter().enumerate() {
            let radius = radius / self.camera.zoom.max(MIN_ZOOM);
            self.scene.objects.push(CadObject {
                id: format!("pcb-route-target:{index}"),
                source_id: None,
                kind: CadObjectKind::BoardOutline,
                layer: CadLayerKind::Overlay,
                pick_priority: CadPickPriority::Background,
                rect: None,
                points: (0..=24)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 24.0;
                        point + Vec2::new(angle.cos(), angle.sin()) * radius
                    })
                    .collect(),
                line_paths: Vec::new(),
                label: None,
                net: None,
                // These rings are authoring feedback for the focused net,
                // including a red incompatible target. Net dimming must not
                // make their compatibility signal disappear.
                net_id: self.highlight_net,
                color_rgba: color,
                shape: Default::default(),
                rotation_degrees: 0.0,
                drill: None,
                stroke_width_world: None,
            });
        }
    }

    pub(super) fn push_route_preview(&mut self) {
        if self.active_surface != CadSurfaceKind::Pcb {
            return;
        }
        self.push_route_targets();
        let Some(draft) = self.pcb_route_draft.as_ref() else {
            return;
        };
        let mut preview = PcbLayout::new("route-preview");
        preview.traces = draft.legs.clone();
        preview.vias = draft.vias.clone();
        if draft.via_pending {
            if let Some(pointer) = self.pointer_world {
                preview.vias.push(raf_electronics::pcb::PcbVia {
                    id: Uuid::nil(),
                    net: draft.net.clone(),
                    position: self.snap_world(pointer),
                    diameter: self.pcb.rules.via_diameter,
                    drill: self.pcb.rules.via_drill,
                    orphaned: false,
                });
            }
        }
        self.scene.objects.extend(
            CadScene::from_pcb(&preview)
                .objects
                .into_iter()
                .filter(|object| matches!(object.kind, CadObjectKind::Trace | CadObjectKind::Pad))
                .map(|mut object| {
                    object.source_id = None;
                    object.pick_priority = CadPickPriority::Background;
                    object
                }),
        );
        let mut points = draft.points.clone();
        if let Some(pointer) = self.pointer_world {
            let end = self
                .route_anchor_at(pointer)
                .filter(|pad| pad.net == draft.net && pad.available)
                .map(|pad| pad.point)
                .unwrap_or_else(|| self.snap_world(pointer));
            extend_route(&mut points, end);
        }
        self.scene.objects.push(CadObject {
            id: "pcb-route-preview".into(),
            source_id: None,
            kind: CadObjectKind::Trace,
            layer: CadLayerKind::Overlay,
            pick_priority: CadPickPriority::Overlay,
            rect: None,
            points,
            line_paths: Vec::new(),
            label: None,
            net: Some(draft.net.clone()),
            net_id: self.highlight_net,
            color_rgba: match draft.layer {
                PcbLayer::TopCopper => [207, 150, 91, 220],
                PcbLayer::BottomCopper => [104, 158, 194, 220],
            },
            shape: Default::default(),
            rotation_degrees: 0.0,
            drill: None,
            stroke_width_world: Some(draft.width),
        });
    }
}
