use glam::Vec2;
use raf_electronics::schematic::component_pin_world_position;
use raf_electronics::{
    export_bom_csv, export_netlist_text, BoardOutline, ElectronicComponent, PcbLayer,
};
use uuid::Uuid;

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::panels::pcb_view::PcbViewPanel;
use crate::panels::schematic_view::SchematicViewPanel;

pub struct ElectronicsCommandContext<'a> {
    pub schematic_view: &'a mut SchematicViewPanel,
    pub pcb_view: &'a mut PcbViewPanel,
}

pub fn execute(
    command_name: &str,
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    match command_name {
        "electronics.add_part" => add_part(command, ctx),
        "electronics.wire" => add_wire(command, ctx),
        "electronics.set_value" => set_value(command, ctx),
        "electronics.rotate" => rotate_component(command, ctx),
        "electronics.delete" => delete_component(command, ctx),
        "electronics.select" => select_component(command, ctx),
        "electronics.generate_circuit" => generate_circuit(command, ctx),
        "electronics.autolayout" => autolayout(command, ctx),
        "electronics.drc" => run_drc(ctx),
        "electronics.simulate" => simulate(ctx),
        "electronics.netlist" => netlist(ctx),
        "electronics.bom" => bom(ctx),
        "electronics.describe" => describe_schematic(ctx),
        "pcb.sync" => pcb_sync(ctx),
        "pcb.route_airwire" => pcb_route_airwire(command, ctx),
        "pcb.set_board" => pcb_set_board(command, ctx),
        "pcb.move" => pcb_move(command, ctx),
        "pcb.rotate" => pcb_rotate(command, ctx),
        "pcb.describe" => pcb_describe(ctx),
        _ => CommandOutput::error(
            "Electronics command",
            format!("Unknown electronics command: {command_name}"),
        ),
    }
}

fn add_part(command: &ParsedCommand, ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let kind = command
        .arg("kind")
        .or_else(|| command.arg("type"))
        .or_else(|| command.first_positional())
        .unwrap_or("resistor");
    let value = command.arg("value");
    let mut component = component_for_kind(kind, value);
    component.position = Vec2::new(f32_arg(command, "x", 0.0), f32_arg(command, "y", 0.0));
    component.rotation = f32_arg(command, "rotation", f32_arg(command, "degrees", 0.0));

    ctx.schematic_view.schematic.add_component(component);
    let index = ctx
        .schematic_view
        .schematic
        .components
        .len()
        .saturating_sub(1);
    ctx.schematic_view.select_component(index);
    let component = &ctx.schematic_view.schematic.components[index];
    CommandOutput::changed(
        format!("Placed {}", component.designator),
        component_lines(index, component),
        component_json(index, component),
    )
}

fn add_wire(command: &ParsedCommand, ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let Some(x1) = required_f32(command, "x1") else {
        return CommandOutput::error("Add wire", "Missing x1.");
    };
    let Some(y1) = required_f32(command, "y1") else {
        return CommandOutput::error("Add wire", "Missing y1.");
    };
    let Some(x2) = required_f32(command, "x2") else {
        return CommandOutput::error("Add wire", "Missing x2.");
    };
    let Some(y2) = required_f32(command, "y2") else {
        return CommandOutput::error("Add wire", "Missing y2.");
    };
    let net = command.arg("net").unwrap_or("");
    let id = ctx
        .schematic_view
        .schematic
        .add_wire(Vec2::new(x1, y1), Vec2::new(x2, y2), net);
    let index = ctx.schematic_view.schematic.wires.len().saturating_sub(1);
    ctx.schematic_view.select_wire(index);
    let wire = &ctx.schematic_view.schematic.wires[index];
    CommandOutput::changed(
        format!("Added wire {}", id),
        vec![
            format!("wire_id: {}", wire.id),
            format_vec2("start", wire.start),
            format_vec2("end", wire.end),
            format!("net: {}", wire.net),
            format!("length: {:.3}", wire.start.distance(wire.end)),
        ],
        serde_json::json!({
            "ok": true,
            "wire": {
                "id": wire.id.to_string(),
                "start": [wire.start.x, wire.start.y],
                "end": [wire.end.x, wire.end.y],
                "net": wire.net,
                "length": wire.start.distance(wire.end)
            }
        }),
    )
}

fn set_value(command: &ParsedCommand, ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let Some(index) = resolve_component(command, ctx) else {
        return CommandOutput::error("Set value", "Component target not found.");
    };
    let Some(value) = command.arg("value") else {
        return CommandOutput::error("Set value", "Missing value=<new value>.");
    };
    if let Some(component) = ctx.schematic_view.schematic.components.get_mut(index) {
        component.value = value.to_string();
        component.sync_sim_model_from_value();
    }
    ctx.schematic_view.select_component(index);
    let component = &ctx.schematic_view.schematic.components[index];
    CommandOutput::changed(
        format!("Updated {}", component.designator),
        component_lines(index, component),
        component_json(index, component),
    )
}

fn rotate_component(
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    let Some(index) = resolve_component(command, ctx) else {
        return CommandOutput::error("Rotate component", "Component target not found.");
    };
    let degrees = f32_arg(command, "degrees", 90.0);
    if let Some(component) = ctx.schematic_view.schematic.components.get_mut(index) {
        component.rotation = (component.rotation + degrees).rem_euclid(360.0);
    }
    ctx.schematic_view.schematic.sync_wire_anchors();
    ctx.schematic_view.select_component(index);
    let component = &ctx.schematic_view.schematic.components[index];
    CommandOutput::changed(
        format!("Rotated {}", component.designator),
        component_lines(index, component),
        component_json(index, component),
    )
}

fn delete_component(
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    let Some(index) = resolve_component(command, ctx) else {
        return CommandOutput::error("Delete component", "Component target not found.");
    };
    let component = ctx.schematic_view.schematic.components.remove(index);
    ctx.schematic_view.clear_selection();
    CommandOutput::changed(
        format!("Deleted {}", component.designator),
        vec![
            format!("removed_index: {index}"),
            format!("removed_id: {}", component.id),
            format!("removed_designator: {}", component.designator),
        ],
        serde_json::json!({
            "ok": true,
            "removed": {
                "index": index,
                "id": component.id.to_string(),
                "designator": component.designator
            }
        }),
    )
}

fn select_component(
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    let Some(index) = resolve_component(command, ctx) else {
        return CommandOutput::error("Select component", "Component target not found.");
    };
    ctx.schematic_view.select_component(index);
    let component = &ctx.schematic_view.schematic.components[index];
    CommandOutput::info(
        format!("Selected {}", component.designator),
        component_lines(index, component),
        component_json(index, component),
    )
}

fn generate_circuit(
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    let kind = command.arg("kind").unwrap_or("led").to_ascii_lowercase();
    if kind != "led" {
        return CommandOutput::warning(
            "Generate circuit",
            vec![
                format!("Unsupported circuit kind: {kind}"),
                "Generated circuit kinds available now: led".to_string(),
            ],
            serde_json::json!({"ok": false, "supported": ["led"]}),
        );
    }

    let start = ctx.schematic_view.schematic.components.len();
    let mut battery = ElectronicComponent::dc_source(9.0);
    battery.position = Vec2::new(0.0, 0.0);
    let mut resistor = ElectronicComponent::resistor("1k");
    resistor.position = Vec2::new(100.0, -40.0);
    let mut led = ElectronicComponent::led();
    led.position = Vec2::new(200.0, -40.0);
    let mut ground = ElectronicComponent::ground();
    ground.position = Vec2::new(200.0, 60.0);

    ctx.schematic_view.schematic.add_component(battery);
    ctx.schematic_view.schematic.add_component(resistor);
    ctx.schematic_view.schematic.add_component(led);
    ctx.schematic_view.schematic.add_component(ground);

    let components = &ctx.schematic_view.schematic.components;
    let v = &components[start];
    let r = &components[start + 1];
    let d = &components[start + 2];
    let g = &components[start + 3];
    let v_plus = component_pin_world_position(v, &v.pins[0]);
    let v_minus = component_pin_world_position(v, &v.pins[1]);
    let r_a = component_pin_world_position(r, &r.pins[0]);
    let r_b = component_pin_world_position(r, &r.pins[1]);
    let d_a = component_pin_world_position(d, &d.pins[0]);
    let d_k = component_pin_world_position(d, &d.pins[1]);
    let g_pin = component_pin_world_position(g, &g.pins[0]);

    ctx.schematic_view.schematic.add_wire(v_plus, r_a, "VCC");
    ctx.schematic_view.schematic.add_wire(r_b, d_a, "LED_A");
    ctx.schematic_view.schematic.add_wire(d_k, g_pin, "GND");
    ctx.schematic_view.schematic.add_wire(v_minus, g_pin, "GND");

    ctx.schematic_view.select_component(start);
    CommandOutput::changed(
        "Generated LED circuit",
        vec![
            "created_components: 4".to_string(),
            "created_wires: 4".to_string(),
            "topology: Battery positive -> resistor -> LED -> ground; battery negative -> ground"
                .to_string(),
            format!(
                "battery: {}",
                ctx.schematic_view.schematic.components[start].designator
            ),
            format!(
                "resistor: {}",
                ctx.schematic_view.schematic.components[start + 1].designator
            ),
            format!(
                "led: {}",
                ctx.schematic_view.schematic.components[start + 2].designator
            ),
            format!(
                "ground: {}",
                ctx.schematic_view.schematic.components[start + 3].designator
            ),
        ],
        serde_json::json!({
            "ok": true,
            "kind": "led",
            "components_created": 4,
            "wires_created": 4
        }),
    )
}

fn autolayout(command: &ParsedCommand, ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let spacing = f32_arg(command, "spacing", 80.0).max(20.0);
    let columns = f32_arg(command, "columns", 4.0).max(1.0) as usize;
    for (index, component) in ctx
        .schematic_view
        .schematic
        .components
        .iter_mut()
        .enumerate()
    {
        let col = index % columns;
        let row = index / columns;
        component.position = Vec2::new(col as f32 * spacing, row as f32 * spacing);
    }
    ctx.schematic_view.schematic.sync_wire_anchors();
    CommandOutput::changed(
        "Schematic autolayout",
        vec![
            format!(
                "components: {}",
                ctx.schematic_view.schematic.components.len()
            ),
            format!("spacing: {spacing:.3}"),
            format!("columns: {columns}"),
        ],
        serde_json::json!({
            "ok": true,
            "components": ctx.schematic_view.schematic.components.len(),
            "spacing": spacing,
            "columns": columns
        }),
    )
}

fn run_drc(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let report = ctx.schematic_view.schematic.run_drc();
    let mut lines = vec![
        format!("passed: {}", report.passed()),
        format!("errors: {}", report.errors.len()),
        format!("warnings: {}", report.warnings.len()),
        format!("info: {}", report.info.len()),
    ];
    for issue in report.all_issues().iter().take(24) {
        lines.push(format!(
            "{:?} {}: {}",
            issue.severity, issue.rule, issue.message
        ));
    }
    CommandOutput::info(
        "DRC report",
        lines,
        serde_json::json!({
            "ok": true,
            "passed": report.passed(),
            "errors": report.errors.len(),
            "warnings": report.warnings.len(),
            "info": report.info.len()
        }),
    )
}

fn simulate(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let results = ctx.schematic_view.schematic.simulate_dc();
    let mut lines = vec![format!("converged: {}", results.converged)];
    for message in &results.messages {
        lines.push(format!("message: {message}"));
    }
    for (net, voltage) in &results.node_voltages {
        lines.push(format!("net N{net:03}: {:.5} V", voltage));
    }
    for (component_index, current) in &results.component_currents {
        if let Some(component) = ctx
            .schematic_view
            .schematic
            .components
            .get(*component_index)
        {
            lines.push(format!(
                "{} current: {:.8} A",
                component.designator, current
            ));
        }
    }
    CommandOutput::info(
        "DC simulation",
        lines,
        serde_json::json!({
            "ok": true,
            "converged": results.converged,
            "node_voltages": results.node_voltages,
            "component_currents": results.component_currents,
            "component_power": results.component_power,
            "messages": results.messages
        }),
    )
}

fn netlist(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let export = export_netlist_text(&ctx.schematic_view.schematic);
    let preview = export
        .content
        .lines()
        .take(28)
        .map(str::to_string)
        .collect::<Vec<_>>();
    CommandOutput::info(
        "Netlist",
        preview,
        serde_json::json!({
            "ok": true,
            "extension": export.extension,
            "content": export.content
        }),
    )
}

fn bom(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let export = export_bom_csv(&ctx.schematic_view.schematic);
    let preview = export
        .content
        .lines()
        .take(28)
        .map(str::to_string)
        .collect::<Vec<_>>();
    CommandOutput::info(
        "BOM",
        preview,
        serde_json::json!({
            "ok": true,
            "extension": export.extension,
            "content": export.content
        }),
    )
}

fn describe_schematic(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let schematic = &ctx.schematic_view.schematic;
    let netlist = schematic.netlist();
    let mut lines = vec![
        format!("name: {}", schematic.name),
        format!("components: {}", schematic.components.len()),
        format!("wires: {}", schematic.wires.len()),
        format!("nets: {}", netlist.nets.len()),
    ];
    for (index, component) in schematic.components.iter().enumerate().take(20) {
        lines.push(format!(
            "#{index} {} {} at [{:.3}, {:.3}] rot {:.1}",
            component.designator,
            component.value,
            component.position.x,
            component.position.y,
            component.rotation
        ));
    }
    CommandOutput::info(
        "Schematic description",
        lines,
        serde_json::json!({
            "ok": true,
            "components": schematic.components.len(),
            "wires": schematic.wires.len(),
            "nets": netlist.nets.len()
        }),
    )
}

fn pcb_sync(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let summary = ctx
        .pcb_view
        .sync_from_schematic(&ctx.schematic_view.schematic);
    CommandOutput::changed(
        "PCB synced from schematic",
        vec![
            format!("added_components: {}", summary.added_components),
            format!("updated_components: {}", summary.updated_components),
            format!("removed_components: {}", summary.removed_components),
            format!("nets: {}", summary.nets),
        ],
        serde_json::json!({
            "ok": true,
            "added_components": summary.added_components,
            "updated_components": summary.updated_components,
            "removed_components": summary.removed_components,
            "nets": summary.nets
        }),
    )
}

fn pcb_route_airwire(
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    let index = usize_arg(command, "index", 0);
    if !ctx.pcb_view.layout.route_airwire(index) {
        return CommandOutput::error("Route airwire", "Airwire index not found.");
    }
    ctx.pcb_view
        .select_trace(ctx.pcb_view.layout.traces.len().saturating_sub(1));
    let trace = ctx.pcb_view.layout.traces.last().expect("trace exists");
    CommandOutput::changed(
        format!("Routed airwire {index}"),
        trace_lines(trace),
        serde_json::json!({
            "ok": true,
            "trace_id": trace.id.to_string(),
            "net": trace.net,
            "points": trace.points.iter().map(|p| vec![p.x, p.y]).collect::<Vec<_>>()
        }),
    )
}

fn pcb_set_board(
    command: &ParsedCommand,
    ctx: &mut ElectronicsCommandContext<'_>,
) -> CommandOutput {
    let width = f32_arg(command, "width", 420.0).max(1.0);
    let height = f32_arg(command, "height", 280.0).max(1.0);
    ctx.pcb_view.layout.board_outline = BoardOutline::default_rect(width, height);
    CommandOutput::changed(
        "PCB board outline updated",
        vec![
            format!("width: {width:.3}"),
            format!("height: {height:.3}"),
            "shape: rectangle".to_string(),
            "outline_points: 5".to_string(),
        ],
        serde_json::json!({
            "ok": true,
            "board": {
                "width": width,
                "height": height,
                "points": ctx.pcb_view.layout.board_outline.points.iter().map(|p| vec![p.x, p.y]).collect::<Vec<_>>()
            }
        }),
    )
}

fn pcb_move(command: &ParsedCommand, ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let Some(index) = resolve_pcb_component(command, ctx) else {
        return CommandOutput::error("PCB move", "PCB component target not found.");
    };
    if let Some(component) = ctx.pcb_view.layout.components.get_mut(index) {
        if let Some(x) = command.arg("x").and_then(parse_f32) {
            component.position.x = x;
        }
        if let Some(y) = command.arg("y").and_then(parse_f32) {
            component.position.y = y;
        }
        component.position += Vec2::new(f32_arg(command, "dx", 0.0), f32_arg(command, "dy", 0.0));
    }
    ctx.pcb_view.layout.rebuild_airwires();
    ctx.pcb_view.select_component(index);
    let component = &ctx.pcb_view.layout.components[index];
    CommandOutput::changed(
        format!("Moved PCB {}", component.designator),
        pcb_component_lines(index, component),
        pcb_component_json(index, component),
    )
}

fn pcb_rotate(command: &ParsedCommand, ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let Some(index) = resolve_pcb_component(command, ctx) else {
        return CommandOutput::error("PCB rotate", "PCB component target not found.");
    };
    let degrees = f32_arg(command, "degrees", 90.0);
    if let Some(component) = ctx.pcb_view.layout.components.get_mut(index) {
        component.rotation = (component.rotation + degrees).rem_euclid(360.0);
    }
    ctx.pcb_view.layout.rebuild_airwires();
    ctx.pcb_view.select_component(index);
    let component = &ctx.pcb_view.layout.components[index];
    CommandOutput::changed(
        format!("Rotated PCB {}", component.designator),
        pcb_component_lines(index, component),
        pcb_component_json(index, component),
    )
}

fn pcb_describe(ctx: &mut ElectronicsCommandContext<'_>) -> CommandOutput {
    let layout = &ctx.pcb_view.layout;
    let board_size = layout.board_size();
    CommandOutput::info(
        "PCB description",
        vec![
            format!("name: {}", layout.name),
            format!("board_size: [{:.3}, {:.3}]", board_size.x, board_size.y),
            format!("outline_closed: {}", layout.outline_is_closed()),
            format!("components: {}", layout.components.len()),
            format!("traces: {}", layout.traces.len()),
            format!("airwires: {}", layout.airwires.len()),
            format!("missing_footprints: {}", layout.missing_footprints()),
        ],
        serde_json::json!({
            "ok": true,
            "components": layout.components.len(),
            "traces": layout.traces.len(),
            "airwires": layout.airwires.len(),
            "board_size": [board_size.x, board_size.y]
        }),
    )
}

fn component_for_kind(kind: &str, value: Option<&str>) -> ElectronicComponent {
    match kind.to_ascii_lowercase().as_str() {
        "capacitor" | "cap" => ElectronicComponent::capacitor(value.unwrap_or("100nF")),
        "led" | "diode" => ElectronicComponent::led(),
        "magnet" => ElectronicComponent::magnet(value.unwrap_or("0.5T")),
        "battery" | "source" | "dc" => {
            ElectronicComponent::dc_source(value.and_then(parse_f64).unwrap_or(9.0))
        }
        "ground" | "gnd" => ElectronicComponent::ground(),
        _ => ElectronicComponent::resistor(value.unwrap_or("10k")),
    }
}

fn resolve_component(
    command: &ParsedCommand,
    ctx: &ElectronicsCommandContext<'_>,
) -> Option<usize> {
    let target = command
        .arg("target")
        .or_else(|| command.arg("designator"))
        .map(str::to_string)
        .or_else(|| {
            if command.positional.is_empty() {
                None
            } else {
                Some(command.positional.join(" "))
            }
        })?;
    if let Ok(index) = target.parse::<usize>() {
        if index < ctx.schematic_view.schematic.components.len() {
            return Some(index);
        }
    }
    if let Ok(uuid) = Uuid::parse_str(&target) {
        return ctx
            .schematic_view
            .schematic
            .components
            .iter()
            .position(|component| component.id == uuid);
    }
    let lower = target.to_ascii_lowercase();
    ctx.schematic_view
        .schematic
        .components
        .iter()
        .position(|component| {
            component.designator.eq_ignore_ascii_case(&target)
                || component.value.to_ascii_lowercase().contains(&lower)
                || component.kind_label().to_ascii_lowercase().contains(&lower)
        })
}

fn resolve_pcb_component(
    command: &ParsedCommand,
    ctx: &ElectronicsCommandContext<'_>,
) -> Option<usize> {
    let target = command
        .arg("target")
        .or_else(|| command.arg("designator"))
        .map(str::to_string)
        .or_else(|| {
            if command.positional.is_empty() {
                None
            } else {
                Some(command.positional.join(" "))
            }
        })?;
    if let Ok(index) = target.parse::<usize>() {
        if index < ctx.pcb_view.layout.components.len() {
            return Some(index);
        }
    }
    let lower = target.to_ascii_lowercase();
    ctx.pcb_view.layout.components.iter().position(|component| {
        component.designator.eq_ignore_ascii_case(&target)
            || component.value.to_ascii_lowercase().contains(&lower)
            || component.footprint.to_ascii_lowercase().contains(&lower)
    })
}

fn component_lines(index: usize, component: &ElectronicComponent) -> Vec<String> {
    let mut lines = vec![
        format!("index: {index}"),
        format!("id: {}", component.id),
        format!("designator: {}", component.designator),
        format!("kind: {}", component.kind_label()),
        format!("value: {}", component.value),
        format!("footprint: {}", component.footprint),
        format_vec2("position", component.position),
        format!("rotation_deg: {:.3}", component.rotation),
        format!("pins: {}", component.pins.len()),
    ];
    for (pin_index, pin) in component.pins.iter().enumerate() {
        let world = component_pin_world_position(component, pin);
        lines.push(format!(
            "pin_{pin_index}: {} {:?} local=[{:.3}, {:.3}] world=[{:.3}, {:.3}] net={}",
            pin.name, pin.direction, pin.offset.x, pin.offset.y, world.x, world.y, pin.net
        ));
    }
    lines
}

fn component_json(index: usize, component: &ElectronicComponent) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "component": {
            "index": index,
            "id": component.id.to_string(),
            "designator": component.designator,
            "kind": component.kind_label(),
            "value": component.value,
            "footprint": component.footprint,
            "position": [component.position.x, component.position.y],
            "rotation_deg": component.rotation,
            "pins": component.pins.iter().map(|pin| {
                let world = component_pin_world_position(component, pin);
                serde_json::json!({
                    "id": pin.id.to_string(),
                    "name": pin.name,
                    "direction": format!("{:?}", pin.direction),
                    "offset": [pin.offset.x, pin.offset.y],
                    "world": [world.x, world.y],
                    "net": pin.net
                })
            }).collect::<Vec<_>>()
        }
    })
}

fn pcb_component_lines(
    index: usize,
    component: &raf_electronics::PcbComponentPlacement,
) -> Vec<String> {
    vec![
        format!("index: {index}"),
        format!("component_id: {}", component.component_id),
        format!("designator: {}", component.designator),
        format!("value: {}", component.value),
        format!("footprint: {}", component.footprint),
        format_vec2("position", component.position),
        format!("rotation_deg: {:.3}", component.rotation),
        format!("layer: {}", layer_name(component.layer)),
        format!("locked: {}", component.locked),
        format!("pad_nets: {}", component.pad_nets.join(", ")),
    ]
}

fn pcb_component_json(
    index: usize,
    component: &raf_electronics::PcbComponentPlacement,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "pcb_component": {
            "index": index,
            "component_id": component.component_id.to_string(),
            "designator": component.designator,
            "value": component.value,
            "footprint": component.footprint,
            "position": [component.position.x, component.position.y],
            "rotation_deg": component.rotation,
            "layer": layer_name(component.layer),
            "locked": component.locked,
            "pad_nets": component.pad_nets
        }
    })
}

fn trace_lines(trace: &raf_electronics::PcbTrace) -> Vec<String> {
    let mut lines = vec![
        format!("trace_id: {}", trace.id),
        format!("net: {}", trace.net),
        format!("layer: {}", layer_name(trace.layer)),
        format!("width: {:.3}", trace.width),
        format!("points: {}", trace.points.len()),
    ];
    for (index, point) in trace.points.iter().enumerate() {
        lines.push(format!("point_{index}: [{:.3}, {:.3}]", point.x, point.y));
    }
    lines
}

fn layer_name(layer: PcbLayer) -> &'static str {
    match layer {
        PcbLayer::TopCopper => "TopCopper",
        PcbLayer::BottomCopper => "BottomCopper",
    }
}

fn required_f32(command: &ParsedCommand, name: &str) -> Option<f32> {
    command.arg(name).and_then(parse_f32)
}

fn f32_arg(command: &ParsedCommand, name: &str, default: f32) -> f32 {
    command.arg(name).and_then(parse_f32).unwrap_or(default)
}

fn usize_arg(command: &ParsedCommand, name: &str, default: usize) -> usize {
    command
        .arg(name)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_f32(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok()
}

fn parse_f64(value: &str) -> Option<f64> {
    value
        .trim()
        .trim_end_matches(['V', 'v'])
        .parse::<f64>()
        .ok()
}

fn format_vec2(label: &str, value: Vec2) -> String {
    format!("{label}: [{:.3}, {:.3}]", value.x, value.y)
}
