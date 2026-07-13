use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::component::ElectronicComponent;
use crate::drc::{DrcIssue, DrcReport, DrcSeverity};
use crate::pcb::{footprint_definition, PcbLayer, PcbLayout};
use crate::schematic::{component_pin_world_position, Schematic};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadObject {
    pub id: String,
    pub source_id: Option<Uuid>,
    pub kind: CadObjectKind,
    pub layer: CadLayerKind,
    pub pick_priority: CadPickPriority,
    pub rect: Option<CadRect>,
    pub points: Vec<Vec2>,
    pub label: Option<String>,
    pub net: Option<String>,
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
            points: Vec::new(),
            label: None,
            net: None,
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
            points,
            label: None,
            net: None,
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
            hash.write_usize(object.points.len());
            for point in &object.points {
                hash.write_vec2(*point);
            }
            hash.write_option_str(object.label.as_deref());
            hash.write_option_str(object.net.as_deref());
            hash.write_bytes(&object.color_rgba);
        }

        hash.finish()
    }

    pub fn from_schematic(schematic: &Schematic) -> Self {
        let mut scene = Self {
            surface: CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };

        for component in &schematic.components {
            push_schematic_component(&mut scene.objects, component);
        }

        for wire in &schematic.wires {
            let mut object = CadObject::polyline(
                format!("wire:{}", wire.id),
                Some(wire.id),
                CadObjectKind::Wire,
                CadLayerKind::Schematic,
                CadPickPriority::Wire,
                vec![wire.start, wire.end],
                [212, 119, 26, 255],
            );
            object.net = Some(wire.net.clone());
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
                label.label = Some(wire.net.clone());
                label.net = Some(wire.net.clone());
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

        scene.objects.push(CadObject::polyline(
            "board_outline",
            None,
            CadObjectKind::BoardOutline,
            CadLayerKind::PcbSilkscreen,
            CadPickPriority::Background,
            layout.board_outline.points.clone(),
            [230, 230, 232, 255],
        ));

        for component in &layout.components {
            let footprint = footprint_definition(&component.footprint, component.pad_nets.len());
            let body_center = component.position;
            scene.objects.push(CadObject::rect(
                format!("pcb_component:{}", component.component_id),
                Some(component.component_id),
                CadObjectKind::Component,
                pcb_layer_to_cad(component.layer),
                CadPickPriority::Component,
                CadRect::new(body_center, footprint.body_size),
                [34, 34, 38, 255],
            ));

            for (pad_index, pad) in footprint.pads.iter().enumerate() {
                let center = component.position + rotate_vec2(pad.offset, component.rotation);
                let mut object = CadObject::rect(
                    format!("pad:{}:{pad_index}", component.component_id),
                    Some(component.component_id),
                    CadObjectKind::Pad,
                    pcb_layer_to_cad(component.layer),
                    CadPickPriority::Pin,
                    CadRect::new(center, pad.size),
                    [212, 119, 26, 255],
                );
                object.label = Some(pad.name.clone());
                object.net = component.pad_nets.get(pad_index).cloned();
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
                [212, 119, 26, 255],
            );
            object.net = Some(trace.net.clone());
            scene.objects.push(object);
        }

        for (index, airwire) in layout.airwires.iter().enumerate() {
            let mut object = CadObject::polyline(
                format!("airwire:{index}"),
                None,
                CadObjectKind::Airwire,
                CadLayerKind::Airwire,
                CadPickPriority::Overlay,
                vec![airwire.from, airwire.to],
                [245, 245, 246, 180],
            );
            object.net = Some(airwire.net.clone());
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

fn push_schematic_component(objects: &mut Vec<CadObject>, component: &ElectronicComponent) {
    let body_size = schematic_component_body_size(component);
    let mut body = CadObject::rect(
        format!("component:{}", component.id),
        Some(component.id),
        CadObjectKind::Component,
        CadLayerKind::Schematic,
        CadPickPriority::Component,
        CadRect::new(component.position, body_size),
        component.appearance.color,
    );
    body.label = Some(format!("{} {}", component.designator, component.value));
    objects.push(body);

    for pin in &component.pins {
        let mut object = CadObject::rect(
            format!("pin:{}:{}", component.id, pin.id),
            Some(component.id),
            CadObjectKind::Pin,
            CadLayerKind::Schematic,
            CadPickPriority::Pin,
            CadRect::new(
                component_pin_world_position(component, pin),
                Vec2::new(10.0, 10.0),
            ),
            [245, 245, 246, 255],
        );
        object.label = Some(pin.name.clone());
        if !pin.net.trim().is_empty() {
            object.net = Some(pin.net.clone());
        }
        objects.push(object);
    }
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

fn cad_object_contains(object: &CadObject, point: Vec2) -> bool {
    if object
        .rect
        .map(|rect| rect.contains(point))
        .unwrap_or(false)
    {
        return true;
    }
    object
        .points
        .windows(2)
        .any(|segment| distance_to_segment(point, segment[0], segment[1]) <= 6.0)
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
    let radians = degrees.to_radians();
    let cos_r = radians.cos();
    let sin_r = radians.sin();
    Vec2::new(
        value.x * cos_r - value.y * sin_r,
        value.x * sin_r + value.y * cos_r,
    )
}

fn pcb_layer_to_cad(layer: PcbLayer) -> CadLayerKind {
    match layer {
        PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
        PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
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
}
