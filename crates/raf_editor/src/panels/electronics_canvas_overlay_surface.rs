//! Retained RafUI labels for the native Electronics canvas.
//!
//! ApiGraphicBasic owns vector CAD geometry and symbol strokes. Text labels
//! remain a RafUI overlay so they use the same compositor as editor chrome.
//! The CAD controller owns drawing and minimap navigation. Only the explicit
//! minimap fold button routes retained input through the overlay host.

use glam::Vec2;
use raf_core::{i18n, Language};
use raf_electronics::{CadObject, CadObjectKind, CadSurfaceKind};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiIcon, UiIconId, UiIconSize, UiImage, UiImageFit,
    UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiSizeMode,
    UiSpacing, UiStyle, UiTextOverflow, UiTextRole, UiTextStyle, UiTokens,
};

use crate::editor_layout::EditorRect;
#[path = "electronics_pcb_labels.rs"]
mod pcb_labels;
use crate::electronics_controller::{
    ElectronicsSelectionKind, ElectronicsTool, NativeElectronicsEditor,
};
use crate::electronics_minimap::{
    self, minimap_tooltip_key, surface_label_key, HEADER_HEIGHT, IMAGE_KEY as MINIMAP_IMAGE_KEY,
};
use crate::panels::electronics_surface::{
    with_alpha, ELECTRONICS_BORDER_WIDTH, ELECTRONICS_CORNER_RADIUS, ELECTRONICS_ROW_MIN_TRACK,
};

const LABEL_HEIGHT: f32 = 18.0;
const LABEL_GAP: f32 = 4.0;
/// Upper bound of a canvas label track. Text longer than this is ellipsized
/// instead of being drawn outside its own box.
const MAX_LABEL_WIDTH: f32 = 180.0;
/// Font size of a component label, in logical points.
const COMPONENT_LABEL_SIZE: f32 = 12.0;
/// Font size of a pin, pad or net label, in logical points.
const DETAIL_LABEL_SIZE: f32 = 10.0;
/// Share of the nominal font size one label character is expected to advance.
/// Measured from the bundled Ubuntu font: a 10px run of designators averages
/// 0.58em and a 12px bold run of component designators averages 0.61em. The
/// atlas remains the authority; an estimate that is too small only ellipsizes.
const LABEL_ADVANCE_RATIO: f32 = 0.61;
/// Horizontal breathing room reserved inside a label track, in logical points.
const LABEL_TRACK_INSET: f32 = 8.0;
/// Size of the DRC severity glyph beside a marker label, in logical points.
const SEVERITY_ICON_SIZE: f32 = 12.0;
/// Width reserved for the DRC severity glyph, in logical points.
const SEVERITY_ICON_TRACK: f32 = SEVERITY_ICON_SIZE + 2.0;
const COMPONENT_Z_INDEX: i16 = 0;
const COMPONENT_PREVIEW_Z_INDEX: i16 = 4;
const MINIMAP_Z_INDEX: i16 = 5;
const MINIMAP_HEADER_Z_INDEX: i16 = 6;
const MINIMAP_CONTENT_Z_INDEX: i16 = 7;
const LABEL_Z_INDEX: i16 = 10;
const HINT_Z_INDEX: i16 = 20;
const MINIMAP_PAD: f32 = 6.0;
const MINIMAP_HEADER_PAD: f32 = 8.0;
/// Horizontal gap between the minimap title and the surface badge.
const MINIMAP_TITLE_GAP: f32 = 6.0;
const BADGE_HEIGHT: f32 = 16.0;
const BADGE_MIN_WIDTH: f32 = 34.0;
/// Maximum share of the header the surface badge may take.
const BADGE_MAX_WIDTH_RATIO: f32 = 0.42;
/// Height of the canvas tool hint, in logical points.
const HINT_HEIGHT: f32 = 28.0;
/// Body font size of the canvas tool hint, in logical points.
const HINT_BODY_SIZE: f32 = 11.0;
/// Inset of the canvas tool hint from the canvas edges, in logical points.
const HINT_INSET: f32 = 12.0;
/// Maximum share of the canvas width the tool hint may take.
///
/// The hint is a single floating line, so the bound is generous: the longest
/// localized copy measures 297 points (Spanish select hint, Ubuntu Regular at
/// 11px) and the narrowest canvas the Electronics layout allows is 360 points.
/// At this ratio the available text track is 302 points, so the copy still
/// fits without an ellipsis at the documented minimum canvas width.
const HINT_MAX_WIDTH_RATIO: f32 = 0.86;

/// Severity of a design-rule marker, derived from the marker color.
///
/// `CadObject` only carries the severity inside `color_rgba`, so the overlay
/// recovers it from the exact palette the scene builder uses. The icon and the
/// localized prefix are what make severity readable without color; the mapping
/// is a presentation detail of this overlay and not a domain contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrcSeverity {
    Error,
    Warning,
    Info,
}

/// Color the CAD scene assigns to each DRC severity.
const DRC_ERROR_COLOR: [u8; 4] = [220, 66, 58, 255];
const DRC_WARNING_COLOR: [u8; 4] = [245, 180, 65, 255];
const DRC_INFO_COLOR: [u8; 4] = [170, 170, 178, 255];

/// Host-owned inputs the overlay needs to stay declarative and localized.
///
/// The workbench keeps the tween and the language; this surface only reads the
/// current values, so no overlay state is stored in the document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectronicsCanvasOverlayParams {
    /// 0..1 opacity of the canvas tool hint. A reduced-motion host resolves its
    /// tween to 1.0, which keeps the hint fully visible.
    pub hint_fade: f32,
    /// Language used for the composed design-rule marker text.
    pub language: Language,
}

impl ElectronicsCanvasOverlayParams {
    pub fn new(hint_fade: f32, language: Language) -> Self {
        Self {
            hint_fade: if hint_fade.is_finite() {
                hint_fade.clamp(0.0, 1.0)
            } else {
                1.0
            },
            language,
        }
    }
}

/// The rules occupy chrome outside the CAD input rectangle. Keep their paint
/// in the camera-dependent overlay so panning never rebuilds the workbench.
pub fn electronics_overlay_rect(pcb: bool, canvas: EditorRect) -> EditorRect {
    use crate::panels::electronics_desktop_surface::{RULER_HEIGHT, RULER_WIDTH};
    if pcb {
        EditorRect::new(
            canvas.x - RULER_WIDTH,
            canvas.y - RULER_HEIGHT,
            canvas.width + RULER_WIDTH,
            canvas.height + RULER_HEIGHT,
        )
    } else {
        canvas
    }
}

pub fn build_electronics_framed_overlay_surface(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    canvas: EditorRect,
    params: ElectronicsCanvasOverlayParams,
) -> UiSurface {
    let mut surface = build_electronics_canvas_overlay_surface(palette, editor, canvas, params);
    if editor.active_surface() == CadSurfaceKind::Pcb {
        use crate::panels::electronics_desktop_surface::{
            ruler_overlay, RULER_HEIGHT, RULER_WIDTH,
        };
        let content = surface.root;
        surface.root = UiNode::new("electronics.canvas.framed-overlay", UiNodeKind::Overlay)
            .with_layout(UiLayout {
                overflow: UiOverflow::Clip,
                ..UiLayout::fill(UiFlow::None)
            })
            .with_style(UiStyle::transparent())
            .with_child(ruler_overlay(
                editor,
                Vec2::new(canvas.width, canvas.height),
                crate::panels::electronics_surface::electronics_tokens(palette),
            ))
            .with_child(
                UiNode::new("electronics.canvas.framed-content", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        overflow: UiOverflow::Clip,
                        ..UiLayout::absolute(UiRect::new(
                            RULER_WIDTH,
                            RULER_HEIGHT,
                            canvas.width,
                            canvas.height,
                        ))
                    })
                    .with_style(UiStyle::transparent())
                    .with_child(content),
            );
    }
    surface
}

/// Builds only the labels that are visible in the current CAD camera.
///
/// The returned root is a canvas-local, clipped RafUI overlay. It is presented
/// into the same physical rectangle as the ApiGraphicBasic CAD surface, so
/// component artwork can never paint over the workbench chrome.
pub fn build_electronics_canvas_overlay_surface(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    canvas: EditorRect,
    params: ElectronicsCanvasOverlayParams,
) -> UiSurface {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let canvas_size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
    let camera = editor.camera();
    let selected = editor.selection();
    let mut root_layout = UiLayout::fill(UiFlow::None);
    root_layout.overflow = UiOverflow::Clip;
    let mut root = UiNode::new("electronics.canvas.labels", UiNodeKind::Overlay)
        .with_layout(root_layout)
        .with_style(UiStyle::transparent());

    // Schematic symbols and physical PCB geometry use vector CAD strokes.
    // Keep the legacy artwork fallback only for PCB placements without a
    // physical footprint; stretching a bitmap made schematic symbols blurry.
    let pcb_surface = editor.active_surface() == CadSurfaceKind::Pcb;
    for (index, object) in editor.scene().objects.iter().enumerate() {
        if !pcb_surface {
            continue;
        }
        if object.kind != CadObjectKind::Component {
            continue;
        }
        let Some(rect) = object.rect else {
            continue;
        };
        let is_placement_preview = object.id == "component-placement-preview";
        let asset_key = match object.source_id {
            Some(source_id) => {
                if pcb_surface {
                    if editor.pcb_has_physical_footprint(source_id) {
                        continue;
                    }
                    editor.pcb_artwork_key(source_id)
                } else {
                    continue;
                }
            }
            None if is_placement_preview => editor
                .placement_preview_asset_key()
                .unwrap_or("electronics://symbols/generic.png"),
            None => continue,
        };
        let screen_center = camera.screen_from_world(rect.center, canvas_size);
        let raw_size = rect.size.abs() * camera.zoom;
        if raw_size.x < 8.0 || raw_size.y < 8.0 {
            continue;
        }
        let image_size = Vec2::new(raw_size.x.clamp(16.0, 180.0), raw_size.y.clamp(16.0, 180.0));
        let x = screen_center.x - image_size.x * 0.5;
        let y = screen_center.y - image_size.y * 0.5;
        let tint = if is_placement_preview {
            Some(with_alpha(tokens.text, 156))
        } else if pcb_surface {
            // Footprint bodies carry their own silkscreen colors; tinting them
            // with the ink token would erase the material distinction between
            // body, silk and the live copper the CAD stream paints.
            None
        } else {
            Some(with_alpha(tokens.text, 255))
        };
        root = root.with_child(
            UiNode::image(
                format!("electronics.canvas.component-image.{index}"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(asset_key),
                    fit: UiImageFit::Stretch,
                    tint,
                },
            )
            .with_layout(
                UiLayout::absolute(UiRect::new(x, y, image_size.x, image_size.y)).with_z_index(
                    if is_placement_preview {
                        COMPONENT_PREVIEW_Z_INDEX
                    } else {
                        COMPONENT_Z_INDEX
                    },
                ),
            ),
        );
    }

    if editor.labels_visible() {
        let physical_labels = if pcb_surface {
            place_pcb_labels(editor, canvas_size)
        } else {
            std::collections::HashMap::new()
        };
        let mut labelled_pads = std::collections::HashSet::new();
        let mut labelled_net_ids = std::collections::HashSet::new();
        let mut labelled_net_names = std::collections::HashSet::new();
        for (index, object) in editor.scene().objects.iter().enumerate() {
            let Some(label) = object
                .label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            else {
                continue;
            };
            let Some(world_anchor) = label_anchor(object) else {
                continue;
            };
            // The geometric severity marker remains on the canvas; its full
            // report belongs in DRC. Repeating entire messages at pad centers
            // hides copper and makes routing impossible at board scale.
            if object.kind == CadObjectKind::DrcMarker {
                continue;
            }
            if editor.active_surface() == CadSurfaceKind::Pcb && object.kind == CadObjectKind::Pad {
                if object
                    .rect
                    .is_some_and(|rect| rect.size.min_element() * camera.zoom < 14.0)
                {
                    continue;
                }
            }
            if object.kind == CadObjectKind::Pad
                && !labelled_pads.insert((
                    object.source_id,
                    label.to_string(),
                    world_anchor.x.to_bits(),
                    world_anchor.y.to_bits(),
                ))
            {
                continue;
            }
            if object.kind == CadObjectKind::NetLabel {
                let first_for_net = if let Some(net_id) = object.net_id {
                    labelled_net_ids.insert(net_id)
                } else {
                    labelled_net_names.insert(label.to_string())
                };
                if !first_for_net {
                    continue;
                }
            }
            let screen = camera.screen_from_world(world_anchor, canvas_size);
            if screen.x < -MAX_LABEL_WIDTH
                || screen.x > canvas.width + MAX_LABEL_WIDTH
                || screen.y < -LABEL_HEIGHT
                || screen.y > canvas.height + LABEL_HEIGHT
            {
                continue;
            }

            let physical_label = physical_labels.get(&index);
            if pcb_surface && object.kind == CadObjectKind::Component
                && object.source_id.is_some() && physical_label.is_none() {
                continue;
            }
            let label = physical_label.map(|p| p.text.as_str()).unwrap_or(label);
            if label.is_empty() {
                continue;
            }
            let physical_component = physical_label.is_some();
            let label_height = if physical_component {
                physical_label.unwrap().rect.height
            } else {
                LABEL_HEIGHT
            };
            let severity = drc_severity(object);
            let label_size = if matches!(object.kind, CadObjectKind::Component) {
                if physical_component {
                    pcb_labels::FONT_SIZE
                } else {
                    COMPONENT_LABEL_SIZE
                }
            } else {
                DETAIL_LABEL_SIZE
            };
            let width = label
                .lines()
                .map(|line| label_track_width(line, label_size, severity))
                .fold(0.0_f32, f32::max);
            let local_x = match object.kind {
                CadObjectKind::Pin | CadObjectKind::Pad => screen.x + LABEL_GAP,
                _ => screen.x - width * 0.5,
            };
            let local_y = match object.kind {
                CadObjectKind::Component | CadObjectKind::NetLabel => {
                    screen.y - label_height - LABEL_GAP
                }
                _ => screen.y - LABEL_HEIGHT * 0.5,
            };
            let x = local_x;
            let y = local_y;

            let selected_object = selected.is_some_and(|selection| {
                selection.source_id == object.source_id.unwrap_or_default()
                    && matches!(
                        (selection.kind, object.kind),
                        (
                            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin,
                            CadObjectKind::Component | CadObjectKind::Pin | CadObjectKind::Pad
                        ) | (ElectronicsSelectionKind::Wire, CadObjectKind::Wire)
                            | (ElectronicsSelectionKind::Trace, CadObjectKind::Trace)
                    )
            });
            let color = if physical_component {
                if palette == StudioUiPalette::PaperLight {
                    [116, 85, 5, 255]
                } else {
                    [243, 222, 96, 255]
                }
            } else if selected_object {
                tokens.accent
            } else {
                label_color(tokens, object.kind, severity)
            };

            let id = format!("electronics.canvas.label.{index}");
            let label_rect = physical_label.map(|p| p.rect)
                .unwrap_or_else(|| UiRect::new(x, y, width, label_height));
            let text_style = UiTextStyle {
                role: UiTextRole::Label,
                size_px: label_size,
                line_height_px: if physical_component {
                    pcb_labels::LINE_HEIGHT
                } else {
                    LABEL_HEIGHT
                },
                weight: if selected_object || physical_component {
                    UiFontWeight::Bold
                } else {
                    UiFontWeight::Regular
                },
                color,
                inherit_color: false,
            };
            let Some(severity) = severity else {
                root = root.with_child(
                    UiNode::new(id, UiNodeKind::Label)
                        .with_layout(UiLayout::absolute(label_rect).with_z_index(LABEL_Z_INDEX))
                        .with_text_value(label.to_string())
                        .with_tooltip_value(object.label.clone().unwrap_or_else(|| label.to_string()))
                        // A canvas label is a runtime value, so the overflow
                        // policy is what keeps a long designator inside its
                        // own track instead of painting outside it.
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(text_style),
                );
                continue;
            };
            // A design-rule marker states its severity with a glyph and with
            // localized text, so the marker does not depend on color alone.
            let text = UiNode::new(format!("{id}.text"), UiNodeKind::Label)
                .with_text_value(format!(
                    "{}: {}",
                    i18n::t(severity_prefix_key(severity), params.language),
                    label
                ))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(text_style)
                .with_layout(UiLayout {
                    max_size: [MAX_LABEL_WIDTH - SEVERITY_ICON_TRACK, LABEL_HEIGHT],
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                });
            root = root.with_child(
                UiNode::new(id, UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Start,
                        gap: 2.0,
                        ..UiLayout::absolute(label_rect).with_z_index(LABEL_Z_INDEX)
                    })
                    .with_child(
                        UiNode::new(
                            format!("electronics.canvas.label.{index}.severity"),
                            UiNodeKind::Label,
                        )
                        .with_icon(
                            UiIcon::new(severity_icon(severity))
                                .with_size(UiIconSize::Custom(SEVERITY_ICON_SIZE as u16))
                                .with_tint(color),
                        )
                        .with_layout(UiLayout::fixed(SEVERITY_ICON_TRACK, LABEL_HEIGHT)),
                    )
                    .with_child(text),
            );
        }
    }

    if let Some(minimap) = build_minimap_chrome(palette, editor, canvas_size) {
        root = root.with_child(minimap);
    }

    root = root.with_child(build_tool_hint(tokens, editor, canvas, params.hint_fade));

    UiSurface::new("electronics.canvas.labels", palette, root)
}

fn place_pcb_labels(
    editor: &NativeElectronicsEditor, size: Vec2,
) -> std::collections::HashMap<usize, pcb_labels::LabelPlacement> {
    let (reference, value) = editor.pcb_label_visibility();
    if !reference && !value { return std::collections::HashMap::new(); }
    let components: std::collections::HashMap<_, _> = editor.pcb().components.iter()
        .map(|c| (c.component_id, c)).collect();
    let viewport = UiRect::new(0.0, 0.0, size.x, size.y);
    let camera = editor.camera();
    let requests = editor.scene().objects.iter().enumerate().filter_map(|(index, object)| {
        if object.kind != CadObjectKind::Component || object.label.is_none() { return None; }
        let id = object.source_id?;
        let component = components.get(&id)?;
        let rect = object.rect?;
        let center = camera.screen_from_world(rect.center, size);
        let (sin, cos) = object.rotation_degrees.to_radians().sin_cos();
        let extent = Vec2::new(rect.size.x*cos.abs()+rect.size.y*sin.abs(),
            rect.size.x*sin.abs()+rect.size.y*cos.abs()) * camera.zoom;
        if !center.is_finite() || !extent.is_finite() { return None; }
        let body = UiRect::new(center.x-extent.x*0.5-3.0, center.y-extent.y*0.5-3.0,
            extent.x+6.0, extent.y+6.0);
        if body.intersection(viewport).is_empty() { return None; }
        let text = match (reference, value) {
            (true,true) => format!("{}\n{}", component.designator, component.value),
            (true,false) => component.designator.clone(),
            _ => component.value.clone(),
        };
        let width = text.lines().map(|l| label_track_width(l, pcb_labels::FONT_SIZE, None))
            .fold(0.0_f32, f32::max);
        Some(pcb_labels::LabelRequest { index, body: body.intersection(viewport), text, width,
            selected: editor.selection().is_some_and(|s| s.source_id==id) })
    }).collect();
    let mut reserved = Vec::new();
    if let Some(bounds) = editor.minimap_rects(size) { reserved.push(bounds.panel); }
    let hint_height = if editor.pcb_routing_hint().is_some() { 100.0 } else { 42.0 };
    reserved.push(UiRect::new(0.0, (size.y-hint_height).max(0.0), size.x, hint_height));
    pcb_labels::place_labels(requests, size, &reserved)
}

/// Canvas tool hint.
///
/// The pill hugs its copy through the retained text atlas instead of stretching
/// a guessed width across the canvas, and the dock bounds it to the canvas so a
/// long translation can only ellipsize. An absolutely placed node keeps
/// `UiFlow::None`, which hands every child the whole overlay rect, so the stack
/// is declared by an explicit column dock instead of by the hint rect itself.
fn build_tool_hint(
    tokens: UiTokens,
    editor: &NativeElectronicsEditor,
    canvas: EditorRect,
    hint_fade: f32,
) -> UiNode {
    let max_width = hint_max_width(canvas);
    if let Some((heading, body, warning)) = editor.pcb_routing_hint() {
        let bottom = 12.0;
        let width = max_width.min(600.0);
        return UiNode::new("electronics.canvas.hint-dock", UiNodeKind::Panel)
            .with_layout(
                UiLayout::absolute(UiRect::new(
                    HINT_INSET,
                    (canvas.height - 80.0 - bottom).max(0.0),
                    width,
                    80.0,
                ))
                .with_z_index(HINT_Z_INDEX),
            )
            .with_style(UiStyle::transparent())
            .with_child(
                UiNode::new("electronics.canvas.route-guide", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        gap: 3.0,
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::fill(UiFlow::Column)
                    })
                    .with_style(UiStyle {
                        fill: tokens.surface_raised,
                        border: if warning {
                            tokens.warning
                        } else {
                            tokens.border
                        },
                        text: tokens.text,
                        border_width: 1.0,
                        radius: 6.0,
                        opacity: 1.0,
                    })
                    .with_child(
                        UiNode::new("electronics.canvas.route-source", UiNodeKind::Label)
                            .with_text_value(heading)
                            .with_layout(UiLayout::fixed(0.0, 20.0).fill_width())
                            .with_text_style(UiTextStyle::button(tokens.text))
                            .with_text_overflow(UiTextOverflow::Ellipsis),
                    )
                    .with_child(
                        UiNode::new("electronics.canvas.tool-hint", UiNodeKind::Label)
                            .with_text_value(body.clone())
                            .with_tooltip_value(body)
                            .with_layout(UiLayout::fixed(0.0, 36.0).fill_width())
                            .with_text_style(UiTextStyle::body(if warning {
                                tokens.warning
                            } else {
                                tokens.text_muted
                            }))
                            .with_text_overflow(UiTextOverflow::Wrap),
                    ),
            );
    }
    UiNode::new("electronics.canvas.hint-dock", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Start,
            justify_content: UiJustify::End,
            padding: UiSpacing::same(HINT_INSET),
            ..UiLayout::absolute(UiRect::new(
                0.0,
                0.0,
                canvas.width.max(1.0),
                canvas.height.max(1.0),
            ))
            .with_z_index(HINT_Z_INDEX)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("electronics.canvas.tool-hint", UiNodeKind::Label)
                .with_layout(UiLayout {
                    width_mode: UiSizeMode::FitContent,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, HINT_HEIGHT],
                    max_size: [max_width, HINT_HEIGHT],
                    ..UiLayout::default().with_text_safe_area(true)
                })
                .with_style(UiStyle {
                    fill: tokens.surface_raised,
                    border: tokens.border,
                    text: tokens.text,
                    border_width: ELECTRONICS_BORDER_WIDTH,
                    radius: ELECTRONICS_CORNER_RADIUS,
                    opacity: 0.96 * hint_fade.clamp(0.0, 1.0),
                })
                .with_text_key(
                    if editor.active_surface() == CadSurfaceKind::Pcb
                        && editor.tool() == crate::electronics_controller::ElectronicsTool::Select
                    {
                        "electronics.hint.select_pcb"
                    } else {
                        tool_hint_key(editor.tool())
                    },
                )
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Body,
                    size_px: HINT_BODY_SIZE,
                    line_height_px: HINT_HEIGHT,
                    weight: UiFontWeight::Regular,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
}

/// Track ceiling of the tool hint for one canvas size.
fn hint_max_width(canvas: EditorRect) -> f32 {
    (canvas.width.max(1.0) - HINT_INSET * 2.0)
        .min(canvas.width.max(1.0) * HINT_MAX_WIDTH_RATIO)
        .max(1.0)
}

fn tool_hint_key(tool: ElectronicsTool) -> &'static str {
    match tool {
        ElectronicsTool::Select => "electronics.hint.select",
        ElectronicsTool::Pan => "electronics.hint.pan",
        ElectronicsTool::Wire => "electronics.hint.wire",
        ElectronicsTool::Route => "electronics.hint.route",
        ElectronicsTool::Place => "electronics.hint.place",
        ElectronicsTool::BoardOutline => "electronics.hint.board_outline",
    }
}

/// Builds the retained minimap chrome: panel shell, header with surface
/// badge, and raster preview. Hit-testing uses the full panel rect; world
/// mapping uses only the inner image rect.
///
/// The panel is owned by the CAD input layer, not by RafUI: it is not focusable
/// and therefore carries no accessibility role, because a role on a node that
/// never enters the accessibility tree only pretends to be announced.
fn build_minimap_chrome(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    canvas_size: Vec2,
) -> Option<UiNode> {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let bounds = editor.minimap_rects(canvas_size)?;
    let panel = bounds.panel;
    let image = bounds.image;
    let local_panel = UiRect::new(0.0, 0.0, panel.width, panel.height);
    let local_image = UiRect::new(
        image.x - panel.x,
        image.y - panel.y,
        image.width,
        image.height,
    );
    let header = UiNode::new("electronics.canvas.minimap.header", UiNodeKind::Toolbar)
        .with_layout(
            UiLayout::absolute(UiRect::new(0.0, 0.0, panel.width, HEADER_HEIGHT))
                .with_z_index(MINIMAP_HEADER_Z_INDEX),
        )
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 0.0,
            radius: ELECTRONICS_CORNER_RADIUS,
            opacity: 1.0,
        });
    let (title_rect, badge) = minimap_header_split(
        palette,
        editor,
        UiRect::new(
            0.0,
            0.0,
            (local_panel.width - 28.0).max(1.0),
            local_panel.height,
        ),
    );
    let header = header
        .with_child(
            UiNode::new("electronics.canvas.minimap.title", UiNodeKind::Label)
                .with_layout(UiLayout::absolute(title_rect))
                .with_text_key("app.electronics_minimap")
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 12.0,
                    line_height_px: HEADER_HEIGHT - 8.0,
                    weight: UiFontWeight::Medium,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(badge)
        .with_child(
            UiNode::new("electronics.canvas.minimap.toggle", UiNodeKind::Button)
                .with_layout(UiLayout::absolute(UiRect::new(
                    (local_panel.width - 28.0).max(0.0),
                    0.0,
                    28.0,
                    HEADER_HEIGHT,
                )))
                .with_icon(UiIcon::new(if editor.minimap_collapsed() {
                    UiIconId::ChevronRight
                } else {
                    UiIconId::ChevronDown
                }))
                .with_style(UiStyle::transparent())
                .with_accessibility_role(raf_ui::UiAccessibilityRole::Button)
                .with_accessibility_label_key(if editor.minimap_collapsed() {
                    "electronics.minimap.expand"
                } else {
                    "electronics.minimap.collapse"
                })
                .with_accessibility_expanded(!editor.minimap_collapsed())
                .with_tooltip_key(if editor.minimap_collapsed() {
                    "electronics.minimap.expand"
                } else {
                    "electronics.minimap.collapse"
                })
                .focusable()
                .with_event(raf_ui::UiEventBinding::command(
                    raf_ui::UiEventKind::Click,
                    "electronics.minimap.toggle",
                )),
        );

    let mut panel_node = UiNode::new("electronics.canvas.minimap", UiNodeKind::Panel)
        .with_layout(UiLayout::absolute(panel).with_z_index(MINIMAP_Z_INDEX))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: ELECTRONICS_BORDER_WIDTH,
            radius: ELECTRONICS_CORNER_RADIUS,
            opacity: 0.98,
        })
        // The overview describes the active surface, so the schematic panel never
        // claims to be a board and the board panel never claims to be schematic.
        .with_tooltip_key(minimap_tooltip_key(editor.active_surface()))
        .with_child(header)
        .with_child(
            UiNode::image(
                "electronics.canvas.minimap.image",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(MINIMAP_IMAGE_KEY),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::absolute(local_image).with_z_index(MINIMAP_CONTENT_Z_INDEX)),
        );

    if editor.minimap_collapsed() {
        panel_node
            .children
            .retain(|node| node.id != "electronics.canvas.minimap.image");
        return Some(panel_node);
    }
    if let Some(viewport) =
        electronics_minimap::viewport_rect(editor.scene(), editor.camera(), canvas_size)
    {
        panel_node = panel_node.with_child(
            UiNode::new("electronics.canvas.minimap.viewport", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        viewport.x - panel.x,
                        viewport.y - panel.y,
                        viewport.width,
                        viewport.height,
                    ))
                    .with_z_index(MINIMAP_CONTENT_Z_INDEX + 1),
                )
                .with_style(UiStyle {
                    fill: [0, 0, 0, 0],
                    border: with_alpha(tokens.text_muted, 160),
                    border_width: 1.0,
                    radius: 0.0,
                    text: tokens.text,
                    opacity: 1.0,
                }),
        );
    }

    Some(panel_node)
}

/// Splits the minimap header into a title track and a surface badge.
///
/// The title track ends before the badge begins in every panel width, which is
/// the box invariant the header used to violate at the default panel size.
fn minimap_header_split(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    panel: UiRect,
) -> (UiRect, UiNode) {
    let badge_width = if editor.active_surface() == CadSurfaceKind::Pcb {
        48.0_f32
    } else {
        78.0
    }
    .min(panel.width - 16.0)
    .max(1.0);
    let badge_x = panel.x + panel.width - badge_width - MINIMAP_PAD;
    let title_x = panel.x + MINIMAP_HEADER_PAD;
    let title_width = (badge_x - MINIMAP_TITLE_GAP - title_x).max(1.0);
    let title = UiRect::new(title_x, panel.y + 4.0, title_width, HEADER_HEIGHT - 8.0);
    (
        title,
        build_surface_badge(palette, editor, badge_x, badge_width, panel),
    )
}

fn build_surface_badge(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    badge_x: f32,
    badge_width: f32,
    panel: UiRect,
) -> UiNode {
    let label_key = surface_label_key(editor.active_surface());
    // The badge is the one accent-filled element of the minimap, so it reuses
    // the shared accent recipe instead of repeating its colors.
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let accent = raf_ui::UiStyle {
        fill: tokens.surface,
        border: tokens.border,
        text: tokens.text_muted,
        border_width: 1.0,
        radius: 3.0,
        opacity: 1.0,
    };
    UiNode::new("electronics.canvas.minimap.badge", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::absolute(UiRect::new(
                badge_x,
                panel.y + (HEADER_HEIGHT - BADGE_HEIGHT) * 0.5,
                badge_width,
                BADGE_HEIGHT,
            ))
        })
        .with_style(UiStyle {
            fill: accent.fill,
            border: accent.border,
            text: accent.text,
            border_width: accent.border_width,
            radius: 3.0,
            opacity: accent.opacity,
        })
        .with_child(
            UiNode::new("electronics.canvas.minimap.badge.label", UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.0,
                    line_height_px: BADGE_HEIGHT - 2.0,
                    weight: UiFontWeight::Bold,
                    color: accent.text,
                    inherit_color: false,
                })
                .with_layout(UiLayout {
                    max_size: [badge_width - 4.0, BADGE_HEIGHT],
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_accessibility_label_key(label_key)
}

fn label_anchor(object: &CadObject) -> Option<Vec2> {
    if object.kind == CadObjectKind::Component && !object.line_paths.is_empty() {
        let mut points = object.line_paths.iter().flatten().copied();
        let first = points.next()?;
        let (min, max) = points.fold((first, first), |(min, max), point| {
            (min.min(point), max.max(point))
        });
        return Some(Vec2::new((min.x + max.x) * 0.5, min.y));
    }
    match object.kind {
        CadObjectKind::Component => object.rect.map(|rect| {
            let (sin, cos) = object.rotation_degrees.to_radians().sin_cos();
            let half_height = (rect.size.x * sin.abs() + rect.size.y * cos.abs()) * 0.5;
            rect.center + Vec2::new(0.0, -half_height)
        }),
        CadObjectKind::Pin | CadObjectKind::Pad | CadObjectKind::NetLabel => {
            object.rect.map(|rect| rect.center)
        }
        CadObjectKind::DrcMarker => object.rect.map(|rect| rect.center),
        _ => None,
    }
}

/// Track reserved for one canvas label.
///
/// The width is a layout bound, not an estimate of the text: the label node is
/// content sized and ellipsized, so a wrong estimate can only shorten the text,
/// never paint outside the box. The advance is derived from the label font size
/// because a 12px component label and a 10px pin label do not advance the same
/// amount per character.
fn label_track_width(label: &str, size_px: f32, severity: Option<DrcSeverity>) -> f32 {
    let characters = label.chars().count() as f32;
    let severity_track = if severity.is_some() {
        SEVERITY_ICON_TRACK
    } else {
        0.0
    };
    let text = characters * size_px * LABEL_ADVANCE_RATIO + LABEL_TRACK_INSET;
    (text + severity_track).clamp(ELECTRONICS_ROW_MIN_TRACK + severity_track, MAX_LABEL_WIDTH)
}

fn label_color(tokens: UiTokens, kind: CadObjectKind, severity: Option<DrcSeverity>) -> [u8; 4] {
    match kind {
        CadObjectKind::Pin | CadObjectKind::Pad => tokens.text_muted,
        CadObjectKind::NetLabel => with_alpha(tokens.positive, 220),
        CadObjectKind::DrcMarker => {
            severity_color(tokens, severity.unwrap_or(DrcSeverity::Warning))
        }
        _ => tokens.text,
    }
}

fn severity_color(tokens: UiTokens, severity: DrcSeverity) -> [u8; 4] {
    match severity {
        DrcSeverity::Error => tokens.danger,
        DrcSeverity::Warning => tokens.warning,
        DrcSeverity::Info => tokens.info,
    }
}

fn severity_icon(severity: DrcSeverity) -> UiIconId {
    match severity {
        DrcSeverity::Error => UiIconId::Error,
        DrcSeverity::Warning => UiIconId::Warning,
        DrcSeverity::Info => UiIconId::Success,
    }
}

fn severity_prefix_key(severity: DrcSeverity) -> &'static str {
    match severity {
        DrcSeverity::Error => "electronics.analysis.severity_error",
        DrcSeverity::Warning => "electronics.analysis.severity_warning",
        DrcSeverity::Info => "electronics.analysis.severity_info",
    }
}

/// Recovers the severity of a design-rule marker from its color.
fn drc_severity(object: &CadObject) -> Option<DrcSeverity> {
    if object.kind != CadObjectKind::DrcMarker {
        return None;
    }
    let color = object.color_rgba;
    if color == DRC_ERROR_COLOR {
        Some(DrcSeverity::Error)
    } else if color == DRC_WARNING_COLOR {
        Some(DrcSeverity::Warning)
    } else if color == DRC_INFO_COLOR {
        Some(DrcSeverity::Info)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;

    fn overlay(width: f32, height: f32) -> (NativeElectronicsEditor, UiSurface) {
        let editor = NativeElectronicsEditor::empty("test");
        let surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            EditorRect::new(0.0, 0.0, width, height),
            ElectronicsCanvasOverlayParams::new(1.0, Language::English),
        );
        (editor, surface)
    }

    fn populated_editor(width: f32, height: f32) -> NativeElectronicsEditor {
        let mut editor = NativeElectronicsEditor::empty("test");
        let canvas = EditorRect::new(0.0, 0.0, width, height);
        assert!(editor.begin_library_drag(0));
        assert!(editor.finish_library_drag_at(Some([width * 0.5, height * 0.5]), canvas));
        editor
    }

    #[test]
    fn overlay_root_is_clipped_to_its_canvas_surface() {
        let (_, surface) = overlay(640.0, 480.0);
        assert_eq!(surface.root.kind, UiNodeKind::Overlay);
        assert_eq!(surface.root.layout.overflow, UiOverflow::Clip);
        assert!(surface.root.layout.rect.is_none());
    }

    #[test]
    fn empty_document_hides_the_minimap_panel() {
        let (_, surface) = overlay(640.0, 480.0);
        assert!(surface.root.find("electronics.canvas.minimap").is_none());
    }

    #[test]
    fn populated_document_exposes_the_compact_minimap() {
        let editor = populated_editor(640.0, 480.0);
        let surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            EditorRect::new(0.0, 0.0, 640.0, 480.0),
            ElectronicsCanvasOverlayParams::new(1.0, Language::English),
        );
        let panel = surface
            .root
            .find("electronics.canvas.minimap")
            .expect("minimap panel");
        assert_eq!(panel.kind, UiNodeKind::Panel);
        assert_eq!(
            panel.tooltip_key.as_deref(),
            Some("electronics.minimap.tooltip_schematic")
        );
        assert!(panel
            .children
            .iter()
            .any(|child| child.id == "electronics.canvas.minimap.header"));
        assert!(panel
            .children
            .iter()
            .any(|child| child.id == "electronics.canvas.minimap.image"));
        assert!(!panel
            .children
            .iter()
            .any(|child| child.id == "electronics.canvas.minimap.empty"));
        assert!(panel.layout.rect.unwrap().width <= 184.0);
        let panel_rect = panel.layout.rect.expect("panel rect");
        let header_rect = panel.children[0].layout.rect.expect("header rect");
        assert_eq!(
            header_rect,
            UiRect::new(0.0, 0.0, panel_rect.width, HEADER_HEIGHT)
        );
    }

    #[test]
    fn the_minimap_title_ends_before_the_badge_starts_at_every_panel_width() {
        for (width, height) in [
            (240.0_f32, 200.0_f32),
            (360.0, 300.0),
            (1024.0, 768.0),
            (120.0, 120.0),
        ] {
            let editor = populated_editor(width, height);
            let surface = build_electronics_canvas_overlay_surface(
                StudioUiPalette::IndustrialDark,
                &editor,
                EditorRect::new(0.0, 0.0, width, height),
                ElectronicsCanvasOverlayParams::new(1.0, Language::English),
            );
            let title = surface
                .root
                .find("electronics.canvas.minimap.title")
                .expect("minimap title");
            let badge = surface
                .root
                .find("electronics.canvas.minimap.badge")
                .expect("minimap badge");
            let title_rect = title.layout.rect.expect("title rect");
            let badge_rect = badge.layout.rect.expect("badge rect");
            assert!(
                title_rect.x + title_rect.width <= badge_rect.x + f32::EPSILON,
                "title [{}, {}, {}] overlaps badge [{}, {}, {}] at {width}x{height}",
                title_rect.x,
                title_rect.y,
                title_rect.width,
                badge_rect.x,
                badge_rect.y,
                badge_rect.width
            );
            assert!(badge_rect.x + badge_rect.width <= title_rect.x + width);
        }
    }

    #[test]
    fn the_minimap_badge_keeps_a_quiet_readable_surface_in_both_themes() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let editor = populated_editor(640.0, 480.0);
            let surface = build_electronics_canvas_overlay_surface(
                palette,
                &editor,
                EditorRect::new(0.0, 0.0, 640.0, 480.0),
                ElectronicsCanvasOverlayParams::new(1.0, Language::English),
            );
            let badge = surface
                .root
                .find("electronics.canvas.minimap.badge")
                .expect("minimap badge");
            let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
            assert_eq!(badge.style.fill, tokens.surface);
            assert_eq!(badge.style.text, tokens.text_muted);
        }
    }

    #[test]
    fn collapsed_minimap_releases_its_image_area_without_editing_the_document() {
        let mut editor = populated_editor(800.0, 600.0);
        let before = editor.is_dirty();
        let size = Vec2::new(800.0, 600.0);
        let expanded = editor.minimap_rects(size).unwrap();
        let expanded_surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark, &editor,
            EditorRect::new(0.0, 0.0, 800.0, 600.0),
            ElectronicsCanvasOverlayParams::new(1.0, Language::English));
        let mut host = raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost::new(expanded_surface, [0, 0, 0, 0]);
        host.render([800, 600], |key| i18n::t(key, Language::English));
        assert!(editor.apply_ui_command("electronics.minimap.toggle"));
        let collapsed = editor.minimap_rects(size).unwrap();
        assert_eq!(collapsed.panel.height, HEADER_HEIGHT);
        assert_eq!(collapsed.image.width, 0.0);
        assert!(expanded.panel.height > collapsed.panel.height);
        assert_eq!(editor.is_dirty(), before);
        let surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            EditorRect::new(0.0, 0.0, 800.0, 600.0),
            ElectronicsCanvasOverlayParams::new(1.0, Language::English),
        );
        assert!(surface
            .root
            .find("electronics.canvas.minimap.image")
            .is_none());
        host.set_surface(surface.clone());
        let frame = host.render([800, 600], |key| i18n::t(key, Language::English));
        let old_image_point = [expanded.image.x + expanded.image.width * 0.5,
            expanded.image.y + expanded.image.height * 0.5];
        assert!(raf_ui::hit_test(&frame.frame.hit_regions, old_image_point,
            raf_ui::UiHitTestMode::InteractiveOnly).is_none(),
            "Collapsed minimap must release actual retained hit regions below its header");

        assert!(
            surface
                .root
                .find("electronics.canvas.minimap.toggle")
                .unwrap()
                .focusable
        );
    }

    #[test]
    fn the_minimap_panel_does_not_claim_an_unreachable_role() {
        let editor = populated_editor(640.0, 480.0);
        let surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            EditorRect::new(0.0, 0.0, 640.0, 480.0),
            ElectronicsCanvasOverlayParams::new(1.0, Language::English),
        );
        let panel = surface
            .root
            .find("electronics.canvas.minimap")
            .expect("minimap panel");
        assert!(!panel.focusable, "the CAD layer owns the minimap drag");
        assert!(
            !panel.interactive,
            "an unowned overlay must not answer RafUI input"
        );
    }

    #[test]
    fn minimap_children_resolve_to_canvas_bounds_and_render_visible_pixels() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;

        let size = Vec2::new(640.0, 480.0);
        let editor = populated_editor(size.x, size.y);
        let expected = electronics_minimap::visible_rects(editor.scene(), size)
            .expect("visible content shares one image and hit region");
        let palette = StudioUiPalette::IndustrialDark;
        let surface = build_electronics_canvas_overlay_surface(
            palette,
            &editor,
            EditorRect::new(0.0, 0.0, size.x, size.y),
            ElectronicsCanvasOverlayParams::new(1.0, Language::English),
        );
        let (image_size, image_pixels) = electronics_minimap::build_scene_rgba(
            electronics_minimap::MinimapColors::from_palette(palette),
            editor.scene(),
            editor.selection(),
        );
        let mut host = CpuUiSurfaceHost::new(surface, [0, 0, 0, 0]);
        host.images_mut()
            .insert_rgba(MINIMAP_IMAGE_KEY, image_size, image_pixels)
            .expect("register the real minimap raster");
        let frame = host.render([size.x as u32, size.y as u32], |key| {
            i18n::t(key, Language::English)
        });
        let resolved_rect = |id: &str| {
            frame
                .frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == id)
                .unwrap_or_else(|| panic!("missing resolved minimap node {id}"))
                .rect
        };

        let panel = resolved_rect("electronics.canvas.minimap");
        let header = resolved_rect("electronics.canvas.minimap.header");
        let title = resolved_rect("electronics.canvas.minimap.title");
        let badge = resolved_rect("electronics.canvas.minimap.badge");
        let image = resolved_rect("electronics.canvas.minimap.image");
        assert_eq!(
            panel, expected.panel,
            "panel resolves in canvas coordinates"
        );
        assert_eq!(
            header,
            UiRect::new(panel.x, panel.y, panel.width, HEADER_HEIGHT)
        );
        assert_eq!(
            image, expected.image,
            "image resolves to the shared mapping rect"
        );
        assert!(title.x >= panel.x && title.right() <= panel.right());
        assert!(badge.x >= panel.x && badge.right() <= panel.right());
        assert!(title.y >= panel.y && title.bottom() <= header.bottom());
        assert!(badge.y >= panel.y && badge.bottom() <= header.bottom());
        assert!(
            frame.metrics.image_pixels > 0,
            "the minimap image paints real pixels"
        );

        let image_left = image.x.floor().max(0.0) as usize;
        let image_top = image.y.floor().max(0.0) as usize;
        let image_right = image.right().ceil().min(size.x) as usize;
        let image_bottom = image.bottom().ceil().min(size.y) as usize;
        let mut visible_colors = std::collections::HashSet::new();
        for y in image_top..image_bottom {
            for x in image_left..image_right {
                let pixel = (y * size.x as usize + x) * 4;
                visible_colors.insert([
                    frame.pixels[pixel],
                    frame.pixels[pixel + 1],
                    frame.pixels[pixel + 2],
                ]);
            }
        }
        assert!(
            visible_colors.len() > 2,
            "the resolved image area contains raster detail, not just the panel fill"
        );
    }

    #[test]
    fn the_tool_hint_stays_inside_the_canvas_at_every_width() {
        for width in [240.0_f32, 360.0, 640.0, 1920.0] {
            let (_, surface) = overlay(width, 400.0);
            let dock = surface
                .root
                .find("electronics.canvas.hint-dock")
                .expect("hint dock");
            let dock_rect = dock.layout.rect.expect("dock rect");
            assert_eq!(dock_rect.width, width);
            let hint = surface
                .root
                .find("electronics.canvas.tool-hint")
                .expect("tool hint");
            let frame = surface.build_frame(width as u32, 400, [0, 0, 0, 255]);
            let resolved = frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == "electronics.canvas.tool-hint")
                .expect("resolved hint");
            assert!(resolved.rect.x >= HINT_INSET - f32::EPSILON);
            assert!(
                resolved.rect.x + resolved.rect.width <= width - HINT_INSET + f32::EPSILON,
                "hint {:?} escapes the inset canvas {width}",
                resolved.rect
            );
            assert!(
                resolved.rect.y + resolved.rect.height <= 400.0 - HINT_INSET + f32::EPSILON,
                "hint {:?} is not anchored to the bottom inset",
                resolved.rect
            );
            assert_eq!(hint.layout.height_mode, UiSizeMode::Fixed);
            assert_eq!(hint.layout.basis[1], HINT_HEIGHT);
            assert!(hint.text_overflow == UiTextOverflow::Ellipsis);
        }
    }

    #[test]
    fn the_tool_hint_is_content_sized_and_only_bounded_by_the_canvas() {
        let (_, surface) = overlay(1920.0, 400.0);
        let hint = surface
            .root
            .find("electronics.canvas.tool-hint")
            .expect("tool hint");
        assert_eq!(
            hint.layout.width_mode,
            UiSizeMode::FitContent,
            "the pill must hug its measured copy instead of a guessed width"
        );
        let wide = hint_max_width(EditorRect::new(0.0, 0.0, 1920.0, 400.0));
        assert_eq!(hint.layout.max_size[0], wide);
        // The invariant is that the pill never outgrows the canvas it sits on,
        // whatever the ratio happens to bind to.
        for width in [360.0_f32, 720.0, 1024.0, 1920.0] {
            let canvas = EditorRect::new(0.0, 0.0, width, 400.0);
            let max = hint_max_width(canvas);
            assert!(max > 0.0, "hint collapsed at {width}px");
            assert!(
                max <= width - HINT_INSET * 2.0,
                "hint track {max} overflows a {width}px canvas"
            );
            assert!(max <= width * HINT_MAX_WIDTH_RATIO + f32::EPSILON);
        }
    }

    #[test]
    fn the_longest_localized_hint_fits_the_narrowest_canvas() {
        // Measured with the bundled Ubuntu Regular face at 11px against the
        // Spanish catalog: "Seleccionar: clic o arrastre; Espacio + arrastre
        // mueve la vista" advances 296.9 points. The hint reserves the shared
        // label safe inset on both sides.
        const LONGEST_HINT_POINTS: f32 = 296.9;
        let narrowest_canvas = 360.0_f32;
        let track = hint_max_width(EditorRect::new(0.0, 0.0, narrowest_canvas, 400.0));
        let safe = raf_ui::UiLayout::LABEL_SAFE_INSET_X * 2.0;
        assert!(
            track - safe >= LONGEST_HINT_POINTS,
            "the longest hint needs {LONGEST_HINT_POINTS} points but the track offers {}",
            track - safe
        );
    }

    #[test]
    fn the_canvas_overlay_passes_the_retained_layout_gate() {
        for (width, height) in [(360.0_f32, 480.0_f32), (1280.0, 720.0)] {
            let (_, surface) = overlay(width, height);
            assert_electronics_layout_gate(&surface, width as u32, height as u32);
        }
    }

    #[test]
    fn the_tool_hint_fades_with_the_host_motion_sample() {
        let editor = NativeElectronicsEditor::empty("test");
        let rect = EditorRect::new(0.0, 0.0, 640.0, 480.0);
        let faded = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            rect,
            ElectronicsCanvasOverlayParams::new(0.0, Language::English),
        );
        let hint = faded
            .root
            .find("electronics.canvas.tool-hint")
            .expect("tool hint");
        assert_eq!(hint.style.opacity, 0.0);
    }

    #[test]
    fn drc_severity_is_recovered_from_the_marker_palette() {
        let object = |color| CadObject {
            shape: Default::default(),
            rotation_degrees: 0.0,
            drill: None,
            stroke_width_world: None,
            id: "drc:test:0".to_string(),
            source_id: None,
            kind: CadObjectKind::DrcMarker,
            layer: raf_electronics::CadLayerKind::Drc,
            pick_priority: raf_electronics::CadPickPriority::Drc,
            rect: Some(raf_electronics::CadRect::new(Vec2::ZERO, Vec2::splat(16.0))),
            points: Vec::new(),
            line_paths: Vec::new(),
            label: Some("short_circuit".to_string()),
            net: None,
            net_id: None,
            color_rgba: color,
        };
        assert_eq!(
            drc_severity(&object(DRC_ERROR_COLOR)),
            Some(DrcSeverity::Error)
        );
        assert_eq!(
            drc_severity(&object(DRC_WARNING_COLOR)),
            Some(DrcSeverity::Warning)
        );
        assert_eq!(
            drc_severity(&object(DRC_INFO_COLOR)),
            Some(DrcSeverity::Info)
        );
        assert_eq!(drc_severity(&object([1, 2, 3, 4])), None);
    }

    #[test]
    fn a_severity_label_reserves_room_for_its_glyph() {
        let with_severity = label_track_width(
            "short_circuit: N001",
            DETAIL_LABEL_SIZE,
            Some(DrcSeverity::Error),
        );
        let without = label_track_width("short_circuit: N001", DETAIL_LABEL_SIZE, None);
        // The glyph is added on top of the measured text, never traded against
        // it, so a severity marker can never push the message out of its box.
        assert!(
            with_severity >= without + SEVERITY_ICON_TRACK,
            "severity label {with_severity} must clear the plain {without} plus the glyph track"
        );
        assert!(with_severity <= MAX_LABEL_WIDTH);
    }

    #[test]
    fn a_label_track_follows_the_font_size_it_will_be_rasterized_at() {
        let short = "N001";
        let component = label_track_width(short, COMPONENT_LABEL_SIZE, None);
        let pin = label_track_width(short, DETAIL_LABEL_SIZE, None);
        assert!(
            component > pin,
            "a 12px component label needs more room than a 10px pin label"
        );
        assert!(pin > ELECTRONICS_ROW_MIN_TRACK);
        assert!(label_track_width(&"x".repeat(64), DETAIL_LABEL_SIZE, None) <= MAX_LABEL_WIDTH);
    }
}
