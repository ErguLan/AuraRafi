use std::collections::HashMap;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::component::{ElectronicComponent, SimModel};
use crate::drc::{DrcIssue, DrcReport, DrcSeverity};
use crate::netlist::Netlist;
use crate::pcb::geometry::{transform_footprint_point, transform_footprint_rotation};
use crate::pcb::{FootprintDrillShape, FootprintLayer, FootprintPadShape, PcbLayer, PcbLayout};
use crate::schematic::{component_pin_world_position, Schematic};
#[path = "cad_footprint_graphics.rs"]
mod footprint_graphics;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadSurfaceKind {
    Schematic,
    Pcb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadObjectKind {
    Component,
    Pin,
    Wire,
    Trace,
    Pad,
    Airwire,
    NetLabel,
    DrcMarker,
    BoardOutline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadLayerKind {
    Schematic,
    PcbTopCopper,
    PcbBottomCopper,
    PcbSilkscreen,
    Airwire,
    Overlay,
    Drc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CadPickPriority {
    Background = 0,
    Copper = 10,
    Wire = 20,
    Component = 30,
    Pin = 40,
    Overlay = 50,
    Drc = 60,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CadRect {
    pub center: Vec2,
    pub size: Vec2,
}

impl CadRect {
    pub fn new(center: Vec2, size: Vec2) -> Self {
        Self { center, size }
    }

    pub fn contains(&self, point: Vec2) -> bool {
        let half = self.size * 0.5;
        point.x >= self.center.x - half.x
            && point.x <= self.center.x + half.x
            && point.y >= self.center.y - half.y
            && point.y <= self.center.y + half.y
    }
}

/// Physical outline used by a CAD object. `CadRect::size` stays the nominal
/// extent, while this value carries the shape that copper and picking share.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CadShape {
    Rectangle,
    RoundRect {
        corner_radius: f32,
    },
    /// Capsule with semicircular ends; a square oval is a circle.
    Oval,
    Circle,
}

impl Default for CadShape {
    fn default() -> Self {
        Self::Rectangle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadDrillShape {
    Circle,
    Oval,
}

/// Bore carried by a pad or other drilled CAD object. Its size, offset and
/// orientation are expressed in the pad's local coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CadDrill {
    pub shape: CadDrillShape,
    pub size: Vec2,
    pub offset: Vec2,
    #[serde(default)]
    pub plated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadObject {
    pub id: String,
    pub source_id: Option<Uuid>,
    pub kind: CadObjectKind,
    pub layer: CadLayerKind,
    pub pick_priority: CadPickPriority,
    pub rect: Option<CadRect>,
    /// Defaults keep previously serialized scenes axis-aligned rectangles.
    #[serde(default)]
    pub shape: CadShape,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default)]
    pub drill: Option<CadDrill>,
    /// Physical conductor/stroke width in board units; absent on legacy marks.
    #[serde(default)]
    pub stroke_width_world: Option<f32>,
    pub points: Vec<Vec2>,
    /// Additional independent line paths used by pin markers and CAD traces.
    ///
    /// `points` remains the canonical polyline for wires/traces. Symbol paths
    /// are kept separate so adjacent strokes are never joined accidentally
    /// when the renderer batches them.
    #[serde(default)]
    pub line_paths: Vec<Vec<Vec2>>,
    pub label: Option<String>,
    pub net: Option<String>,
    /// Stable connectivity identity used for cross-probing and net highlighting.
    #[serde(default)]
    pub net_id: Option<usize>,
    pub color_rgba: [u8; 4],
}

impl CadObject {
    fn rect(
        id: impl Into<String>,
        source_id: Option<Uuid>,
        kind: CadObjectKind,
        layer: CadLayerKind,
        priority: CadPickPriority,
        rect: CadRect,
        color_rgba: [u8; 4],
    ) -> Self {
        Self {
            id: id.into(),
            source_id,
            kind,
            layer,
            pick_priority: priority,
            rect: Some(rect),
            shape: CadShape::Rectangle,
            rotation_degrees: 0.0,
            drill: None,
            stroke_width_world: None,
            points: Vec::new(),
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba,
        }
    }

    fn polyline(
        id: impl Into<String>,
        source_id: Option<Uuid>,
        kind: CadObjectKind,
        layer: CadLayerKind,
        priority: CadPickPriority,
        points: Vec<Vec2>,
        color_rgba: [u8; 4],
    ) -> Self {
        Self {
            id: id.into(),
            source_id,
            kind,
            layer,
            pick_priority: priority,
            rect: None,
            shape: CadShape::Rectangle,
            rotation_degrees: 0.0,
            drill: None,
            stroke_width_world: None,
            points,
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadScene {
    pub surface: CadSurfaceKind,
    pub objects: Vec<CadObject>,
}

impl CadScene {
    /// Stable structural fingerprint for retained rendering caches.
    ///
    /// The value covers every field that affects CAD geometry, picking, or
    /// presentation. It is intentionally independent from pointer addresses
    /// so CPU and GPU hosts can reuse the same cache policy.
    pub fn stable_fingerprint(&self) -> u64 {
        let mut hash = CadFingerprint::new();
        hash.write_u8(self.surface as u8);
        hash.write_usize(self.objects.len());

        for object in &self.objects {
            hash.write_str(&object.id);
            match object.source_id {
                Some(id) => {
                    hash.write_u8(1);
                    hash.write_bytes(id.as_bytes());
                }
                None => hash.write_u8(0),
            }
            hash.write_u8(object.kind as u8);
            hash.write_u8(object.layer as u8);
            hash.write_u8(object.pick_priority as u8);
            match object.rect {
                Some(rect) => {
                    hash.write_u8(1);
                    hash.write_vec2(rect.center);
                    hash.write_vec2(rect.size);
                }
                None => hash.write_u8(0),
            }
            match object.shape {
                CadShape::Rectangle => hash.write_u8(0),
                CadShape::RoundRect { corner_radius } => {
                    hash.write_u8(1);
                    hash.write_f32(corner_radius);
                }
                CadShape::Oval => hash.write_u8(2),
                CadShape::Circle => hash.write_u8(3),
            }
            hash.write_f32(object.rotation_degrees);
            if let Some(width) = object.stroke_width_world {
                hash.write_u8(1);
                hash.write_f32(width);
            } else {
                hash.write_u8(0);
            }
            if let Some(drill) = object.drill {
                hash.write_u8(1);
                hash.write_u8(match drill.shape {
                    CadDrillShape::Circle => 0,
                    CadDrillShape::Oval => 1,
                });
                hash.write_vec2(drill.size);
                hash.write_vec2(drill.offset);
                hash.write_u8(u8::from(drill.plated));
            } else {
                hash.write_u8(0);
            }
            hash.write_usize(object.points.len());
            for point in &object.points {
                hash.write_vec2(*point);
            }
            hash.write_usize(object.line_paths.len());
            for path in &object.line_paths {
                hash.write_usize(path.len());
                for point in path {
                    hash.write_vec2(*point);
                }
            }
            hash.write_option_str(object.label.as_deref());
            hash.write_option_str(object.net.as_deref());
            match object.net_id {
                Some(id) => {
                    hash.write_u8(1);
                    hash.write_usize(id);
                }
                None => hash.write_u8(0),
            }
            hash.write_bytes(&object.color_rgba);
        }

        hash.finish()
    }

    pub fn from_schematic(schematic: &Schematic) -> Self {
        let mut scene = Self {
            surface: CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };

        // Connectivity is resolved exactly once per rebuild and shared by every
        // object of the document, so a wire, its label and the pins it touches
        // all report the same `net_id`.
        let netlist = schematic.netlist();
        let mut net_ids = NetIdentityMap::new();
        for net in &netlist.nets {
            net_ids.intern(&net.name);
        }
        for wire in &schematic.wires {
            net_ids.intern(&wire.net);
        }
        let pin_net_ids = pin_net_ids(&netlist, &net_ids);

        for (component_index, component) in schematic.components.iter().enumerate() {
            push_schematic_component(
                &mut scene.objects,
                component_index,
                component,
                &net_ids,
                &pin_net_ids,
            );
        }

        for wire in &schematic.wires {
            let net_name = netlist
                .net_for_wire(wire.id)
                .map(|net| net.name.as_str())
                .unwrap_or(&wire.net);
            let net_id = net_ids.get(net_name);
            let mut object = CadObject::polyline(
                format!("wire:{}", wire.id),
                Some(wire.id),
                CadObjectKind::Wire,
                CadLayerKind::Schematic,
                CadPickPriority::Wire,
                orthogonal_wire_points(wire.start, wire.end),
                [216, 221, 227, 255],
            );
            object.net = Some(net_name.to_string());
            object.net_id = net_id;
            scene.objects.push(object);

            if !wire.net.trim().is_empty() {
                let mut label = CadObject::rect(
                    format!("net_label:{}", wire.id),
                    Some(wire.id),
                    CadObjectKind::NetLabel,
                    CadLayerKind::Overlay,
                    CadPickPriority::Overlay,
                    CadRect::new((wire.start + wire.end) * 0.5, Vec2::new(48.0, 16.0)),
                    [245, 245, 246, 220],
                );
                label.label = Some(net_name.to_string());
                label.net = Some(net_name.to_string());
                label.net_id = net_id;
                scene.objects.push(label);
            }
        }

        scene
    }

    pub fn from_schematic_with_drc(schematic: &Schematic, report: &DrcReport) -> Self {
        let mut scene = Self::from_schematic(schematic);
        scene.push_drc_report(report);
        scene
    }

    pub fn from_pcb(layout: &PcbLayout) -> Self {
        let mut scene = Self {
            surface: CadSurfaceKind::Pcb,
            objects: Vec::new(),
        };

        let net_ids = pcb_net_ids(layout);

        scene.objects.push(CadObject::polyline(
            "board_outline",
            None,
            CadObjectKind::BoardOutline,
            CadLayerKind::PcbSilkscreen,
            CadPickPriority::Background,
            layout.board_outline.points.clone(),
            [54, 181, 110, 255],
        ));

        for component in &layout.components {
            let footprint = layout.resolved_footprint(component);
            let asset = layout.footprint_assets.get(&component.footprint);
            let body_center = asset
                .and_then(|asset| asset.bounds_mm())
                .map(|bounds| {
                    transform_footprint_point(
                        component,
                        crate::pcb::PointMm {
                            x: (bounds.min.x + bounds.max.x) * 0.5,
                            y: (bounds.min.y + bounds.max.y) * 0.5,
                        },
                        true,
                    )
                })
                .unwrap_or(component.position);
            let mut body = CadObject::rect(
                format!("pcb_component:{}", component.component_id),
                Some(component.component_id),
                CadObjectKind::Component,
                pcb_layer_to_cad(component.layer),
                CadPickPriority::Component,
                CadRect::new(body_center, footprint.body_size),
                [34, 34, 38, 255],
            );
            body.rotation_degrees = component.rotation;
            body.label = Some(format!("{} · {}", component.designator, component.value));
            scene.objects.push(body);
            if let Some(asset) = asset {
                for (index, graphic) in asset.graphics.iter().enumerate() {
                    if !matches!(
                        graphic.layer,
                        FootprintLayer::FrontSilkscreen | FootprintLayer::BackSilkscreen
                    ) {
                        continue;
                    }
                    if let Some((points, width)) =
                        footprint_graphics::graphic_path(component, &graphic.geometry)
                    {
                        let mut object = CadObject::polyline(
                            format!("pcb-silk:{}:{index}", component.component_id),
                            None,
                            CadObjectKind::BoardOutline,
                            CadLayerKind::PcbSilkscreen,
                            CadPickPriority::Background,
                            points,
                            [214, 221, 212, 230],
                        );
                        object.stroke_width_world = Some(width.max(0.01));
                        scene.objects.push(object);
                    }
                }
            }

            for (pad_index, pad) in footprint.pads.iter().enumerate() {
                let physical_pad = asset.and_then(|asset| asset.pads.get(pad_index));
                let center = physical_pad
                    .map(|physical| {
                        transform_footprint_point(component, physical.position_mm, true)
                    })
                    .unwrap_or_else(|| {
                        component.position + rotate_vec2(pad.offset, component.rotation)
                    });
                let pad_rotation = physical_pad
                    .map(|physical| {
                        transform_footprint_rotation(component, physical.rotation_degrees, true)
                    })
                    .unwrap_or(component.rotation);
                let copper_layer = physical_pad
                    .map(|physical| pad_copper_layer(&physical.layers, component.layer))
                    .unwrap_or(Some(component.layer));
                let copper_color = copper_layer.map(pcb_copper_color).unwrap_or([0, 0, 0, 0]);
                let net = component.pad_nets.get(pad_index).cloned();
                let net_id = net.as_deref().and_then(|net| net_ids.get(net));
                let mut object = CadObject::rect(
                    format!("pad:{}:{pad_index}", component.component_id),
                    Some(component.component_id),
                    CadObjectKind::Pad,
                    pcb_layer_to_cad(copper_layer.unwrap_or(component.layer)),
                    CadPickPriority::Pin,
                    CadRect::new(center, pad.size),
                    copper_color,
                );
                object.shape = physical_pad
                    .map(|physical| cad_pad_shape(&physical.shape, pad.size))
                    .unwrap_or(CadShape::Rectangle);
                object.rotation_degrees = pad_rotation;
                object.drill = physical_pad.and_then(|physical| {
                    physical.drill.as_ref().map(|drill| {
                        let (shape, size) = match &drill.shape {
                            FootprintDrillShape::Circular { diameter_mm } => {
                                (CadDrillShape::Circle, Vec2::splat(*diameter_mm as f32))
                            }
                            FootprintDrillShape::Oval {
                                width_mm,
                                height_mm,
                            } => (
                                CadDrillShape::Oval,
                                Vec2::new(*width_mm as f32, *height_mm as f32),
                            ),
                        };
                        let drill_angle = physical.rotation_degrees.to_radians();
                        let (drill_sin, drill_cos) = drill_angle.sin_cos();
                        let drill_offset_local = crate::pcb::PointMm {
                            x: drill.offset_mm.x * drill_cos - drill.offset_mm.y * drill_sin,
                            y: drill.offset_mm.x * drill_sin + drill.offset_mm.y * drill_cos,
                        };
                        let drill_center = transform_footprint_point(
                            component,
                            crate::pcb::PointMm {
                                x: physical.position_mm.x + drill_offset_local.x,
                                y: physical.position_mm.y + drill_offset_local.y,
                            },
                            true,
                        );
                        let drill_offset_world = drill_center - center;
                        let drill_offset = rotate_vec2(drill_offset_world, -pad_rotation);
                        CadDrill {
                            shape,
                            size,
                            offset: drill_offset,
                            plated: matches!(
                                &physical.technology,
                                crate::pcb::FootprintPadTechnology::PlatedThroughHole
                            ),
                        }
                    })
                });
                object.label = Some(pad.name.clone());
                object.net = net;
                object.net_id = net_id;
                scene.objects.push(object);
            }
        }

        for via in &layout.vias {
            if !via.position.is_finite() || !via.diameter.is_finite() || !via.drill.is_finite()
                || via.drill <= 0.0 || via.diameter <= via.drill { continue; }
            for layer in [PcbLayer::TopCopper, PcbLayer::BottomCopper] {
                let mut object = CadObject::rect(
                    format!("via:{}:{}", via.id, if layer == PcbLayer::TopCopper { "top" } else { "bottom" }),
                    Some(via.id), CadObjectKind::Pad, pcb_layer_to_cad(layer),
                    CadPickPriority::Pin, CadRect::new(via.position, Vec2::splat(via.diameter)),
                    pcb_copper_color(layer),
                );
                object.shape = CadShape::Circle;
                object.drill = Some(CadDrill { shape: CadDrillShape::Circle,
                    size: Vec2::splat(via.drill), offset: Vec2::ZERO, plated: true });
                object.net = Some(via.net.clone());
                object.net_id = net_ids.get(&via.net);
                scene.objects.push(object);
            }
        }
        for trace in &layout.traces {
            let mut object = CadObject::polyline(
                format!("trace:{}", trace.id),
                Some(trace.id),
                CadObjectKind::Trace,
                pcb_layer_to_cad(trace.layer),
                CadPickPriority::Copper,
                trace.points.clone(),
                pcb_copper_color(trace.layer),
            );
            object.net = Some(trace.net.clone());
            object.stroke_width_world = Some(trace.width);
            object.net_id = net_ids.get(&trace.net);
            scene.objects.push(object);
        }

        for (index, airwire) in layout.airwires.iter().enumerate() {
            let net_id = net_ids.get(&airwire.net);
            let mut object = CadObject::polyline(
                format!("airwire:{index}"),
                None,
                CadObjectKind::Airwire,
                CadLayerKind::Airwire,
                CadPickPriority::Overlay,
                vec![airwire.from, airwire.to],
                [217, 190, 133, 124],
            );
            object.net = Some(airwire.net.clone());
            object.net_id = net_id;
            scene.objects.push(object);
        }

        scene
    }

    pub fn push_drc_report(&mut self, report: &DrcReport) {
        for issue in report.all_issues() {
            self.push_drc_issue(issue);
        }
    }

    pub fn hit_test(&self, point: Vec2) -> Option<&CadObject> {
        self.objects
            .iter()
            .filter(|object| cad_object_contains(object, point))
            .max_by_key(|object| object.pick_priority)
    }

    fn push_drc_issue(&mut self, issue: &DrcIssue) {
        let Some(location) = issue.location else {
            return;
        };
        let mut object = CadObject::rect(
            format!("drc:{}:{}", issue.rule, self.objects.len()),
            issue.components.first().copied(),
            CadObjectKind::DrcMarker,
            CadLayerKind::Drc,
            CadPickPriority::Drc,
            CadRect::new(location, Vec2::new(16.0, 16.0)),
            drc_color(issue.severity),
        );
        object.label = Some(issue.message.clone());
        self.objects.push(object);
    }
}

struct CadFingerprint(u64);

impl CadFingerprint {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_bytes(&(value as u64).to_le_bytes());
    }

    fn write_vec2(&mut self, value: Vec2) {
        self.write_bytes(&value.x.to_bits().to_le_bytes());
        self.write_bytes(&value.y.to_bits().to_le_bytes());
    }

    fn write_f32(&mut self, value: f32) {
        self.write_bytes(&value.to_bits().to_le_bytes());
    }

    fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        self.write_bytes(value.as_bytes());
    }

    fn write_option_str(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_str(value);
            }
            None => self.write_u8(0),
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn push_schematic_component(
    objects: &mut Vec<CadObject>,
    component_index: usize,
    component: &ElectronicComponent,
    net_ids: &NetIdentityMap,
    pin_net_ids: &PinNetIds,
) {
    if !component.visible {
        return;
    }
    let body_size = schematic_component_body_size(component);
    let rotated_body_size = rotate_extent(body_size, component.rotation);
    let mut body = CadObject::rect(
        format!("component:{}", component.id),
        Some(component.id),
        CadObjectKind::Component,
        CadLayerKind::Schematic,
        CadPickPriority::Component,
        CadRect::new(component.position, rotated_body_size),
        component.appearance.color,
    );
    body.label = Some(format!("{} {}", component.designator, component.value));
    body.line_paths = schematic_component_symbol_paths(component);
    objects.push(body);

    for (pin_index, pin) in component.pins.iter().enumerate() {
        let pin_center = component_pin_world_position(component, pin);
        let pin_size = rotate_extent(Vec2::new(10.0, 10.0), component.rotation);
        let mut object = CadObject::rect(
            format!("pin:{}:{}", component.id, pin.id),
            Some(component.id),
            CadObjectKind::Pin,
            CadLayerKind::Schematic,
            CadPickPriority::Pin,
            CadRect::new(pin_center, pin_size),
            [112, 224, 136, 255],
        );
        object.line_paths = schematic_pin_paths(pin_center, component.rotation);
        object.label = Some(pin.name.clone());
        if !pin.net.trim().is_empty() {
            object.net = Some(pin.net.clone());
        }
        object.net_id = pin_net_ids
            .get(&(component_index, pin_index))
            .copied()
            .or_else(|| net_ids.get(&pin.net));
        objects.push(object);
    }
}

/// Maps every netlist pin to the stable identity assigned to its net.
///
/// The netlist already encodes "pin within tolerance of a wire endpoint", so its
/// membership is the authoritative answer for pins that no wire label names.
fn pin_net_ids(netlist: &Netlist, net_ids: &NetIdentityMap) -> PinNetIds {
    let mut map = PinNetIds::new();
    for net in &netlist.nets {
        let Some(net_id) = net_ids.get(&net.name) else {
            continue;
        };
        for &(component_index, pin_index) in &net.pins {
            map.insert((component_index, pin_index), net_id);
        }
    }
    map
}

/// Net name to stable identity map shared by every CAD surface builder.
type PinNetIds = HashMap<(usize, usize), usize>;

/// Deterministic net name to stable identity map.
///
/// Ids are handed out in document order (netlist order first, then the remaining
/// names in the order they appear) and never come from memory addresses or from
/// a counter that survives between calls, so rebuilding an identical document
/// always produces identical ids. Other layers can pin, highlight or cross-probe
/// a net from `CadObject::net_id` without re-deriving connectivity.
///
/// Blank names are never interned: a wire without a net label carries no
/// identity and stays `None` on the scene.
struct NetIdentityMap {
    ids: HashMap<String, usize>,
}

impl NetIdentityMap {
    fn new() -> Self {
        Self {
            ids: HashMap::new(),
        }
    }

    /// Registers a non-blank net name if needed and returns its stable id.
    fn intern(&mut self, net_name: &str) -> Option<usize> {
        let name = net_name.trim();
        if name.is_empty() {
            return None;
        }
        if let Some(existing) = self.ids.get(name) {
            return Some(*existing);
        }
        let next = self.ids.len();
        self.ids.insert(name.to_string(), next);
        Some(next)
    }

    fn get(&self, net_name: &str) -> Option<usize> {
        self.ids.get(net_name.trim()).copied()
    }
}

/// Interns every net name carried by a PCB layout in document order.
fn pcb_net_ids(layout: &PcbLayout) -> NetIdentityMap {
    let mut map = NetIdentityMap::new();
    for component in &layout.components {
        for net_name in &component.pad_nets {
            map.intern(net_name);
        }
    }
    for trace in &layout.traces {
        map.intern(&trace.net);
    }
    for airwire in &layout.airwires {
        map.intern(&airwire.net);
    }
    map
}

fn schematic_pin_paths(center: Vec2, rotation_degrees: f32) -> Vec<Vec<Vec2>> {
    let radius = 0.9;
    let points = (0..=12)
        .map(|step| {
            let angle = step as f32 / 12.0 * std::f32::consts::TAU;
            center
                + rotate_vec2(
                    Vec2::new(angle.cos(), angle.sin()) * radius,
                    rotation_degrees,
                )
        })
        .collect();
    vec![points]
}

/// Vector symbol ink is authored in the component's local schematic space.
/// Pin offsets remain the electrical endpoints; the artwork only occupies the
/// compact center between them, so changing the symbol never changes picking.
fn schematic_component_symbol_paths(component: &ElectronicComponent) -> Vec<Vec<Vec2>> {
    let pins = component
        .pins
        .iter()
        .map(|pin| pin.offset * 20.0)
        .collect::<Vec<_>>();
    let point = |local: Vec2| component.position + rotate_vec2(local, component.rotation);
    let path = |points: &[Vec2]| points.iter().copied().map(point).collect::<Vec<_>>();

    let first = pins.first().copied().unwrap_or(Vec2::ZERO);
    let second = pins.get(1).copied().unwrap_or(first + Vec2::X * 20.0);
    let axis = (second - first).normalize_or_zero();
    let axis = if axis.length_squared() > 0.5 { axis } else { Vec2::X };
    let normal = Vec2::new(-axis.y, axis.x);
    let center = (first + second) * 0.5;
    let mut paths = Vec::new();

    match &component.sim_model {
        SimModel::Resistor { .. } => {
            let half = 6.0_f32.min(first.distance(second) * 0.22);
            let start = center - axis * half;
            let end = center + axis * half;
            paths.push(path(&[first, start]));
            paths.push(path(&[end, second]));
            let zigzag = [
                start,
                center - axis * half * 0.66 + normal * 4.5,
                center - axis * half * 0.33 - normal * 4.5,
                center + normal * 4.5,
                center + axis * half * 0.33 - normal * 4.5,
                center + axis * half * 0.66 + normal * 4.5,
                end,
            ];
            paths.push(path(&zigzag));
        }
        SimModel::Capacitor { .. } => {
            let plate_offset = 2.2_f32.min(first.distance(second) * 0.12);
            let plate_half = normal * 8.0;
            let left = center - axis * plate_offset;
            let right = center + axis * plate_offset;
            paths.push(path(&[first, left]));
            paths.push(path(&[right, second]));
            paths.push(path(&[left - plate_half, left + plate_half]));
            paths.push(path(&[right - plate_half, right + plate_half]));
        }
        SimModel::Led { .. } => {
            let base = center - axis * 8.0;
            let cathode = center + axis * 7.0;
            let tip = center + axis * 4.0;
            paths.push(path(&[first, base]));
            paths.push(path(&[cathode, second]));
            paths.push(path(&[
                base - normal * 8.0,
                tip,
                base + normal * 8.0,
                base - normal * 8.0,
            ]));
            paths.push(path(&[
                cathode - normal * 10.0,
                cathode + normal * 10.0,
            ]));
            for offset in [normal * 12.0, normal * 5.0] {
                let ray_start = center - axis * 4.0 - offset;
                let ray_end = ray_start + axis * 6.0 - normal * 6.0;
                paths.push(path(&[ray_start, ray_end]));
                paths.push(path(&[
                    ray_end - axis * 2.0 + normal * 0.5,
                    ray_end,
                    ray_end + normal * 2.5,
                ]));
            }
        }
        SimModel::Magnet { .. } => {
            let upper = center - axis * 8.0;
            let lower = center + axis * 8.0;
            paths.push(path(&[first, upper]));
            paths.push(path(&[lower, second]));
            let outline = [
                center - axis * 8.0 - normal * 8.0,
                center - axis * 8.0 + normal * 8.0,
                center - axis * 4.0 + normal * 8.0,
                center - axis * 2.0 + normal * 6.0,
                center + axis * 2.0 + normal * 6.0,
                center + axis * 4.0 + normal * 8.0,
                center + axis * 8.0 + normal * 8.0,
                center + axis * 8.0 - normal * 8.0,
                center + axis * 4.0 - normal * 8.0,
                center + axis * 2.0 - normal * 6.0,
                center - axis * 2.0 - normal * 6.0,
                center - axis * 4.0 - normal * 8.0,
                center - axis * 8.0 - normal * 8.0,
            ];
            paths.push(path(&outline));
        }
        SimModel::DcSource { .. } => {
            let plate_offset = 3.5_f32.min(first.distance(second) * 0.16);
            let plate_half = normal * 9.0;
            let long_plate = center - axis * plate_offset;
            let short_plate = center + axis * plate_offset;
            paths.push(path(&[first, long_plate]));
            paths.push(path(&[short_plate, second]));
            paths.push(path(&[long_plate - plate_half, long_plate + plate_half]));
            paths.push(path(&[
                short_plate - plate_half * 0.64,
                short_plate + plate_half * 0.64,
            ]));
        }
        SimModel::Wire => {
            if component.pins.len() <= 1 {
                let origin = pins.first().copied().unwrap_or(Vec2::ZERO);
                let stem = origin + Vec2::Y * 5.0;
                paths.push(path(&[origin, stem]));
                for (index, half_width) in [8.0, 5.5, 3.0].into_iter().enumerate() {
                    let y = stem.y + index as f32 * 3.0;
                    paths.push(path(&[
                        Vec2::new(-half_width, y),
                        Vec2::new(half_width, y),
                    ]));
                }
            } else {
                let half = 7.0_f32.min(first.distance(second) * 0.22);
                let start = center - axis * half;
                let end = center + axis * half;
                paths.push(path(&[first, start]));
                paths.push(path(&[start, start + normal * 6.0, end + normal * 6.0, end]));
                paths.push(path(&[end, second]));
            }
        }
    }

    paths
}

fn schematic_component_body_size(component: &ElectronicComponent) -> Vec2 {
    let pin_extent = component
        .pins
        .iter()
        .fold(Vec2::new(40.0, 28.0), |extent, pin| {
            Vec2::new(
                extent.x.max(pin.offset.x.abs() * 40.0),
                extent.y.max(pin.offset.y.abs() * 40.0),
            )
        });
    Vec2::new(pin_extent.x + 40.0, pin_extent.y + 28.0)
}

/// Returns the footprint-sized body extent used by the schematic scene after
/// applying the component rotation. Native editors use this same value for
/// placement previews so the ghost and the committed symbol share geometry.
pub fn schematic_component_size(component: &ElectronicComponent) -> Vec2 {
    rotate_extent(schematic_component_body_size(component), component.rotation)
}

fn rotate_extent(size: Vec2, rotation_degrees: f32) -> Vec2 {
    let normalized = ((rotation_degrees % 360.0) + 360.0) % 360.0;
    if (normalized - 90.0).abs() < 0.01 || (normalized - 270.0).abs() < 0.01 {
        Vec2::new(size.y, size.x)
    } else {
        size
    }
}

/// Distance from a point to the visible filled shape, including its drill.
/// The result is zero inside copper and positive outside, which lets picking
/// share the renderer's physical pad boundary without exposing CAD internals.
pub fn cad_object_distance(object: &CadObject, point: Vec2) -> f32 {
    if let Some(rect) = object.rect {
        let local = rotate_vec2(
            point - rect.center,
            -finite_rotation(object.rotation_degrees),
        );
        let half = rect.size.abs() * 0.5;
        let outer_signed = shape_signed_distance(local, half, object.shape);
        let outer = outer_signed.max(0.0);
        if object.kind == CadObjectKind::Pad {
            if let Some(drill) = object.drill {
                if drill.size.is_finite()
                    && drill.offset.is_finite()
                    && drill.size.min_element() > 0.0
                    && outer_signed <= 0.0
                {
                    let hole_signed = shape_signed_distance(
                        local - drill.offset,
                        drill.size.abs() * 0.5,
                        match drill.shape {
                            CadDrillShape::Circle => CadShape::Circle,
                            CadDrillShape::Oval => CadShape::Oval,
                        },
                    );
                    if hole_signed < 0.0 {
                        return -hole_signed;
                    }
                }
            }
        }
        return outer;
    }

    let distance = object
        .points
        .windows(2)
        .map(|segment| distance_to_segment(point, segment[0], segment[1]))
        .chain(object.line_paths.iter().flat_map(|path| {
            path.windows(2)
                .map(|segment| distance_to_segment(point, segment[0], segment[1]))
        }))
        .fold(f32::INFINITY, f32::min);
    (distance
        - object
            .stroke_width_world
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(0.0)
            * 0.5)
        .max(0.0)
}

fn cad_object_contains(object: &CadObject, point: Vec2) -> bool {
    if object.rect.is_some() {
        return cad_object_distance(object, point) <= 0.0;
    }
    cad_object_distance(object, point) <= 6.0
}

fn finite_rotation(degrees: f32) -> f32 {
    if degrees.is_finite() {
        degrees.rem_euclid(360.0)
    } else {
        0.0
    }
}

fn shape_signed_distance(point: Vec2, half: Vec2, shape: CadShape) -> f32 {
    if half.min_element() <= 0.0 || !half.is_finite() || !point.is_finite() {
        return f32::INFINITY;
    }
    let rounded_box = |half: Vec2, radius: f32| {
        let q = point.abs() - (half - Vec2::splat(radius));
        q.max(Vec2::ZERO).length() + q.max_element().min(0.0) - radius
    };
    let box_distance = |half: Vec2| {
        let delta = point.abs() - half;
        delta.max(Vec2::ZERO).length() + delta.max_element().min(0.0)
    };
    match shape {
        CadShape::Rectangle => box_distance(half),
        CadShape::Circle => point.length() - half.min_element(),
        CadShape::RoundRect { corner_radius } => {
            rounded_box(half, safe_corner_radius(corner_radius, half))
        }
        CadShape::Oval => {
            let radius = half.min_element();
            let extent = if half.x >= half.y { half.x } else { half.y };
            let along_x = half.x >= half.y;
            let cap = (extent - radius).max(0.0);
            let axis_point = if along_x {
                Vec2::new(point.x.clamp(-cap, cap), 0.0)
            } else {
                Vec2::new(0.0, point.y.clamp(-cap, cap))
            };
            point.distance(axis_point) - radius
        }
    }
}

fn cad_pad_shape(shape: &FootprintPadShape, size: Vec2) -> CadShape {
    match shape {
        FootprintPadShape::Rectangle | FootprintPadShape::Unsupported { .. } => CadShape::Rectangle,
        FootprintPadShape::Circle => CadShape::Circle,
        FootprintPadShape::Oval => CadShape::Oval,
        FootprintPadShape::RoundRect {
            corner_ratio: Some(ratio),
        } if ratio.is_finite() && (0.0..=1.0).contains(ratio) => CadShape::RoundRect {
            corner_radius: size.min_element() * *ratio as f32,
        },
        FootprintPadShape::RoundRect { .. } => CadShape::Rectangle,
    }
}

fn pad_copper_layer(layers: &[FootprintLayer], component_layer: PcbLayer) -> Option<PcbLayer> {
    let front = layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::FrontCopper | FootprintLayer::AllCopper
        )
    });
    let back = layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::BackCopper | FootprintLayer::AllCopper
        )
    });
    match (front, back) {
        (false, false) => None,
        (true, true) => Some(component_layer),
        (true, false) => Some(component_layer),
        (false, true) => Some(match component_layer {
            PcbLayer::TopCopper => PcbLayer::BottomCopper,
            PcbLayer::BottomCopper => PcbLayer::TopCopper,
        }),
    }
}

fn safe_corner_radius(radius: f32, half: Vec2) -> f32 {
    if radius.is_finite() {
        radius.clamp(0.0, half.min_element())
    } else {
        0.0
    }
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let len_sq = segment.length_squared();
    if len_sq <= f32::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / len_sq).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

fn rotate_vec2(value: Vec2, degrees: f32) -> Vec2 {
    let radians = finite_rotation(degrees).to_radians();
    let cos_r = radians.cos();
    let sin_r = radians.sin();
    Vec2::new(
        value.x * cos_r - value.y * sin_r,
        value.x * sin_r + value.y * cos_r,
    )
}

/// Keeps the authoritative CAD representation aligned with the editor's wire-routing rule.
///
/// Persisted wires currently store their endpoints, so the elbow is derived deterministically
/// instead of being duplicated as another piece of scene state. This makes AGB/WGPU, overlays,
/// snapping and hit regions agree without requiring a second wire geometry format.
pub fn orthogonal_wire_points(start: Vec2, end: Vec2) -> Vec<Vec2> {
    if (start.x - end.x).abs() < 0.01 || (start.y - end.y).abs() < 0.01 {
        vec![start, end]
    } else {
        vec![start, Vec2::new(end.x, start.y), end]
    }
}

fn pcb_layer_to_cad(layer: PcbLayer) -> CadLayerKind {
    match layer {
        PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
        PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
    }
}

fn pcb_copper_color(layer: PcbLayer) -> [u8; 4] {
    match layer {
        PcbLayer::TopCopper => [207, 150, 91, 255],
        PcbLayer::BottomCopper => [104, 158, 194, 255],
    }
}

fn drc_color(severity: DrcSeverity) -> [u8; 4] {
    match severity {
        DrcSeverity::Error => [220, 66, 58, 255],
        DrcSeverity::Warning => [245, 180, 65, 255],
        DrcSeverity::Info => [170, 170, 178, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;

    #[test]
    fn schematic_scene_exports_components_pins_and_wires() {
        let mut schematic = Schematic::new("Cad Test");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(50.0, 60.0);
        let resistor_id = resistor.id;
        schematic.add_component(resistor);
        schematic.add_wire(Vec2::new(0.0, 0.0), Vec2::new(80.0, 0.0), "N001");

        let scene = CadScene::from_schematic(&schematic);

        assert!(scene
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::Component));
        assert!(scene
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::Pin));
        assert!(scene
            .objects
            .iter()
            .filter(|object| object.kind == CadObjectKind::Pin)
            .all(|object| !object.line_paths.is_empty()));
        assert!(scene.objects.iter().all(|object| {
            object.kind != CadObjectKind::Pin || object.source_id == Some(resistor_id)
        }));
        assert!(scene
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::Wire));
        assert_eq!(
            scene
                .hit_test(Vec2::new(5.0, 0.0))
                .map(|object| object.kind),
            Some(CadObjectKind::Wire)
        );
    }

    #[test]
    fn schematic_component_scene_contains_compact_vector_symbol_geometry() {
        let mut schematic = Schematic::new("Vector symbol geometry");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(100.0, 80.0);
        resistor.rotation = 90.0;
        let component_id = resistor.id;
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let component = scene
            .objects
            .iter()
            .find(|object| object.kind == CadObjectKind::Component && object.source_id == Some(component_id))
            .expect("component CAD object");

        assert!(!component.line_paths.is_empty());
        assert!(component.rect.is_some());
        let bounds = component
            .line_paths
            .iter()
            .flatten()
            .fold((Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)), |(min, max), point| {
                (min.min(*point), max.max(*point))
            });
        let symbol_size = bounds.1 - bounds.0;
        assert!(symbol_size.x < 25.0 && symbol_size.y < 45.0, "symbol ink stays proportional to pin spacing: {symbol_size:?}");
    }

    #[test]
    fn pcb_scene_exports_board_component_and_pads() {
        let mut layout = PcbLayout::new("Cad PCB");
        layout.components.push(crate::pcb::PcbComponentPlacement {
            component_id: Uuid::new_v4(),
            designator: "R1".to_string(),
            value: "10k".to_string(),
            footprint: "0805".to_string(),
            position: Vec2::new(100.0, 80.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["A".to_string(), "B".to_string()],
        });

        let scene = CadScene::from_pcb(&layout);

        assert!(scene
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::BoardOutline));
        assert!(scene
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::Pad));
    }

    #[test]
    fn physical_pad_shapes_drills_layers_and_bottom_transform_reach_the_scene() {
        let asset = crate::pcb::parse_kicad_footprint(
            r#"(footprint "CanvasTest"
              (pad "1" thru_hole circle (at 1 0 90) (size 2 2) (drill 0.8) (layers "*.Cu" "*.Mask"))
              (pad "2" smd roundrect (at -1 0 30) (size 2 1) (roundrect_rratio 0.25) (layers "F.Cu" "F.Mask")))"#,
        )
        .expect("physical footprint asset parses");
        let footprint_id = asset.id.clone();
        let component_id = Uuid::new_v4();
        let mut layout = PcbLayout::new("Physical CAD pads");
        layout.footprint_assets.insert(footprint_id.clone(), asset);
        layout.components.push(crate::pcb::PcbComponentPlacement {
            component_id,
            designator: "J1".to_string(),
            value: "Header".to_string(),
            footprint: footprint_id,
            position: Vec2::new(100.0, 50.0),
            rotation: 90.0,
            layer: PcbLayer::BottomCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["GND".to_string(), "SIG".to_string()],
        });

        let scene = CadScene::from_pcb(&layout);
        let pads = scene
            .objects
            .iter()
            .filter(|object| object.kind == CadObjectKind::Pad)
            .collect::<Vec<_>>();
        assert_eq!(pads.len(), 2);
        assert_eq!(pads[0].label.as_deref(), Some("1"));
        assert_eq!(pads[0].layer, CadLayerKind::PcbBottomCopper);
        assert_eq!(pads[0].shape, CadShape::Circle);
        assert!((pads[0].rect.unwrap().center - Vec2::new(100.0, 49.0)).length() < 0.001);
        assert_eq!(pads[0].drill.unwrap().shape, CadDrillShape::Circle);
        assert_eq!(pads[0].drill.unwrap().size, Vec2::splat(0.8));
        assert!(pads[0].drill.unwrap().plated);
        assert_eq!(pads[0].net.as_deref(), Some("GND"));

        assert_eq!(pads[1].label.as_deref(), Some("2"));
        assert_eq!(pads[1].layer, CadLayerKind::PcbBottomCopper);
        assert_eq!(
            pads[1].shape,
            CadShape::RoundRect {
                corner_radius: 0.25
            }
        );
        assert!((pads[1].rect.unwrap().center - Vec2::new(100.0, 51.0)).length() < 0.001);
        assert!((pads[1].rotation_degrees - 240.0).abs() < 0.001);
        assert_eq!(pads[1].net.as_deref(), Some("SIG"));
    }

    #[test]
    fn stable_fingerprint_changes_when_renderable_data_changes() {
        let mut schematic = Schematic::new("Cad Fingerprint");
        schematic.add_component(ElectronicComponent::resistor("10k"));
        let scene = CadScene::from_schematic(&schematic);
        let first = scene.stable_fingerprint();
        assert_eq!(first, scene.clone().stable_fingerprint());

        let mut changed = scene.clone();
        changed.objects[0].color_rgba[0] ^= 0xff;
        assert_ne!(first, changed.stable_fingerprint());
    }

    #[test]
    fn pad_shape_rotation_and_drill_are_fingerprint_inputs() {
        let mut layout = PcbLayout::new("Pad fingerprint");
        layout.components.push(crate::pcb::PcbComponentPlacement {
            component_id: Uuid::new_v4(),
            designator: "J1".to_string(),
            value: "Header".to_string(),
            footprint: "legacy-header".to_string(),
            position: Vec2::ZERO,
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["GND".to_string()],
        });
        let scene = CadScene::from_pcb(&layout);
        let first = scene.stable_fingerprint();
        let pad_index = scene
            .objects
            .iter()
            .position(|object| object.kind == CadObjectKind::Pad)
            .expect("legacy footprint has a pad");

        let mut changed = scene.clone();
        changed.objects[pad_index].shape = CadShape::Circle;
        assert_ne!(first, changed.stable_fingerprint());

        let mut changed = scene.clone();
        changed.objects[pad_index].rotation_degrees = 23.0;
        assert_ne!(first, changed.stable_fingerprint());

        let mut changed = scene;
        changed.objects[pad_index].drill = Some(CadDrill {
            shape: CadDrillShape::Oval,
            size: Vec2::new(0.6, 1.0),
            offset: Vec2::new(0.1, -0.2),
            plated: true,
        });
        assert_ne!(first, changed.stable_fingerprint());
    }

    #[test]
    fn rotated_component_swaps_body_dimensions_in_cad_scene() {
        let mut schematic = Schematic::new("Rotated body");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(100.0, 100.0);
        resistor.rotation = 90.0;
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let body = scene
            .objects
            .iter()
            .find(|object| object.kind == CadObjectKind::Component)
            .expect("component CAD object");

        let rect = body.rect.expect("body rect");
        let unrotated = ElectronicComponent::resistor("10k");
        let expected_unrotated = schematic_component_body_size(&unrotated);
        assert!(
            (rect.size.x - expected_unrotated.y).abs() < 0.01
                && (rect.size.y - expected_unrotated.x).abs() < 0.01,
            "rotated 90 degree body should swap width/height, got {:?}",
            rect.size
        );
    }

    #[test]
    fn unrotated_component_keeps_axis_aligned_body() {
        let mut schematic = Schematic::new("Axis body");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(100.0, 100.0);
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let body = scene
            .objects
            .iter()
            .find(|object| object.kind == CadObjectKind::Component)
            .expect("component CAD object");
        let rect = body.rect.expect("body rect");

        let unrotated = ElectronicComponent::resistor("10k");
        let expected = schematic_component_body_size(&unrotated);
        assert_eq!(rect.size, expected);
    }

    #[test]
    fn pin_markers_stay_small_circles_after_component_rotation() {
        let mut schematic = Schematic::new("Rotated pin");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(100.0, 100.0);
        resistor.rotation = 90.0;
        let pin_id = resistor.pins[0].id;
        let component_id = resistor.id;
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let pin = scene
            .objects
            .iter()
            .find(|object| {
                object.kind == CadObjectKind::Pin
                    && object.source_id == Some(component_id)
                    && object.id.ends_with(&pin_id.to_string())
            })
            .expect("rotated pin CAD object");

        let path = &pin.line_paths[0];
        let center = component_pin_world_position(
            &schematic.components[0],
            &schematic.components[0].pins[0],
        );
        let expected_radius = 0.9;
        for point in path {
            let distance = point.distance(center);
            assert!(
                (distance - expected_radius).abs() < 0.01,
                "rotated pin corners must stay on a circle of radius {expected_radius} around the pin center, got {distance} for {point:?} vs center {center:?}"
            );
        }
    }

    #[test]
    fn schematic_wires_use_deterministic_orthogonal_geometry() {
        let mut schematic = Schematic::new("Orthogonal CAD");
        schematic.add_wire(Vec2::new(10.0, 20.0), Vec2::new(80.0, 90.0), "N001");

        let scene = CadScene::from_schematic(&schematic);
        let wire = scene
            .objects
            .iter()
            .find(|object| object.kind == CadObjectKind::Wire)
            .expect("schematic wire should be exported");

        assert_eq!(
            wire.points,
            vec![
                Vec2::new(10.0, 20.0),
                Vec2::new(80.0, 20.0),
                Vec2::new(80.0, 90.0),
            ]
        );
    }

    #[test]
    fn wire_label_and_touched_pins_share_one_stable_net_id() {
        let mut schematic = Schematic::new("Net identity");
        let mut left = ElectronicComponent::resistor("10k");
        left.position = Vec2::new(100.0, 100.0);
        let left_id = left.id;
        schematic.add_component(left);
        let mut right = ElectronicComponent::resistor("4.7k");
        right.position = Vec2::new(140.0, 100.0);
        let right_id = right.id;
        schematic.add_component(right);
        schematic.add_wire(Vec2::new(120.0, 100.0), Vec2::new(120.0, 100.0), "VCC");

        let scene = CadScene::from_schematic(&schematic);

        let wire_ids = ids_of_kind(&scene, CadObjectKind::Wire);
        assert_eq!(wire_ids.len(), 1);
        let expected = wire_ids[0].expect("wire net id");
        assert_eq!(
            ids_of_kind(&scene, CadObjectKind::NetLabel),
            vec![Some(expected)]
        );

        let joined_pins = scene
            .objects
            .iter()
            .filter(|object| object.kind == CadObjectKind::Pin && object.net_id == Some(expected))
            .filter(|object| {
                object.source_id == Some(left_id) || object.source_id == Some(right_id)
            })
            .count();
        assert_eq!(
            joined_pins, 2,
            "both wire endpoints should report the net id"
        );
    }

    #[test]
    fn net_ids_are_stable_across_identical_rebuilds() {
        let mut schematic = Schematic::new("Stable net identity");
        let mut component = ElectronicComponent::resistor("10k");
        component.position = Vec2::new(0.0, 200.0);
        schematic.add_component(component);
        schematic.add_wire(Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), "SIG_A");
        schematic.add_wire(Vec2::new(60.0, 0.0), Vec2::new(80.0, 0.0), "SIG_B");

        let first = CadScene::from_schematic(&schematic);
        let second = CadScene::from_schematic(&schematic);

        let ids = |scene: &CadScene| -> Vec<Option<usize>> {
            scene.objects.iter().map(|object| object.net_id).collect()
        };
        assert_eq!(ids(&first), ids(&second));

        let wire_ids = ids_of_kind(&first, CadObjectKind::Wire);
        assert_eq!(wire_ids.len(), 2);
        assert!(wire_ids.iter().all(|id| id.is_some()));
        assert_ne!(
            wire_ids[0], wire_ids[1],
            "different nets must not share an identity"
        );
        assert_eq!(ids_of_kind(&first, CadObjectKind::NetLabel), wire_ids);
    }

    fn ids_of_kind(scene: &CadScene, kind: CadObjectKind) -> Vec<Option<usize>> {
        scene
            .objects
            .iter()
            .filter(|object| object.kind == kind)
            .map(|object| object.net_id)
            .collect()
    }
}
