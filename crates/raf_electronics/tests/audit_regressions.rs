use glam::Vec2;
use raf_electronics::pcb::layout::PcbTrace;
use raf_electronics::schematic::component_pin_world_position;
use raf_electronics::{ElectronicComponent, PcbLayer, PcbLayout, Schematic};
use uuid::Uuid;

fn pin(s: &Schematic, c: usize, p: usize) -> Vec2 {
    component_pin_world_position(&s.components[c], &s.components[c].pins[p])
}
fn connect(s: &mut Schematic, a: (usize, usize), b: (usize, usize), name: &str) {
    s.add_wire(pin(s, a.0, a.1), pin(s, b.0, b.1), name);
}
fn put(s: &mut Schematic, mut c: ElectronicComponent, x: f32, y: f32) {
    c.position = Vec2::new(x, y);
    s.add_component(c);
}
fn two_resistors() -> Schematic {
    let mut s = Schematic::new("audit");
    put(&mut s, ElectronicComponent::resistor("1k"), 0., 0.);
    put(&mut s, ElectronicComponent::resistor("1k"), 200., 100.);
    s
}

#[test]
fn a_bent_wire_must_connect_its_two_pins() {
    let mut s = two_resistors();
    let a = pin(&s, 0, 1);
    let b = pin(&s, 1, 0);
    s.add_wire_path(&[a, Vec2::new(b.x, a.y), b], "SIGNAL");
    let n = s.netlist();
    assert_eq!(
        n.net_for_pin(0, 1).unwrap().id,
        n.net_for_pin(1, 0).unwrap().id,
        "two contiguous segments must preserve connectivity"
    );
}

#[test]
fn b_deletion_must_not_create_duplicate_designators() {
    let mut s = two_resistors();
    put(&mut s, ElectronicComponent::resistor("1k"), 300., 0.);
    s.components.remove(0);
    put(&mut s, ElectronicComponent::resistor("1k"), 400., 0.);
    let names: std::collections::HashSet<_> = s.components.iter().map(|c| &c.designator).collect();
    assert_eq!(
        names.len(),
        s.components.len(),
        "designators: {:?}",
        s.components
            .iter()
            .map(|c| &c.designator)
            .collect::<Vec<_>>()
    );
}

#[test]
fn c_two_sources_sharing_only_negative_must_not_be_a_short() {
    let mut s = Schematic::new("two supplies");
    put(&mut s, ElectronicComponent::dc_source(5.), 0., 0.);
    put(&mut s, ElectronicComponent::dc_source(12.), 200., 0.);
    connect(&mut s, (0, 1), (1, 1), "GND");
    let report = s.run_drc();
    assert!(
        !report.errors.iter().any(|e| e.rule == "short_circuit"),
        "errors: {:?}",
        report.errors
    );
}

#[test]
fn d_orphaned_traces_must_survive_a_second_sync() {
    let s = two_resistors();
    let mut pcb = PcbLayout::new("audit");
    pcb.sync_from_schematic(&s);
    let id = Uuid::new_v4();
    pcb.traces.push(PcbTrace {
        id,
        net: "DELETED_NET".into(),
        layer: PcbLayer::TopCopper,
        width: 6.,
        points: vec![Vec2::ZERO, Vec2::new(100., 0.)],
    });
    pcb.sync_from_schematic(&s);
    assert_eq!(pcb.removed_traces.len(), 1);
    pcb.sync_from_schematic(&s);
    assert!(
        pcb.removed_traces.iter().any(|t| t.id == id),
        "second sync lost the parked trace"
    );
}

#[test]
fn e_bottom_trace_must_not_connect_top_smd_pads_without_vias() {
    let mut s = two_resistors();
    connect(&mut s, (0, 1), (1, 0), "SIGNAL");
    let mut pcb = PcbLayout::new("audit");
    pcb.sync_from_schematic(&s);
    let a = pcb.pad_world_position(0, 1).unwrap();
    let b = pcb.pad_world_position(1, 0).unwrap();
    assert_eq!(pcb.airwires.len(), 1);
    pcb.traces.push(PcbTrace {
        id: Uuid::new_v4(),
        net: "SIGNAL".into(),
        layer: PcbLayer::BottomCopper,
        width: 6.,
        points: vec![a, b],
    });
    pcb.rebuild_airwires();
    assert_eq!(
        pcb.airwires.len(),
        1,
        "a bottom trace cannot connect these top 0805 pads"
    );
}

#[test]
fn f_led_and_resistor_without_power_must_not_generate_voltage() {
    let mut s = Schematic::new("passive circuit");
    put(&mut s, ElectronicComponent::led(), 0., 0.);
    put(&mut s, ElectronicComponent::resistor("1k"), 200., 0.);
    put(&mut s, ElectronicComponent::ground(), 400., 0.);
    connect(&mut s, (0, 0), (1, 0), "A");
    connect(&mut s, (0, 1), (1, 1), "K");
    connect(&mut s, (1, 1), (2, 0), "GND");
    let result = s.simulate_dc();
    assert!(result.converged);
    assert!(
        result.node_voltages.values().all(|v| v.abs() < 1e-9),
        "passive circuit voltages: {:?}; currents: {:?}",
        result.node_voltages,
        result.component_currents
    );
}

#[test]
fn g_auto_net_names_must_not_collide_with_explicit_names() {
    let mut s = Schematic::new("names");
    for x in [0., 100., 200., 300.] {
        put(&mut s, ElectronicComponent::ground(), x, 0.);
    }
    connect(&mut s, (0, 0), (1, 0), "N001");
    let n = s.netlist();
    let names: std::collections::HashSet<_> = n.nets.iter().map(|n| &n.name).collect();
    assert_eq!(names.len(), n.nets.len(), "nets: {:?}", n.nets);
}

#[test]
fn h_parallel_resistor_must_not_count_as_led_current_limiter() {
    let mut s = Schematic::new("parallel resistor");
    put(&mut s, ElectronicComponent::dc_source(5.), 0., 0.);
    put(&mut s, ElectronicComponent::led(), 200., 0.);
    put(&mut s, ElectronicComponent::resistor("1k"), 400., 0.);
    connect(&mut s, (0, 0), (1, 0), "VCC");
    connect(&mut s, (0, 1), (1, 1), "GND");
    connect(&mut s, (1, 0), (2, 0), "VCC");
    connect(&mut s, (1, 1), (2, 1), "GND");
    let report = s.run_drc();
    assert!(
        report.warnings.iter().any(|e| e.rule == "led_no_resistor"),
        "a resistor parallel to the LED does not limit its current: {:?}",
        report
    );
}

#[test]
fn i_control_direct_wire_connects_two_pins() {
    let mut s = two_resistors();
    connect(&mut s, (0, 1), (1, 0), "SIGNAL");
    let n = s.netlist();
    assert_eq!(
        n.net_for_pin(0, 1).unwrap().id,
        n.net_for_pin(1, 0).unwrap().id
    );
}

#[test]
fn j_control_five_volt_source_with_one_kilohm_load() {
    let mut s = Schematic::new("powered resistor");
    put(&mut s, ElectronicComponent::dc_source(5.), 0., 0.);
    put(&mut s, ElectronicComponent::resistor("1k"), 200., 0.);
    put(&mut s, ElectronicComponent::ground(), 400., 0.);
    connect(&mut s, (0, 0), (1, 0), "VCC");
    connect(&mut s, (0, 1), (1, 1), "GND");
    connect(&mut s, (1, 1), (2, 0), "GND");
    let result = s.simulate_dc();
    assert!(result.converged);
    assert!((result.component_currents[&1] - 0.005).abs() < 1e-6);
}

fn powered_led(reverse: bool) -> Schematic {
    let mut s = Schematic::new("series LED");
    put(&mut s, ElectronicComponent::dc_source(5.), 0., 0.);
    put(&mut s, ElectronicComponent::resistor("1k"), 200., 100.);
    put(&mut s, ElectronicComponent::led(), 400., 200.);
    put(&mut s, ElectronicComponent::ground(), 600., 300.);
    connect(&mut s, (0, 0), (1, 0), "VCC");
    connect(&mut s, (1, 1), (2, usize::from(reverse)), "ANODE");
    connect(&mut s, (2, usize::from(!reverse)), (3, 0), "GND");
    connect(&mut s, (0, 1), (3, 0), "GND");
    s
}

#[test]
fn powered_led_obeys_branch_law_kirchhoff_and_power_balance() {
    let s = powered_led(false);
    let result = s.simulate_dc();
    assert!(result.converged, "{:?}", result.messages);
    assert!((result.component_currents[&2] - 3.0 / 1100.01).abs() < 1e-8);
    for net in s.netlist().nets {
        let balance: f64 = net
            .pins
            .iter()
            .map(|&(ci, pi)| {
                let current = result.component_currents[&ci];
                if pi == 0 {
                    current
                } else {
                    -current
                }
            })
            .sum();
        assert!(balance.abs() < 1e-8, "{} imbalance {balance}", net.name);
    }
    assert!(result.component_power.values().sum::<f64>().abs() < 1e-8);
    assert!(!s
        .run_drc()
        .warnings
        .iter()
        .any(|issue| issue.rule == "led_no_resistor"));
}

#[test]
fn reverse_led_stays_off_and_does_not_supply_power() {
    let result = powered_led(true).simulate_dc();
    assert!(result.converged, "{:?}", result.messages);
    assert!(result.component_currents[&2].abs() < 1e-8);
    assert!(result.component_power[&2] >= 0.0);
}

#[test]
fn incompatible_parallel_supplies_are_reported() {
    let mut s = Schematic::new("conflicting supplies");
    put(&mut s, ElectronicComponent::dc_source(5.), 0., 0.);
    put(&mut s, ElectronicComponent::dc_source(12.), 200., 100.);
    connect(&mut s, (0, 0), (1, 0), "VCC");
    connect(&mut s, (0, 1), (1, 1), "GND");
    assert!(s
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "short_circuit"));
}

#[test]
fn isolated_duplicate_labels_remain_distinct_electrical_nets() {
    let mut s = Schematic::new("repeated labels");
    for x in [0., 100., 200., 300.] {
        put(&mut s, ElectronicComponent::ground(), x, 0.);
    }
    connect(&mut s, (0, 0), (1, 0), "SIGNAL");
    connect(&mut s, (2, 0), (3, 0), "SIGNAL");
    let n = s.netlist();
    assert_ne!(
        n.net_for_pin(0, 0).unwrap().name,
        n.net_for_pin(2, 0).unwrap().name
    );
    assert!(s
        .run_drc()
        .warnings
        .iter()
        .any(|issue| issue.rule == "ambiguous_net_label"));
    let scene = raf_electronics::CadScene::from_schematic(&s);
    let labels: Vec<_> = scene
        .objects
        .iter()
        .filter(|o| o.kind == raf_electronics::CadObjectKind::NetLabel)
        .collect();
    assert_eq!(labels.len(), 2);
    assert_ne!(labels[0].label, labels[1].label);
    assert!(labels.iter().all(|label| label.label == label.net));
}

#[test]
fn wire_crossing_requires_an_explicit_junction_split() {
    let mut s = two_resistors();
    let a = pin(&s, 0, 1);
    let b = pin(&s, 1, 0);
    let crossing = Vec2::new(b.x, a.y);
    s.add_wire(a, Vec2::new(b.x + 100., a.y), "HORIZONTAL");
    s.add_wire(Vec2::new(b.x, a.y - 100.), b, "VERTICAL");
    let n = s.netlist();
    assert_ne!(
        n.net_for_pin(0, 1).unwrap().id,
        n.net_for_pin(1, 0).unwrap().id
    );
    assert!(s.split_wire_at(0, crossing));
    // Splitting removes the original and appends its replacements; the
    // original vertical wire is now at index zero.
    assert!(s.split_wire_at(0, crossing));
    let n = s.netlist();
    assert_eq!(
        n.net_for_pin(0, 1).unwrap().id,
        n.net_for_pin(1, 0).unwrap().id
    );
}

fn linked_board() -> (Schematic, PcbLayout) {
    let mut s = two_resistors();
    connect(&mut s, (0, 1), (1, 0), "SIGNAL");
    let mut p = PcbLayout::new("board");
    p.sync_from_schematic(&s);
    (s, p)
}

#[test]
fn pcb_route_uses_copper_layer_width_and_actual_contact() {
    let (_, mut pcb) = linked_board();
    pcb.components
        .iter_mut()
        .for_each(|c| c.layer = PcbLayer::BottomCopper);
    pcb.rebuild_airwires();
    pcb.try_route_airwire(0).unwrap();
    assert_eq!(pcb.traces[0].layer, PcbLayer::BottomCopper);
    assert_eq!(pcb.traces[0].width, raf_core::units::DEFAULT_TRACE_WIDTH_MM);
    assert!(pcb.airwires.is_empty());
    assert!(pcb.run_drc().errors.is_empty());
}

#[test]
fn pcb_route_refuses_crossing_another_net_without_partial_edit() {
    let (_, mut pcb) = linked_board();
    let a = pcb.airwires[0].from;
    let b = pcb.airwires[0].to;
    let x = (a.x + b.x) * 0.5;
    pcb.traces.push(PcbTrace {
        id: Uuid::new_v4(),
        net: "BLOCK".into(),
        layer: PcbLayer::TopCopper,
        width: 0.25,
        points: vec![Vec2::new(x, a.y - 30.), Vec2::new(x, a.y + 30.)],
    });
    let before = pcb.clone();
    assert!(pcb.try_route_airwire(0).is_err());
    assert_eq!(pcb.traces, before.traces);
    assert_eq!(pcb.airwires, before.airwires);
}

#[test]
fn pcb_drc_checks_copper_clearance_only_on_the_same_layer() {
    let mut pcb = PcbLayout::new("clearance");
    pcb.traces.push(PcbTrace {
        id: Uuid::new_v4(),
        net: "A".into(),
        layer: PcbLayer::TopCopper,
        width: 0.25,
        points: vec![Vec2::new(20., 50.), Vec2::new(100., 50.)],
    });
    pcb.traces.push(PcbTrace {
        id: Uuid::new_v4(),
        net: "B".into(),
        layer: PcbLayer::TopCopper,
        width: 0.25,
        points: vec![Vec2::new(50., 20.), Vec2::new(50., 100.)],
    });
    assert!(pcb
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "copper_clearance"));
    pcb.traces[1].layer = PcbLayer::BottomCopper;
    assert!(!pcb
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "copper_clearance"));
    pcb.traces[0].points[0].x = -10.;
    assert!(pcb
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "copper_outside_board"));
}

#[test]
fn net_rename_preserves_routing_but_net_split_parks_it() {
    let (mut s, mut pcb) = linked_board();
    pcb.try_route_airwire(0).unwrap();
    let id = pcb.traces[0].id;
    s.wires[0].net = "RENAMED".into();
    pcb.sync_from_schematic(&s);
    assert_eq!(pcb.traces[0].id, id);
    assert_eq!(pcb.traces[0].net, "RENAMED");
    let saved = ron::ser::to_string(&pcb).unwrap();
    pcb = ron::from_str(&saved).unwrap();
    s.wires.clear();
    pcb.sync_from_schematic(&s);
    pcb.sync_from_schematic(&s);
    assert!(pcb.traces.is_empty());
    assert_eq!(pcb.removed_traces[0].id, id);
    let target = pcb.components[0].pad_nets[1].clone();
    assert!(pcb.restore_removed_trace(id, &target));
    assert_eq!(pcb.traces[0].id, id);
    assert!(pcb.removed_traces.is_empty());
}

#[test]
fn cancellation_returns_no_partial_analysis_results() {
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let s = powered_led(false);
    assert!(
        raf_electronics::netlist::Netlist::from_schematic_cancellable(&s, &cancelled).is_none()
    );
    assert!(raf_electronics::drc::run_drc_cancellable(&s, &cancelled).is_none());
    assert!(raf_electronics::simulation::simulate_dc_cancellable(&s, &cancelled).is_none());
    assert!(linked_board().1.run_drc_cancellable(&cancelled).is_none());
}

#[test]
fn stale_airwire_cannot_route_to_the_old_pad_position() {
    let (_, mut pcb) = linked_board();
    pcb.components[1].position.y += 20.;
    assert!(pcb.try_route_airwire(0).is_err());
    assert!(pcb.traces.is_empty());
    pcb.rebuild_airwires();
    assert!(pcb.try_route_airwire(0).is_ok());
}

#[test]
fn pcb_drc_rejects_self_intersecting_and_almost_closed_outlines() {
    let mut pcb = PcbLayout::new("outline");
    pcb.board_outline.points = vec![
        Vec2::new(20., 20.),
        Vec2::new(100., 100.),
        Vec2::new(20., 100.),
        Vec2::new(100., 20.),
        Vec2::new(20., 20.),
    ];
    assert!(pcb
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "invalid_board_outline"));
    pcb.board_outline = raf_electronics::BoardOutline::default_rect(100., 100.);
    pcb.board_outline.points.last_mut().unwrap().x = 0.4;
    assert!(pcb
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "invalid_board_outline"));
}
