//! The signal map paints a bounded, static image from the real circuit topology.
//! RafUI keeps the labels and hit targets independently accessible.
use glam::Vec2;
use image::{Rgba, RgbaImage};
use raf_ui::UiTokens;

use crate::electronics_lab::{ElectronicsLab, GraphPart};

pub(crate) const IMAGE_KEY: &str = "electronics-lab/signal-field";
const LIMIT: usize = 32;
pub(crate) const PART_SIZE: [f32; 2] = [52.0, 28.0];

pub(crate) struct Part<'a> {
    pub model: &'a GraphPart,
    pub at: Vec2,
}

pub(crate) struct NetLabel<'a> {
    pub name: &'a str,
    pub at: Vec2,
    id: usize,
}

pub(crate) struct Field<'a> {
    pub parts: Vec<Part<'a>>,
    pub net_labels: Vec<NetLabel<'a>>,
    traces: Vec<Trace>,
    pin_leads: Vec<Trace>,
    selected_net: Option<usize>,
    pub limited: bool,
}

struct Trace {
    start: Vec2,
    end: Vec2,
    net: Option<usize>,
    active: bool,
}

impl<'a> Field<'a> {
    pub fn new(lab: &'a ElectronicsLab, width: f32, height: f32) -> Self {
        let mut field = Self {
            parts: Vec::new(),
            net_labels: Vec::new(),
            traces: Vec::new(),
            pin_leads: Vec::new(),
            selected_net: None,
            limited: false,
        };
        let Some(graph) = &lab.graph else {
            return field;
        };
        let capacity = ((width / 19.0) as usize).clamp(8, LIMIT);
        field.limited = graph.limited || capacity < graph.parts.len();
        let points = graph
            .parts
            .iter()
            .map(|part| part.position)
            .chain(graph.wires.iter().flat_map(|wire| [wire.start, wire.end]))
            .map(Vec2::from_array)
            .filter(|point| point.is_finite());
        let (lo, hi) = points.fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(lo, hi), point| (lo.min(point), hi.max(point)),
        );
        let middle = (lo + hi) * 0.5;
        let span = (hi - lo).max(Vec2::splat(40.0));
        let project = |point: [f32; 2]| {
            Vec2::new(0.48, 0.53)
                + (Vec2::from_array(point) - middle) / span * Vec2::new(0.66, 0.66)
        };
        field.parts.extend(
            graph
                .parts
                .iter()
                .filter(|part| Vec2::from_array(part.position).is_finite())
                .take(capacity)
                .map(|model| Part {
                    model,
                    at: project(model.position),
                }),
        );
        let selected = graph
            .parts
            .iter()
            .find(|part| Some(part.id) == lab.selected);
        field.selected_net = lab.selected_pin.and_then(|pin| {
            graph
                .edges
                .iter()
                .find(|(part, index, _)| Some(*part) == lab.selected && *index == pin)
                .map(|(_, _, net)| *net)
        });
        field.traces.extend(graph.wires.iter().map(|wire| {
            let active = match lab.selected_pin {
                Some(_) => field.selected_net.is_some() && wire.net == field.selected_net,
                None => selected.is_some_and(|part| {
                    graph.edges.iter().any(|(id, pin, net)| {
                        *id == part.id
                            && wire.net == Some(*net)
                            && touches_wire(part.terminals[*pin], wire.start, wire.end)
                    })
                }),
            };
            Trace {
                start: project(wire.start),
                end: project(wire.end),
                net: wire.net,
                active,
            }
        }));
        let viewport = Vec2::new(width.max(1.0), height.max(1.0));
        let half_card = Vec2::from_array(PART_SIZE) * 0.5;
        for part in &field.parts {
            for (index, &pin) in part.model.terminals.iter().enumerate() {
                let Some((_, _, net)) = graph
                    .edges
                    .iter()
                    .find(|(id, i, _)| *id == part.model.id && *i == index)
                else {
                    continue;
                };
                let joint = graph
                    .wires
                    .iter()
                    .filter(|wire| wire.net == Some(*net))
                    .map(|wire| closest_wire_point(pin, wire.start, wire.end))
                    .map(|point| (point.distance_squared(Vec2::from_array(pin)), point))
                    .filter(|(distance, _)| *distance <= 4.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                let Some((_, joint)) = joint else {
                    continue;
                };
                let end = project(joint.to_array());
                let offset = ((end - part.at) * viewport).abs();
                let ratio = (offset / half_card).max_element();
                if ratio > 1.0 {
                    field.pin_leads.push(Trace {
                        start: part.at.lerp(end, ratio.recip()),
                        end,
                        net: Some(*net),
                        active: field.selected_net.map_or(
                            lab.selected_pin.is_none() && lab.selected == Some(part.model.id),
                            |selected| selected == *net,
                        ),
                    });
                }
            }
        }
        for (id, name) in &graph.nets {
            if let Some(wire) = field
                .traces
                .iter()
                .filter(|wire| wire.net == Some(*id))
                .max_by(|a, b| {
                    a.start
                        .distance_squared(a.end)
                        .total_cmp(&b.start.distance_squared(b.end))
                })
            {
                field.net_labels.push(NetLabel {
                    name,
                    at: wire.start.lerp(wire.end, 0.52),
                    id: *id,
                });
            }
        }
        field
    }

    pub fn highlighted(&self, junction: &NetLabel<'_>) -> bool {
        self.selected_net == Some(junction.id)
    }
}

fn touches_wire(pin: [f32; 2], start: [f32; 2], end: [f32; 2]) -> bool {
    closest_wire_point(pin, start, end).distance_squared(Vec2::from_array(pin)) <= 4.0
}

fn closest_wire_point(pin: [f32; 2], start: [f32; 2], end: [f32; 2]) -> Vec2 {
    let start = Vec2::from_array(start);
    let delta = Vec2::from_array(end) - start;
    let offset = Vec2::from_array(pin) - start;
    let position = (offset.dot(delta) / delta.length_squared().max(f32::EPSILON)).clamp(0.0, 1.0);
    start + delta * position
}

pub(crate) fn texture_size(width: f32, height: f32) -> [u32; 2] {
    let scale = (1200.0 / width.max(1.0))
        .min(740.0 / height.max(1.0))
        .min(1.0);
    [
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    ]
}

pub(crate) fn paint(field: &Field<'_>, size: [u32; 2], tokens: &UiTokens) -> Vec<u8> {
    let [w, h] = size;
    let width = w as f32;
    let height = h as f32;
    let extent = Vec2::new(width, height);
    let mut layer = RgbaImage::new(w, h);
    let mut grid = tokens.background;
    for (index, channel) in grid[..3].iter_mut().enumerate() {
        *channel = ((u16::from(*channel) * 2 + u16::from(tokens.border[index])) / 3) as u8;
    }
    for (x, y, pixel) in layer.enumerate_pixels_mut() {
        *pixel = Rgba(if x % 24 == 0 && y % 24 == 0 {
            grid
        } else {
            tokens.background
        });
    }
    if field.parts.is_empty() {
        return layer.into_raw();
    }

    for active in [false, true] {
        for trace in field
            .traces
            .iter()
            .chain(&field.pin_leads)
            .filter(|trace| trace.active == active)
        {
            let a = trace.start * extent;
            let b = trace.end * extent;
            let gaps = crossing_gaps(trace.net, a, b, &field.traces, extent);
            let color = rgb(if active {
                tokens.accent_hot
            } else {
                tokens.text_muted
            });
            let steps = ((a.distance(b) * 0.85) as usize).clamp(1, 960);
            for index in 0..=steps {
                let point = a.lerp(b, index as f32 / steps as f32);
                if gaps.iter().any(|y| (point.y - y).abs() < 5.0) {
                    continue;
                }
                dot(
                    &mut layer,
                    point,
                    if active { 1.8 } else { 1.4 },
                    color,
                    if active { 0.98 } else { 0.56 },
                );
            }
        }
    }
    // Actual wire junctions. Intersections away from endpoints are just crossings.
    let mut endpoints = std::collections::BTreeMap::<(i32, i32, usize), usize>::new();
    for trace in &field.traces {
        if let Some(net) = trace.net {
            for point in [trace.start, trace.end] {
                let p = point * extent;
                *endpoints
                    .entry((p.x.round() as i32, p.y.round() as i32, net))
                    .or_default() += 1;
            }
        }
    }
    for ((x, y, net), count) in endpoints {
        if count >= 3 {
            let active = field.selected_net == Some(net);
            terminal(
                &mut layer,
                Vec2::new(x as f32, y as f32),
                3.5,
                rgb(if active {
                    tokens.accent_hot
                } else {
                    tokens.text_muted
                }),
                tokens.background,
            );
        }
    }
    for lead in &field.pin_leads {
        terminal(
            &mut layer,
            lead.start * extent,
            2.65,
            rgb(if lead.active {
                tokens.accent_hot
            } else {
                tokens.text_muted
            }),
            tokens.background,
        );
    }
    layer.into_raw()
}

// Distinct nets can cross without forming a junction. Keep the horizontal stroke
// continuous and leave a small gap in the vertical one at those crossings.
fn crossing_gaps(net: Option<usize>, a: Vec2, b: Vec2, traces: &[Trace], extent: Vec2) -> Vec<f32> {
    if net.is_none() || (a.x - b.x).abs() > 0.25 {
        return Vec::new();
    }
    traces
        .iter()
        .filter(|other| other.net.is_some() && other.net != net)
        .filter_map(|other| {
            let c = other.start * extent;
            let d = other.end * extent;
            ((c.y - d.y).abs() <= 0.25
                && (c.x.min(d.x) + 5.0..c.x.max(d.x) - 5.0).contains(&a.x)
                && (a.y.min(b.y) + 7.0..a.y.max(b.y) - 7.0).contains(&c.y))
            .then_some(c.y)
        })
        .collect()
}

fn rgb([r, g, b, _]: [u8; 4]) -> [u8; 3] {
    [r, g, b]
}

fn terminal(layer: &mut RgbaImage, at: Vec2, radius: f32, color: [u8; 3], background: [u8; 4]) {
    dot(layer, at, radius + 0.6, color, 1.0);
    dot(layer, at, radius - 0.7, rgb(background), 0.9);
}

fn dot(layer: &mut RgbaImage, at: Vec2, radius: f32, color: [u8; 3], strength: f32) {
    let x0 = (at.x - radius).floor().max(0.0) as u32;
    let x1 = (at.x + radius).ceil().min(layer.width() as f32) as u32;
    let y0 = (at.y - radius).floor().max(0.0) as u32;
    let y1 = (at.y + radius).ceil().min(layer.height() as f32) as u32;
    let inv = 1.0 / (radius * radius);
    for y in y0..y1 {
        for x in x0..x1 {
            let d = (Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - at).length_squared();
            let opacity = (1.0 - d * inv).clamp(0.0, 1.0) * strength;
            let pixel = layer.get_pixel_mut(x, y);
            for channel in 0..3 {
                pixel[channel] = (pixel[channel] as f32 * (1.0 - opacity)
                    + color[channel] as f32 * opacity) as u8;
            }
        }
    }
}
