//! Retains independent absolute regions while preserving one ordered frame.
//! Flow-dependent roots deliberately use the ordinary whole-layout path.

use super::{render, UiSurfaceFrame};
use raf_ui::{
    UiControlState, UiFlow, UiNode, UiPerformanceMonitor, UiPositionMode, UiSpacing, UiStyleSheet,
    UiVisualState, UiWorkCause, UiWorkPhase,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

const MAX_CACHED_REGIONS: usize = 64;
const MAX_CACHED_NODES: usize = 4096;
const MAX_CACHED_TEXT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
struct RegionKey {
    node: UiNode,
    controls: UiControlState,
    visual_states: Vec<u8>,
    intrinsic: Vec<Option<[f32; 2]>>,
    viewport: [u32; 2],
    text_scale: u32,
    parent_z: i16,
}

#[derive(Debug, Clone)]
struct CachedRegion {
    measured: bool,
    key: RegionKey,
    rules: Arc<UiStyleSheet>,
    frame: UiSurfaceFrame,
    nodes: usize,
    text_bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct UiRegionalLayoutCache {
    entries: Vec<CachedRegion>,
    rules: Arc<UiStyleSheet>,
}

fn state_inputs(
    node: &UiNode,
    visual: UiVisualState<'_>,
    sizes: &HashMap<String, [f32; 2]>,
    states: &mut Vec<u8>,
    intrinsic: &mut Vec<Option<[f32; 2]>>,
    bytes: &mut usize,
) {
    states.push(visual.style_state_key(node));
    if node.control.scroll_axis().is_some() {
        for suffix in ["track", "thumb"] {
            let id = format!("__rafui.scrollbar.{}.{suffix}", node.id);
            states.push(
                u8::from(visual.hovered_id == Some(id.as_str()))
                    | (u8::from(visual.active_id == Some(id.as_str())) << 1),
            );
        }
    }
    intrinsic.push(sizes.get(&node.id).copied());
    *bytes = bytes
        .saturating_add(node.text_value.as_ref().map_or(0, String::len))
        .saturating_add(node.text_key.as_ref().map_or(0, String::len))
        .saturating_add(node.tooltip_value.as_ref().map_or(0, String::len));
    for child in &node.children {
        state_inputs(child, visual, sizes, states, intrinsic, bytes);
    }
}

impl UiRegionalLayoutCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn build(
        &mut self,
        root: &UiNode,
        rules: &UiStyleSheet,
        visual: UiVisualState<'_>,
        controls: &UiControlState,
        width: u32,
        height: u32,
        clear: [u8; 4],
        text_scale: f32,
        sizes: &HashMap<String, [f32; 2]>,
        monitor: &mut UiPerformanceMonitor,
    ) -> UiSurfaceFrame {
        let independent = !root.hidden
            && root.kind == raf_ui::UiNodeKind::Root
            && root.layout == raf_ui::UiLayout::fill(UiFlow::None)
            && root.layout.rect.is_none()
            && root.layout.padding == UiSpacing::ZERO
            && root.control == raf_ui::UiControl::None
            && !root.visual.clip_children
            && root.children.iter().all(|n| {
                n.layout.position_mode == UiPositionMode::Absolute && n.layout.rect.is_some()
            });
        if !independent || root.children.is_empty() {
            self.clear();
            let start = Instant::now();
            let frame = render::build_surface_frame_with_intrinsic_sizes(
                root, rules, visual, controls, width, height, clear, text_scale, sizes,
            );
            super::cost_observation::observe(
                monitor,
                &root.id,
                if sizes.is_empty() {
                    UiWorkPhase::Layout
                } else {
                    UiWorkPhase::IntrinsicLayout
                },
                UiWorkCause::Geometry,
                start.elapsed(),
                frame.layout_boxes.len(),
                0,
                false,
            );
            return frame;
        }
        if self.rules.as_ref() != rules {
            self.rules = Arc::new(rules.clone());
        }
        self.entries.retain(|entry| {
            root.children
                .iter()
                .any(|node| node.id == entry.key.node.id && !node.hidden)
        });
        let mut shell = root.clone();
        shell.children.clear();
        let mut frame = render::build_surface_frame_with_intrinsic_sizes(
            &shell, rules, visual, controls, width, height, clear, text_scale, sizes,
        );
        let parent = frame.layout_boxes[0].content_rect;
        let parent_z = frame.layout_boxes[0].z_index;
        let measured = !sizes.is_empty();
        for node in root.children.iter().filter(|node| !node.hidden) {
            let start = Instant::now();
            let scoped = controls.scoped_snapshot(node);
            let mut states = Vec::new();
            let mut intrinsic = Vec::new();
            let mut text_bytes = 0;
            state_inputs(
                node,
                visual,
                sizes,
                &mut states,
                &mut intrinsic,
                &mut text_bytes,
            );
            text_bytes = text_bytes
                .saturating_add(scoped.retained_text_bytes())
                .saturating_mul(3);
            let phase = if measured {
                UiWorkPhase::IntrinsicLayout
            } else {
                UiWorkPhase::Layout
            };
            let cached = self
                .entries
                .iter()
                .find(|entry| entry.measured == measured && entry.key.node.id == node.id);
            let cause = match cached {
                None => UiWorkCause::Initial,
                Some(entry) if entry.key.node != *node || entry.rules.as_ref() != rules => {
                    UiWorkCause::Document
                }
                Some(entry) if !entry.key.controls.has_same_text(&scoped) => UiWorkCause::Text,
                Some(entry)
                    if entry.key.controls.geometry_snapshot() != scoped.geometry_snapshot() =>
                {
                    UiWorkCause::Scroll
                }
                Some(entry) if entry.key.controls != scoped => UiWorkCause::Visual,
                Some(entry) if entry.key.visual_states != states => UiWorkCause::Visual,
                _ => UiWorkCause::Geometry,
            };
            let hit = cached.filter(|entry| {
                entry.key.node == *node
                    && entry.rules.as_ref() == rules
                    && entry.key.controls == scoped
                    && entry.key.visual_states == states
                    && entry.key.intrinsic == intrinsic
                    && entry.key.viewport == [width, height]
                    && entry.key.text_scale == text_scale.to_bits()
                    && entry.key.parent_z == parent_z
            });
            let region = if let Some(entry) = hit {
                entry.frame.clone()
            } else {
                let mut region = UiSurfaceFrame {
                    tokens: frame.tokens,
                    layout_boxes: Vec::new(),
                    hit_regions: Vec::new(),
                    text_requests: Vec::new(),
                    focus_order: Vec::new(),
                    tooltip_anchor: None,
                    scroll_metrics: Vec::new(),
                };
                render::record_node(
                    node,
                    rules,
                    visual,
                    controls,
                    parent,
                    parent,
                    -0.02,
                    parent_z,
                    &mut region.layout_boxes,
                    &mut region.hit_regions,
                    &mut region.text_requests,
                    &mut region.focus_order,
                    &mut region.scroll_metrics,
                    text_scale,
                    sizes,
                );
                region
            };
            let reused = hit.is_some();
            let nodes = region.layout_boxes.len();
            let elapsed = start.elapsed();
            if monitor.record(&node.id, phase, cause, elapsed, nodes, 0, reused) {
                tracing::warn!(region = %node.id, ?phase, ?cause, cpu_us = elapsed.as_micros() as u64, nodes, reused, "Retained UI cost exceeded its advisory budget");
            }
            if !reused {
                self.entries
                    .retain(|entry| !(entry.measured == measured && entry.key.node.id == node.id));
                // Bounds apply to retained copies, not to the authored document.
                if nodes <= MAX_CACHED_NODES && text_bytes <= MAX_CACHED_TEXT_BYTES {
                    while self.entries.len() >= MAX_CACHED_REGIONS
                        || self
                            .entries
                            .iter()
                            .map(|e| e.nodes)
                            .sum::<usize>()
                            .saturating_add(nodes)
                            > MAX_CACHED_NODES
                        || self
                            .entries
                            .iter()
                            .map(|e| e.text_bytes)
                            .sum::<usize>()
                            .saturating_add(text_bytes)
                            > MAX_CACHED_TEXT_BYTES
                    {
                        self.entries.remove(0);
                    }
                    self.entries.push(CachedRegion {
                        measured,
                        key: RegionKey {
                            node: node.clone(),
                            controls: scoped,
                            visual_states: states,
                            intrinsic,
                            viewport: [width, height],
                            text_scale: text_scale.to_bits(),
                            parent_z,
                        },
                        rules: Arc::clone(&self.rules),
                        frame: region.clone(),
                        nodes,
                        text_bytes,
                    });
                }
            }
            frame.layout_boxes.extend(region.layout_boxes);
            frame.hit_regions.extend(region.hit_regions);
            frame.text_requests.extend(region.text_requests);
            frame.focus_order.extend(region.focus_order);
            frame.scroll_metrics.extend(region.scroll_metrics);
        }
        render::resolve_inherited_text(&mut frame, root);
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::super::{UiSurface, UiSurfaceDrawList, UiTextAtlas};
    use super::*;
    use raf_ui::{UiLayout, UiNodeKind, UiRect, UiScrollAxis, UiStyle};

    #[test]
    fn ui_cost_guard_regions_preserve_paint_order_and_reuse_neighbours() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_child(
                UiNode::scroll_view("left", UiScrollAxis::Vertical)
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        ..UiLayout::absolute(UiRect::new(0.0, 0.0, 90.0, 70.0))
                    })
                    .with_child(
                        UiNode::new("left.content", UiNodeKind::Label)
                            .with_text_value("First")
                            .with_layout(UiLayout::fixed(80.0, 200.0)),
                    ),
            )
            .with_child(
                UiNode::new("right", UiNodeKind::Panel)
                    .with_text_value("Neighbour")
                    .with_icon(raf_ui::UiIcon::new(raf_ui::UiIconId::Cube))
                    .with_style(UiStyle {
                        fill: [20, 30, 40, 255],
                        radius: 5.0,
                        ..UiStyle::transparent()
                    })
                    .with_layout(UiLayout::absolute(UiRect::new(100.0, 0.0, 90.0, 70.0))),
            );
        let mut surface = UiSurface::new("regions", raf_ui::StudioUiPalette::IndustrialDark, root);
        surface.retained_tooltips = false;
        let mut controls = UiControlState::default();
        let mut layout = UiRegionalLayoutCache::default();
        let mut paint = super::super::regional_paint::UiRegionalPaintCache::default();
        let mut monitor = UiPerformanceMonitor::default();
        let mut atlas = UiTextAtlas::default();
        for scroll in [0.0, 12.0] {
            controls.set_scroll_metrics("left", [0.0, 200.0]);
            controls.set_scroll_offset("left", [0.0, scroll]);
            let actual = layout.build(
                &surface.root,
                &surface.style_sheet,
                UiVisualState::default(),
                &controls,
                200,
                100,
                [0; 4],
                1.0,
                &HashMap::new(),
                &mut monitor,
            );
            let expected = render::build_surface_frame_with_intrinsic_sizes(
                &surface.root,
                &surface.style_sheet,
                UiVisualState::default(),
                &controls,
                200,
                100,
                [0; 4],
                1.0,
                &HashMap::new(),
            );
            assert_eq!(actual, expected);
            let values = actual
                .text_requests
                .iter()
                .map(|request| request.text_key.clone())
                .collect::<Vec<_>>();
            atlas.sync(&actual.text_requests);
            let actual_paint = paint.build(&surface, &actual, &atlas, 1.0, &values, &mut monitor);
            let expected_paint = UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
                &expected, &atlas, 1.0, &values,
            );
            assert_eq!(actual_paint, expected_paint);
        }
        assert!(
            monitor
                .snapshot()
                .regions
                .iter()
                .find(|region| region.id == "right")
                .unwrap()
                .reuses
                >= 2
        );
        surface.style_sheet.rules.push(raf_ui::UiStyleRule::new(
            raf_ui::UiStyleSelector::Id("right".into()),
            raf_ui::UiStylePatch {
                fill: Some([70, 10, 10, 255]),
                ..Default::default()
            },
        ));
        let changed = layout.build(
            &surface.root,
            &surface.style_sheet,
            UiVisualState::default(),
            &controls,
            200,
            100,
            [0; 4],
            1.0,
            &HashMap::new(),
            &mut monitor,
        );
        assert_eq!(
            changed
                .layout_boxes
                .iter()
                .find(|b| b.id == "right")
                .unwrap()
                .style
                .fill,
            [70, 10, 10, 255]
        );
    }
}
