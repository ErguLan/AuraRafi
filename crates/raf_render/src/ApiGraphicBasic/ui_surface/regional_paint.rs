//! CPU paint preparation reuse. All regions still merge into one draw list.
use super::{
    UiPaintKey, UiSurface, UiSurfaceDrawList, UiSurfaceFrame, UiSurfacePaintCommand, UiTextAtlas,
};
use raf_ui::{UiPerformanceMonitor, UiWorkCause, UiWorkPhase};
use std::collections::HashMap;
use std::time::Instant;

const MAX_PAINT_REGIONS: usize = 64;
const MAX_PAINT_NODES: usize = 4096;
const MAX_PAINT_TEXT_BYTES: usize = 4 * 1024 * 1024;

struct CachedPaintRegion {
    id: String,
    frame: UiSurfaceFrame,
    values: Vec<String>,
    slots: Vec<Option<super::UiTextAtlasRect>>,
    scale: u32,
    list: UiSurfaceDrawList,
    text_bytes: usize,
}

#[derive(Default)]
pub(super) struct UiRegionalPaintCache {
    entries: Vec<CachedPaintRegion>,
}

impl UiRegionalPaintCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn build(
        &mut self,
        surface: &UiSurface,
        frame: &UiSurfaceFrame,
        atlas: &UiTextAtlas,
        scale: f32,
        values: &[String],
        monitor: &mut UiPerformanceMonitor,
    ) -> UiSurfaceDrawList {
        let positions = frame
            .layout_boxes
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id.as_str(), i))
            .collect::<HashMap<_, _>>();
        let mut starts = vec![(0, surface.root.id.as_str())];
        if surface.root.layout.flow != raf_ui::UiFlow::None
            || surface
                .root
                .children
                .iter()
                .any(|node| node.layout.position_mode != raf_ui::UiPositionMode::Absolute)
        {
            self.clear();
            return UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
                frame, atlas, scale, values,
            );
        }
        for child in &surface.root.children {
            if let Some(&position) = positions.get(child.id.as_str()) {
                starts.push((position, child.id.as_str()));
            }
        }
        if let Some(&position) = positions.get("__rafui.tooltip") {
            starts.push((position, "__rafui.tooltip"));
        }
        starts.sort_unstable_by_key(|(position, _)| *position);
        starts.dedup_by_key(|(position, _)| *position);
        self.entries
            .retain(|entry| starts.iter().any(|(_, id)| *id == entry.id));
        let request_positions = frame
            .text_requests
            .iter()
            .enumerate()
            .map(|(i, request)| (request.node_id.as_str(), i))
            .collect::<HashMap<_, _>>();
        let mut merged = UiSurfaceDrawList {
            atlas_size: atlas.size(),
            ..Default::default()
        };
        for (index, &(start, id)) in starts.iter().enumerate() {
            let started = Instant::now();
            let end = starts
                .get(index + 1)
                .map_or(frame.layout_boxes.len(), |(end, _)| *end);
            let boxes = frame.layout_boxes[start..end].to_vec();
            let has_tooltip = boxes.iter().any(|b| b.id == "__rafui.tooltip");
            let mut requests = Vec::new();
            let mut resolved = Vec::new();
            for node in &boxes {
                if let Some(&position) = request_positions.get(node.id.as_str()) {
                    requests.push(frame.text_requests[position].clone());
                    resolved.push(values.get(position).cloned().unwrap_or_default());
                }
            }
            let slots = requests
                .iter()
                .zip(&resolved)
                .map(|(request, value)| atlas.slot_for(request, value).map(|slot| slot.rect))
                .collect::<Vec<_>>();
            let text_bytes = resolved
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_mul(3);
            let region = UiSurfaceFrame {
                tokens: frame.tokens,
                layout_boxes: boxes,
                hit_regions: Vec::new(),
                text_requests: requests,
                focus_order: Vec::new(),
                tooltip_anchor: frame.tooltip_anchor.filter(|_| has_tooltip),
                scroll_metrics: Vec::new(),
            };
            let cached = self.entries.iter().find(|entry| entry.id == id);
            let hit = cached.filter(|entry| {
                entry.frame == region
                    && entry.values == resolved
                    && entry.slots == slots
                    && entry.scale == scale.to_bits()
                    && entry.list.atlas_size == atlas.size()
            });
            let reused = hit.is_some();
            let list = hit.map(|entry| entry.list.clone()).unwrap_or_else(|| {
                UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
                    &region, atlas, scale, &resolved,
                )
            });
            super::cost_observation::observe(
                monitor,
                id,
                UiWorkPhase::Paint,
                if cached.is_some_and(|entry| {
                    entry.slots != slots || entry.list.atlas_size != atlas.size()
                }) {
                    UiWorkCause::Resources
                } else {
                    UiWorkCause::Visual
                },
                started.elapsed(),
                region.layout_boxes.len(),
                0,
                reused,
            );
            if !reused {
                self.entries.retain(|entry| entry.id != id);
                if region.layout_boxes.len() <= MAX_PAINT_NODES
                    && text_bytes <= MAX_PAINT_TEXT_BYTES
                {
                    while self.entries.len() >= MAX_PAINT_REGIONS
                        || self
                            .entries
                            .iter()
                            .map(|e| e.frame.layout_boxes.len())
                            .sum::<usize>()
                            .saturating_add(region.layout_boxes.len())
                            > MAX_PAINT_NODES
                        || self
                            .entries
                            .iter()
                            .map(|e| e.text_bytes)
                            .sum::<usize>()
                            .saturating_add(text_bytes)
                            > MAX_PAINT_TEXT_BYTES
                    {
                        self.entries.remove(0);
                    }
                    self.entries.push(CachedPaintRegion {
                        id: id.into(),
                        frame: region,
                        values: resolved,
                        slots,
                        scale: scale.to_bits(),
                        list: list.clone(),
                        text_bytes,
                    });
                }
            }
            append(&mut merged, list);
        }
        merged
            .paint_order
            .sort_by_key(|command| (command.z_index(), command.sequence()));
        merged
    }
}

fn append(target: &mut UiSurfaceDrawList, source: UiSurfaceDrawList) {
    let offsets = [
        target.solids.len(),
        target.strokes.len(),
        target.text.len(),
        target.images.len(),
    ];
    let sequence_offset = target.paint_order.len() as u32;
    for (key, effects) in source.effects {
        let key = match key {
            UiPaintKey::Solid(i) => UiPaintKey::Solid(i + offsets[0]),
            UiPaintKey::Stroke(i) => UiPaintKey::Stroke(i + offsets[1]),
            UiPaintKey::Text(i) => UiPaintKey::Text(i + offsets[2]),
            UiPaintKey::Image(i) => UiPaintKey::Image(i + offsets[3]),
        };
        target.effects.insert(key, effects);
    }
    target.solids.extend(source.solids);
    target.strokes.extend(source.strokes);
    target.text.extend(source.text);
    target.images.extend(source.images);
    for command in source.paint_order {
        target.paint_order.push(match command {
            UiSurfacePaintCommand::Solid {
                index,
                z_index,
                sequence,
            } => UiSurfacePaintCommand::Solid {
                index: index + offsets[0],
                z_index,
                sequence: sequence + sequence_offset,
            },
            UiSurfacePaintCommand::Stroke {
                index,
                z_index,
                sequence,
            } => UiSurfacePaintCommand::Stroke {
                index: index + offsets[1],
                z_index,
                sequence: sequence + sequence_offset,
            },
            UiSurfacePaintCommand::Text {
                index,
                z_index,
                sequence,
            } => UiSurfacePaintCommand::Text {
                index: index + offsets[2],
                z_index,
                sequence: sequence + sequence_offset,
            },
            UiSurfacePaintCommand::Image {
                index,
                z_index,
                sequence,
            } => UiSurfacePaintCommand::Image {
                index: index + offsets[3],
                z_index,
                sequence: sequence + sequence_offset,
            },
        });
    }
}
