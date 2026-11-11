//! Opt-in authoring inspection. Never run this tree walk as an idle timer.
use super::{UiSurface, UiSurfaceFrame, UiSurfaceImageStore};
use raf_ui::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringIssueKind {
    AmbiguousChildFlow,
    InconsistentConstraints,
    InvalidAspectRatio,
    BelowMinimum,
    OverlappingFlowSiblings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAuthoringIssue {
    pub node: String,
    pub related_node: Option<String>,
    pub kind: UiAuthoringIssueKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiImageInspection {
    pub source: String,
    /// Absent when the caller did not provide the owning resource store.
    pub status: Option<UiImageResourceStatus>,
    pub options: UiImageOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNodeInspection {
    pub id: String,
    pub parent: Option<String>,
    pub classes: Vec<String>,
    pub declared_layout: UiLayout,
    pub rect: UiRect,
    pub content_rect: UiRect,
    pub clip_rect: UiRect,
    pub resolved_style: UiStyle,
    pub text_style: Option<UiTextStyle>,
    pub image: Option<UiImageInspection>,
    /// Selectors match this node; each rule still carries its state predicate.
    pub candidate_rules: Vec<UiStyleRule>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiSurfaceInspection {
    pub nodes: Vec<UiNodeInspection>,
    pub issues: Vec<UiAuthoringIssue>,
    pub truncated: bool,
}

/// Inspects an already compiled frame. `max_nodes` bounds captured entries and
/// traversal. It does not rebuild layout, resolve files, or expose edit drafts.
pub fn inspect_surface(
    surface: &UiSurface,
    frame: &UiSurfaceFrame,
    images: Option<&UiSurfaceImageStore>,
    max_nodes: usize,
) -> UiSurfaceInspection {
    let boxes = frame
        .layout_boxes
        .iter()
        .map(|b| (b.id.as_str(), b))
        .collect::<HashMap<_, _>>();
    let mut report = UiSurfaceInspection::default();
    let mut stack = vec![(&surface.root, None)];
    let mut visited = 0;
    while let Some((node, parent)) = stack.pop() {
        if visited >= max_nodes {
            report.truncated = true;
            break;
        }
        visited += 1;
        if node.hidden {
            continue;
        }
        let Some(layout) = boxes.get(node.id.as_str()) else {
            continue;
        };
        let mut issue = |kind, related_node| {
            report.issues.push(UiAuthoringIssue {
                node: node.id.clone(),
                related_node,
                kind,
            })
        };
        let capacity = max_nodes.saturating_sub(visited + stack.len());
        let children = node.children.iter().take(capacity).collect::<Vec<_>>();
        report.truncated |= children.len() < node.children.len();
        let flow_children = children
            .iter()
            .copied()
            .filter(|child| {
                !child.hidden
                    && child.layout.position_mode == UiPositionMode::Flow
                    && child.layout.rect.is_none()
                    && child.layout.anchored.is_none()
            })
            .collect::<Vec<_>>();
        if node.kind == UiNodeKind::Panel
            && node.layout.flow == UiFlow::None
            && flow_children.len() > 1
        {
            issue(UiAuthoringIssueKind::AmbiguousChildFlow, None);
        }
        if (0..2).any(|axis| {
            node.layout.max_size[axis] > 0.0
                && node.layout.min_size[axis] > node.layout.max_size[axis]
        }) {
            issue(UiAuthoringIssueKind::InconsistentConstraints, None);
        }
        if node
            .layout
            .aspect_ratio
            .is_some_and(|ratio| !ratio.is_finite() || ratio <= 0.0)
        {
            issue(UiAuthoringIssueKind::InvalidAspectRatio, None);
        }
        if layout.rect.width + 0.5 < node.layout.min_size[0]
            || layout.rect.height + 0.5 < node.layout.min_size[1]
        {
            issue(UiAuthoringIssueKind::BelowMinimum, None);
        }
        if node.layout.compact == UiCompactMode::None
            && node.layout.responsive.is_empty()
            && matches!(node.layout.flow, UiFlow::Row | UiFlow::Column)
        {
            let row = node.layout.flow == UiFlow::Row;
            for pair in flow_children.windows(2) {
                if let (Some(a), Some(b)) = (
                    boxes.get(pair[0].id.as_str()),
                    boxes.get(pair[1].id.as_str()),
                ) {
                    let overlap = if row {
                        a.rect.right() > b.rect.x + 0.5
                    } else {
                        a.rect.bottom() > b.rect.y + 0.5
                    };
                    if overlap {
                        issue(
                            UiAuthoringIssueKind::OverlappingFlowSiblings,
                            Some(pair[1].id.clone()),
                        );
                    }
                }
            }
        }
        let image = match &node.control {
            UiControl::Image(image) => Some(UiImageInspection {
                source: image.source.key.clone(),
                status: images.map(|store| store.resource_status(&image.source.key)),
                options: image.options.clone(),
            }),
            _ => None,
        };
        report.nodes.push(UiNodeInspection {
            id: node.id.clone(),
            parent: parent.map(str::to_owned),
            classes: node.classes.clone(),
            declared_layout: node.layout.clone(),
            rect: layout.rect,
            content_rect: layout.content_rect,
            clip_rect: layout.clip_rect,
            resolved_style: layout.style.clone(),
            text_style: layout.text_style,
            image,
            candidate_rules: surface
                .style_sheet
                .rules
                .iter()
                .filter(|rule| rule.selector.matches(node))
                .cloned()
                .collect(),
        });
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, Some(node.id.as_str()))),
        );
    }
    report
}
