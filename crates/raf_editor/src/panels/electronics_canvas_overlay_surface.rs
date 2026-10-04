//! Retained RafUI artwork and labels for the native Electronics canvas.
//!
//! ApiGraphicBasic owns the CAD geometry. Native component artwork and labels
//! remain a RafUI overlay so they use the same image/text compositor as the
//! editor chrome and do not need a second presentation implementation inside
//! the line renderer. The overlay is deliberately non-interactive: pointer
//! ownership stays with the CAD controller.

use glam::Vec2;
use raf_core::{i18n, Language};
use raf_electronics::{CadObject, CadObjectKind};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiIcon, UiIconId, UiIconSize, UiImage, UiImageFit,
    UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiSizeMode,
    UiSpacing, UiStyle, UiTextOverflow, UiTextRole, UiTextStyle, UiTokens,
};

use crate::editor_layout::EditorRect;
use crate::electronics_controller::{
    ElectronicsSelectionKind, ElectronicsTool, NativeElectronicsEditor,
};
use crate::electronics_minimap::{
    self, minimap_empty_key, minimap_tooltip_key, surface_label_key, HEADER_HEIGHT,
    IMAGE_KEY as MINIMAP_IMAGE_KEY,
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
    let tokens = palette.tokens();
    let canvas_size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
    let camera = editor.camera();
    let selected = editor.selection();
    let mut root_layout = UiLayout::fill(UiFlow::None);
    root_layout.overflow = UiOverflow::Clip;
    let mut root = UiNode::new("electronics.canvas.labels", UiNodeKind::Overlay)
        .with_layout(root_layout)
        .with_style(UiStyle::transparent());

    // Component artwork is a native PNG overlay. The CAD line stream keeps
    // wires, pins and hit regions, while the symbol itself comes from the
    // asset catalog so it stays crisp and does not depend on procedural
    // line-by-line symbol geometry.
    for (index, object) in editor.scene().objects.iter().enumerate() {
        if object.kind != CadObjectKind::Component {
            continue;
        }
        let Some(rect) = object.rect else {
            continue;
        };
        let is_placement_preview = object.id == "component-placement-preview";
        let asset_key = match object.source_id {
            Some(source_id) => editor.component_asset_key(source_id),
            None if is_placement_preview => editor
                .placement_preview_asset_key()
                .unwrap_or("electronics://library/generic.png"),
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
        root = root.with_child(
            UiNode::image(
                format!("electronics.canvas.component-image.{index}"),
                UiImage {
                    source: UiImageSource::new(asset_key),
                    fit: UiImageFit::Stretch,
                    // The placement ghost is the themed text token at a reduced
                    // alpha, not a literal white, so it follows both themes.
                    tint: is_placement_preview.then_some(with_alpha(tokens.text, 156)),
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
            let screen = camera.screen_from_world(world_anchor, canvas_size);
            if screen.x < -MAX_LABEL_WIDTH
                || screen.x > canvas.width + MAX_LABEL_WIDTH
                || screen.y < -LABEL_HEIGHT
                || screen.y > canvas.height + LABEL_HEIGHT
            {
                continue;
            }

            let severity = drc_severity(object);
            let label_size = if matches!(object.kind, CadObjectKind::Component) {
                COMPONENT_LABEL_SIZE
            } else {
                DETAIL_LABEL_SIZE
            };
            let width = label_track_width(label, label_size, severity);
            let local_x = match object.kind {
                CadObjectKind::Pin | CadObjectKind::Pad => screen.x + LABEL_GAP,
                _ => screen.x - width * 0.5,
            };
            let local_y = match object.kind {
                CadObjectKind::Component => screen.y - LABEL_HEIGHT - LABEL_GAP,
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
            let color = if selected_object {
                tokens.accent
            } else {
                label_color(tokens, object.kind, severity)
            };

            let id = format!("electronics.canvas.label.{index}");
            let label_rect = UiRect::new(x, y, width, LABEL_HEIGHT);
            let text_style = UiTextStyle {
                role: UiTextRole::Label,
                size_px: label_size,
                line_height_px: LABEL_HEIGHT,
                weight: if selected_object {
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

    root = root.with_child(build_minimap_chrome(palette, editor, canvas_size));

    root = root.with_child(build_tool_hint(tokens, editor, canvas, params.hint_fade));

    UiSurface::new("electronics.canvas.labels", palette, root)
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
                .with_text_key(tool_hint_key(editor.tool()))
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
/// badge, raster preview, and empty state. Hit-testing uses the full panel
/// rect; world mapping uses only the inner image rect.
///
/// The panel is owned by the CAD input layer, not by RafUI: it is not focusable
/// and therefore carries no accessibility role, because a role on a node that
/// never enters the accessibility tree only pretends to be announced.
fn build_minimap_chrome(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    canvas_size: Vec2,
) -> UiNode {
    let tokens = palette.tokens();
    let panel = electronics_minimap::overlay_rect(canvas_size);
    let image = electronics_minimap::image_rect(canvas_size);
    let header = UiNode::new("electronics.canvas.minimap.header", UiNodeKind::Toolbar)
        .with_layout(
            UiLayout::absolute(UiRect::new(panel.x, panel.y, panel.width, HEADER_HEIGHT))
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
    let (title_rect, badge) = minimap_header_split(palette, editor, panel);
    let header = header
        .with_child(
            UiNode::new("electronics.canvas.minimap.title", UiNodeKind::Label)
                .with_layout(UiLayout::absolute(title_rect))
                .with_text_key("app.electronics_minimap")
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 11.0,
                    line_height_px: HEADER_HEIGHT - 8.0,
                    weight: UiFontWeight::Medium,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(badge);

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
                    source: UiImageSource::new(MINIMAP_IMAGE_KEY),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::absolute(image).with_z_index(MINIMAP_CONTENT_Z_INDEX)),
        );

    if !electronics_minimap::has_document_content(editor.scene()) {
        panel_node = panel_node.with_child(
            UiNode::new("electronics.canvas.minimap.empty", UiNodeKind::Label)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        image.x,
                        image.y + image.height * 0.5 - 10.0,
                        image.width,
                        20.0,
                    ))
                    .with_z_index(MINIMAP_CONTENT_Z_INDEX)
                    .with_text_safe_area(true),
                )
                .with_text_key(minimap_empty_key(editor.active_surface()))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Body,
                    size_px: 11.0,
                    line_height_px: 20.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                }),
        );
    }

    panel_node
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
    let badge_width = BADGE_MIN_WIDTH
        .max(panel.width * BADGE_MAX_WIDTH_RATIO)
        .min(panel.width - MINIMAP_HEADER_PAD * 2.0)
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
    let accent = palette.accent_style();
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
    match object.kind {
        CadObjectKind::Component => object
            .rect
            .map(|rect| rect.center + Vec2::new(0.0, -rect.size.y * 0.5)),
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

    #[test]
    fn overlay_root_is_clipped_to_its_canvas_surface() {
        let (_, surface) = overlay(640.0, 480.0);
        assert_eq!(surface.root.kind, UiNodeKind::Overlay);
        assert_eq!(surface.root.layout.overflow, UiOverflow::Clip);
        assert!(surface.root.layout.rect.is_none());
    }

    #[test]
    fn minimap_chrome_exposes_header_image_and_empty_state() {
        let (_, surface) = overlay(640.0, 480.0);
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
        assert!(panel
            .children
            .iter()
            .any(|child| child.id == "electronics.canvas.minimap.empty"));
        let panel_rect = panel.layout.rect.expect("panel rect");
        let header_rect = panel.children[0].layout.rect.expect("header rect");
        assert_eq!(
            header_rect,
            UiRect::new(panel_rect.x, panel_rect.y, panel_rect.width, HEADER_HEIGHT)
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
            let editor = NativeElectronicsEditor::empty("test");
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
    fn the_minimap_badge_keeps_the_shared_accent_recipe_in_both_themes() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let editor = NativeElectronicsEditor::empty("test");
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
            let accent = palette.accent_style();
            assert_eq!(badge.style.fill, accent.fill);
            assert_eq!(badge.style.text, accent.text);
        }
    }

    #[test]
    fn the_minimap_panel_does_not_claim_an_unreachable_role() {
        let (_, surface) = overlay(640.0, 480.0);
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
    fn minimap_image_rect_sits_below_header_and_inside_panel() {
        let size = Vec2::new(640.0, 480.0);
        let panel = electronics_minimap::overlay_rect(size);
        let image = electronics_minimap::image_rect(size);
        assert!(image.y >= panel.y + HEADER_HEIGHT - f32::EPSILON);
        assert!(image.x >= panel.x - f32::EPSILON);
        assert!(image.x + image.width <= panel.x + panel.width + f32::EPSILON);
        assert!(image.y + image.height <= panel.y + panel.height + f32::EPSILON);
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
