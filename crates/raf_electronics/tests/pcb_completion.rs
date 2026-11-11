use glam::Vec2;
use raf_electronics::pcb::{parse_kicad_footprint, BoardOutline};
use raf_electronics::schematic::component_pin_world_position;
use raf_electronics::{ElectronicComponent, PcbLayer, PcbLayout, Schematic};

fn connect(s: &mut Schematic, a: (usize, usize), b: (usize, usize), net: &str) {
    let pin = |c: usize, p: usize| {
        component_pin_world_position(&s.components[c], &s.components[c].pins[p])
    };
    s.add_wire(pin(a.0, a.1), pin(b.0, b.1), net);
}

fn board() -> PcbLayout {
    let asset = parse_kicad_footprint(
        r#"(footprint "TwoPads"
        (pad "1" smd rect (at -1 0) (size 1 1) (layers "F.Cu" "F.Mask"))
        (pad "2" smd rect (at 1 0) (size 1 1) (layers "F.Cu" "F.Mask")))"#,
    )
    .unwrap();
    let mut s = Schematic::new("via continuity");
    for x in [0.0, 200.0] {
        let mut component = ElectronicComponent::resistor("1k");
        component.position.x = x;
        component.footprint = asset.id.clone();
        s.add_component(component);
    }
    connect(&mut s, (0, 1), (1, 0), "LINK");
    let mut b = PcbLayout::new("via continuity");
    b.footprint_assets.insert(asset.id.clone(), asset);
    b.sync_from_schematic(&s);
    b.board_outline = BoardOutline::default_rect(40.0, 20.0);
    b.components[0].position = Vec2::new(5.0, 10.0);
    b.components[1].position = Vec2::new(35.0, 10.0);
    b.rebuild_airwires();
    b
}

#[test]
fn plated_vias_connect_faces_and_survive_save_reload_cam_and_deletion() {
    let mut b = board();
    let a = b.pad_world_position(0, 1).unwrap();
    let z = b.pad_world_position(1, 0).unwrap();
    let v1 = Vec2::new(10.0, 10.0);
    let v2 = Vec2::new(30.0, 10.0);
    b.try_add_trace("LINK", PcbLayer::TopCopper, 0.25, vec![a, v1])
        .unwrap();
    b.try_add_trace("LINK", PcbLayer::BottomCopper, 0.25, vec![v1, v2])
        .unwrap();
    b.try_add_trace("LINK", PcbLayer::TopCopper, 0.25, vec![v2, z])
        .unwrap();
    assert_eq!(
        b.airwires.len(),
        1,
        "opposite faces must stay disconnected without plating"
    );
    b.try_add_via("LINK", v1, 0.7, 0.3).unwrap();
    assert_eq!(b.airwires.len(), 1);
    b.try_add_via("LINK", v2, 0.7, 0.3).unwrap();
    assert!(b.airwires.is_empty());
    assert!(
        b.run_manufacturing_preflight().errors.is_empty(),
        "{:?}",
        b.run_drc()
    );
    let serialized = ron::to_string(&b).unwrap();
    let mut restored: PcbLayout = ron::from_str(&serialized).unwrap();
    assert_eq!(
        raf_electronics::pcb_fingerprint(&b),
        raf_electronics::pcb_fingerprint(&restored)
    );
    let files = raf_electronics::pcb::export_manufacturing_files(&restored).unwrap();
    assert!(files.files["board-PTH.drl"].contains("C0.300000"));
    assert!(files.files["board-PTH.drl"].contains("X10.000000Y-10.000000"));
    assert!(files.files["board-F_Cu.gbr"].contains("X10000000Y-10000000D03*"));
    assert!(files.files["board-B_Cu.gbr"].contains("X10000000Y-10000000D03*"));
    restored.vias.pop();
    restored.rebuild_airwires();
    assert_eq!(restored.airwires.len(), 1);
    assert!(raf_electronics::pcb::export_manufacturing_files(&restored).is_err());
}

#[test]
fn via_failures_are_atomic_and_net_rules_drive_routing_and_drc() {
    let mut b = board();
    let before = raf_electronics::pcb_fingerprint(&b);
    for (net, point, diameter, drill) in [
        ("UNKNOWN", Vec2::new(10.0, 10.0), 0.7, 0.3),
        ("LINK", Vec2::new(0.1, 10.0), 0.7, 0.3),
        ("LINK", Vec2::new(f32::NAN, 10.0), 0.7, 0.3),
        ("LINK", Vec2::new(10.0, 10.0), 0.5, 0.4),
        ("LINK", Vec2::new(4.0, 10.0), 0.7, 0.3),
    ] {
        assert!(b.try_add_via(net, point, diameter, drill).is_err());
    }
    assert_eq!(before, raf_electronics::pcb_fingerprint(&b));
    b.rules.nets.insert(
        "LINK".into(),
        raf_electronics::pcb::rules::NetRules {
            min_width: 0.5,
            clearance: 0.4,
        },
    );
    let a = b.pad_world_position(0, 1).unwrap();
    assert!(b
        .try_add_trace(
            "LINK",
            PcbLayer::TopCopper,
            0.25,
            vec![a, Vec2::new(20.0, 10.0)]
        )
        .is_err());
    b.try_add_trace(
        "LINK",
        PcbLayer::TopCopper,
        0.5,
        vec![a, Vec2::new(20.0, 10.0)],
    )
    .unwrap();
    b.rules.nets.get_mut("LINK").unwrap().min_width = 0.8;
    assert!(b
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "invalid_trace"));
    b.try_add_via("LINK", Vec2::new(25.0, 15.0), 0.7, 0.3)
        .unwrap();
    assert!(b
        .run_drc()
        .warnings
        .iter()
        .any(|issue| issue.rule == "unconnected_via"));
    assert!(b
        .run_manufacturing_preflight()
        .errors
        .iter()
        .any(|issue| issue.rule == "manufacturing_unconnected_via"));
}

#[test]
fn assembly_uses_physical_placement_origin_mirroring_and_safe_csv() {
    let mut b = board();
    b.components[0].designator = "R\"1".into();
    b.components[0].value = "=unsafe".into();
    b.components[0].rotation = 90.0;
    b.components[0].layer = PcbLayer::BottomCopper;
    let files = raf_electronics::pcb::assembly::export_assembly_files(&b).unwrap();
    assert!(files["BOM.csv"].contains("\"'=unsafe\""));
    assert!(files["BOM.csv"].contains("\"R\"\"1\""));
    assert!(files["CPL.csv"].contains("5.0000,-10.0000,Bottom,270.000"));
    assert!(files["BOM.csv"].contains("LCSC Part #"));
}

#[test]
fn stale_via_is_preserved_without_joining_a_reused_net_name() {
    let mut board = board();
    let id = board
        .try_add_via("LINK", Vec2::new(10.0, 10.0), 0.7, 0.3)
        .unwrap();
    let mut replacement = Schematic::new("different pin identities");
    for x in [0.0, 200.0] {
        let mut component = ElectronicComponent::resistor("1k");
        component.position.x = x;
        replacement.add_component(component);
    }
    connect(&mut replacement, (0, 1), (1, 0), "LINK");
    board.sync_from_schematic(&replacement);
    assert_eq!(board.vias[0].id, id);
    assert!(board.vias[0].orphaned);
    assert!(board
        .run_drc()
        .errors
        .iter()
        .any(|issue| issue.rule == "invalid_via"));
}

#[test]
fn five_led_parallel_circuit_passes_erc_and_electrical_balance() {
    let mut s = Schematic::new("five LED acceptance fixture");
    let mut supply = ElectronicComponent::dc_source(9.0);
    supply.position = Vec2::new(-300.0, -200.0);
    s.add_component(supply);
    for i in 0..5 {
        let mut r = ElectronicComponent::resistor("1k");
        r.position = Vec2::new(0.0, i as f32 * 200.0);
        s.add_component(r);
        let mut led = ElectronicComponent::led();
        led.position = Vec2::new(300.0, i as f32 * 200.0 + 80.0);
        s.add_component(led);
        let ri = 1 + i * 2;
        let di = ri + 1;
        connect(&mut s, (0, 0), (ri, 0), "VCC");
        connect(&mut s, (ri, 1), (di, 0), &format!("LED_{i}"));
        connect(&mut s, (di, 1), (0, 1), "RETURN");
    }
    let erc = s.run_drc();
    assert!(erc.passed(), "{:?}", erc);
    let result = s.simulate_dc();
    assert!(result.converged, "{:?}", result.messages);
    let nets = s.netlist();
    assert_eq!(
        result.node_voltages[&nets.net_for_pin(0, 1).unwrap().id],
        0.0
    );
    assert!(result.node_voltages[&nets.net_for_pin(0, 0).unwrap().id] > 8.99);
    for i in 0..5 {
        let current = result.component_currents[&(2 + i * 2)];
        assert!(current > 0.006 && current < 0.007, "LED current {current}");
    }
    assert!(result.max_kcl_residual_amps < 1e-8);
    assert!(result.power_balance_watts.abs() < 1e-8);
}
