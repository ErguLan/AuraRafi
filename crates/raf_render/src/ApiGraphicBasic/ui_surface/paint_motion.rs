//! Paint-only retained UI state and short interaction transitions.

use std::collections::{HashMap, HashSet};

use raf_ui::{UiControlState, UiFocusState, UiNode, UiStyle, UiTween, UiVisualState};

use super::UiSurface;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UiSurfacePaintStateKey {
    focus: UiFocusState,
    controls: UiControlState,
    tooltip_motion_bits: u32,
    paint_motion_revision: u64,
    tooltip_pointer_bits: Option<[u32; 2]>,
    reduced_motion: bool,
}

impl UiSurfacePaintStateKey {
    pub fn new(surface: &UiSurface, session: &super::UiSurfaceSession) -> Self {
        let hovered_has_tooltip = surface.retained_tooltips
            && session
                .interaction
                .focus
                .hovered
                .as_deref()
                .and_then(|id| surface.root.find(id))
                .is_some_and(|node| {
                    !node.disabled
                        && node.interactive
                        && (node.tooltip_key.is_some() || node.tooltip_value.is_some())
                });
        let tooltip_pointer_bits = hovered_has_tooltip
            .then(|| session.interaction.pointer_position())
            .flatten()
            .map(|position| [position[0].to_bits(), position[1].to_bits()]);

        Self {
            focus: session.interaction.focus.clone(),
            controls: session.interaction.controls.layout_snapshot(),
            tooltip_motion_bits: session.transient_motion_revision(),
            paint_motion_revision: session.paint_motion.revision(),
            tooltip_pointer_bits,
            reduced_motion: session.reduced_motion,
        }
    }
}

#[derive(Debug, Clone)]
struct StyleTransition {
    from: UiStyle,
    target: UiStyle,
    tween: UiTween,
    state_key: u8,
    base_style: UiStyle,
}

#[derive(Debug, Clone, Default)]
pub(super) struct UiPaintMotion {
    transitions: HashMap<String, StyleTransition>,
    revision: u64,
    #[cfg(test)]
    target_resolutions: usize,
}

impl UiPaintMotion {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn has_active_motion(&self) -> bool {
        self.transitions
            .values()
            .any(|transition| !transition.tween.is_settled())
    }

    pub fn advance(&mut self, delta_seconds: f32, reduced_motion: bool) {
        let mut changed = false;
        for transition in self.transitions.values_mut() {
            if transition.tween.is_settled() {
                continue;
            }
            let previous = transition.tween.value();
            let current = transition.tween.advance(delta_seconds, reduced_motion);
            changed |= previous.to_bits() != current.to_bits();
        }
        if changed {
            self.bump_revision();
        }
    }

    pub fn reset(&mut self) {
        if !self.transitions.is_empty() {
            self.transitions.clear();
            self.bump_revision();
        }
    }

    pub fn snap_to_current_styles(&mut self, surface: &UiSurface, visual_state: UiVisualState<'_>) {
        let mut styles = HashMap::new();
        collect_target_styles(
            &surface.root,
            &surface.style_sheet,
            visual_state,
            &mut styles,
        );
        self.transitions = styles
            .into_iter()
            .map(|(id, (style, state_key, base_style))| {
                let mut tween = UiTween::default();
                tween.set_immediate(1.0);
                (
                    id,
                    StyleTransition {
                        from: style.clone(),
                        target: style,
                        tween,
                        state_key,
                        base_style,
                    },
                )
            })
            .collect();
        self.bump_revision();
    }

    pub fn resolve_styles(
        &mut self,
        surface: &UiSurface,
        visual_state: UiVisualState<'_>,
        delta_seconds: f32,
        reduced_motion: bool,
    ) -> HashMap<String, UiStyle> {
        let mut live_ids = HashSet::new();
        let mut styles = HashMap::new();
        self.resolve_node(
            &surface.root,
            &surface.style_sheet,
            visual_state,
            delta_seconds,
            reduced_motion,
            &mut live_ids,
            &mut styles,
        );
        let original_len = self.transitions.len();
        self.transitions.retain(|id, _| live_ids.contains(id));
        if self.transitions.len() != original_len {
            self.bump_revision();
        }
        styles
    }

    fn resolve_node(
        &mut self,
        node: &UiNode,
        style_sheet: &raf_ui::UiStyleSheet,
        visual_state: UiVisualState<'_>,
        delta_seconds: f32,
        reduced_motion: bool,
        live_ids: &mut HashSet<String>,
        styles: &mut HashMap<String, UiStyle>,
    ) {
        if node.hidden {
            return;
        }

        live_ids.insert(node.id.clone());
        let state_key = visual_state.style_state_key(node);
        let target = if let Some(cached) = self
            .transitions
            .get(&node.id)
            .filter(|cached| cached.state_key == state_key && cached.base_style == node.style)
        {
            cached.target.clone()
        } else {
            #[cfg(test)]
            {
                self.target_resolutions += 1;
            }
            style_sheet.resolve_with_state(node, visual_state)
        };
        let transition = self.transitions.entry(node.id.clone()).or_insert_with(|| {
            let mut tween = UiTween::default();
            tween.set_immediate(1.0);
            StyleTransition {
                from: target.clone(),
                target: target.clone(),
                tween,
                state_key,
                base_style: node.style.clone(),
            }
        });
        transition.state_key = state_key;
        transition.base_style = node.style.clone();
        let mut changed = false;

        if transition.target != target {
            let current = interpolate_style(
                &transition.from,
                &transition.target,
                transition.tween.value(),
            );
            transition.from = current;
            transition.target = target;
            transition.tween.set_immediate(0.0);
            transition.tween.set_target(1.0);
            changed = true;
        }

        let previous_progress = transition.tween.value();
        let progress = transition.tween.advance(delta_seconds, reduced_motion);
        if progress.to_bits() != previous_progress.to_bits() {
            changed = true;
        }
        let style = interpolate_style(&transition.from, &transition.target, progress);
        styles.insert(node.id.clone(), style);
        if changed {
            self.bump_revision();
        }

        for child in &node.children {
            self.resolve_node(
                child,
                style_sheet,
                visual_state,
                delta_seconds,
                reduced_motion,
                live_ids,
                styles,
            );
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

pub(super) fn layout_control_snapshot(
    _surface: &UiSurface,
    controls: &UiControlState,
) -> UiControlState {
    controls.geometry_snapshot()
}

fn interpolate_style(from: &UiStyle, target: &UiStyle, progress: f32) -> UiStyle {
    let progress = progress.clamp(0.0, 1.0);
    let blend_color = |from: [u8; 4], target: [u8; 4]| {
        std::array::from_fn(|index| {
            (f32::from(from[index])
                + (f32::from(target[index]) - f32::from(from[index])) * progress)
                .round()
                .clamp(0.0, 255.0) as u8
        })
    };
    let blend_float = |from: f32, target: f32| from + (target - from) * progress;

    UiStyle {
        fill: blend_color(from.fill, target.fill),
        border: blend_color(from.border, target.border),
        text: blend_color(from.text, target.text),
        border_width: blend_float(from.border_width, target.border_width),
        radius: blend_float(from.radius, target.radius),
        opacity: blend_float(from.opacity, target.opacity).clamp(0.0, 1.0),
    }
}

fn collect_target_styles(
    node: &UiNode,
    style_sheet: &raf_ui::UiStyleSheet,
    visual_state: UiVisualState<'_>,
    styles: &mut HashMap<String, (UiStyle, u8, UiStyle)>,
) {
    if node.hidden {
        return;
    }
    styles.insert(
        node.id.clone(),
        (
            style_sheet.resolve_with_state(node, visual_state),
            visual_state.style_state_key(node),
            node.style.clone(),
        ),
    );
    for child in &node.children {
        collect_target_styles(child, style_sheet, visual_state, styles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rafui_interaction_animation_reuses_unaffected_style_targets() {
        let root = (0..128).fold(
            raf_ui::UiNode::new("root", raf_ui::UiNodeKind::Root),
            |root, i| {
                root.with_child(raf_ui::UiNode::new(
                    format!("button-{i}"),
                    raf_ui::UiNodeKind::Button,
                ))
            },
        );
        let mut surface = UiSurface::new("styles", raf_ui::StudioUiPalette::IndustrialDark, root);
        let mut motion = UiPaintMotion::default();
        motion.snap_to_current_styles(&surface, UiVisualState::default());
        let hover = UiVisualState {
            hovered_id: Some("button-8"),
            ..Default::default()
        };
        let styles = motion.resolve_styles(&surface, hover, 0.0, false);
        assert_eq!(styles.len(), 129);
        assert_eq!(motion.target_resolutions, 1);
        for _ in 0..12 {
            motion.advance(0.016, false);
            motion.resolve_styles(&surface, hover, 0.0, false);
        }
        assert_eq!(motion.target_resolutions, 1);
        surface.root.find_mut("button-0").unwrap().disabled = true;
        motion.resolve_styles(&surface, hover, 0.0, false);
        assert_eq!(motion.target_resolutions, 2);
        let focused = UiVisualState {
            focused_id: Some("button-5"),
            ..hover
        };
        motion.resolve_styles(&surface, focused, 0.0, false);
        assert_eq!(motion.target_resolutions, 3);
    }
}
