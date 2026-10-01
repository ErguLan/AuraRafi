//! Retained RafUI artwork and labels for the native Electronics canvas.
//!
//! ApiGraphicBasic owns the CAD geometry. Native component artwork and labels
//! remain a RafUI overlay so they use the same image/text compositor as the
//! editor chrome and do not need a second presentation implementation inside
//! the line renderer. The overlay is deliberately non-interactive: pointer
//! ownership stays with the CAD controller.

use glam::Vec2;
use raf_electronics::{CadObject, CadObjectKind};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiFlow, UiFontWeight, UiImage, UiImageFit, UiImageSource,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiStyle, UiTextOverflow,
    UiTextRole, UiTextStyle, UiTokens,
};

use crate::editor_layout::EditorRect;
use crate::electronics_controller::{
    ElectronicsSelectionKind, ElectronicsTool, NativeElectronicsEditor,
};
use crate::electronics_minimap::{
    self, HEADER_HEIGHT, IMAGE_KEY as MINIMAP_IMAGE_KEY, surface_label_key,
};

const LABEL_HEIGHT: f32 = 18.0;
const LABEL_GAP: f32 = 4.0;
const MIN_LABEL_WIDTH: f32 = 24.0;
const MAX_LABEL_WIDTH: f32 = 180.0;
const COMPONENT_Z_INDEX: i16 = 0;
const COMPONENT_PREVIEW_Z_INDEX: i16 = 4;
const MINIMAP_Z_INDEX: i16 = 5;
const MINIMAP_HEADER_Z_INDEX: i16 = 6;
const MINIMAP_CONTENT_Z_INDEX: i16 = 7;
const LABEL_Z_INDEX: i16 = 10;
const HINT_Z_INDEX: i16 = 20;
const MINIMAP_PAD: f32 = 6.0;
const MINIMAP_HEADER_PAD: f32 = 8.0;
const BADGE_HEIGHT: f32 = 16.0;
const BADGE_MIN_WIDTH: f32 = 34.0;

/// Builds only the labels that are visible in the current CAD camera.
///
/// The returned root is a canvas-local, clipped RafUI overlay. It is presented
/// into the same physical rectangle as the ApiGraphicBasic CAD surface, so
/// component artwork can never paint over the workbench chrome.
pub fn build_electronics_canvas_overlay_surface(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    canvas: EditorRect,
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
                    tint: is_placement_preview.then_some([255, 255, 255, 156]),
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

            let width = label_width(label);
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
                label_color(tokens.text, tokens.text_muted, object.kind)
            };

            root = root.with_child(
                UiNode::new(
                    format!("electronics.canvas.label.{index}"),
                    UiNodeKind::Label,
                )
                .with_layout(
                    UiLayout::absolute(UiRect::new(x, y, width, LABEL_HEIGHT))
                        .with_z_index(LABEL_Z_INDEX),
                )
                .with_text_value(label.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: if matches!(object.kind, CadObjectKind::Component) {
                        12.0
                    } else {
                        10.0
                    },
                    line_height_px: LABEL_HEIGHT,
                    weight: if selected_object {
                        UiFontWeight::Bold
                    } else {
                        UiFontWeight::Regular
                    },
                    color,
                    inherit_color: false,
                }),
            );
        }
    }

    root = root.with_child(build_minimap_chrome(tokens, editor, canvas_size));

    let hint_width: f32 = 320.0;
    let hint_height: f32 = 28.0;
    let hint_x = 12.0;
    let hint_y = canvas.height - hint_height - 12.0;
    root = root.with_child(
        UiNode::new("electronics.canvas.tool-hint", UiNodeKind::Label)
            .with_layout(
                UiLayout::absolute(UiRect::new(
                    hint_x,
                    hint_y,
                    hint_width.min(canvas.width.max(1.0) - 24.0).max(1.0),
                    hint_height,
                ))
                .with_z_index(HINT_Z_INDEX)
                .with_text_safe_area(true),
            )
            .with_style(UiStyle {
                fill: tokens.surface_raised,
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 4.0,
                opacity: 0.96,
            })
            .with_text_key(tool_hint_key(editor.tool()))
            .with_text_style(UiTextStyle {
                role: UiTextRole::Body,
                size_px: 11.0,
                line_height_px: hint_height,
                weight: UiFontWeight::Regular,
                color: tokens.text,
                inherit_color: false,
            }),
    );

    UiSurface::new("electronics.canvas.labels", palette, root)
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
fn build_minimap_chrome(
    tokens: UiTokens,
    editor: &NativeElectronicsEditor,
    canvas_size: Vec2,
) -> UiNode {
    let panel = electronics_minimap::overlay_rect(canvas_size);
    let image = electronics_minimap::image_rect(canvas_size);
    let mut panel_node = UiNode::new("electronics.canvas.minimap", UiNodeKind::Panel)
        .with_class("electronics-minimap")
        .with_layout(UiLayout::absolute(panel).with_z_index(MINIMAP_Z_INDEX))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 0.98,
        })
        .with_tooltip_key("electronics.minimap.tooltip")
        .with_accessibility_label_key("electronics.minimap.tooltip")
        .with_accessibility_role(UiAccessibilityRole::Status);

    let header = UiNode::new("electronics.canvas.minimap.header", UiNodeKind::Toolbar)
        .with_class("electronics-minimap-header")
        .with_layout(
            UiLayout::absolute(UiRect::new(
                panel.x,
                panel.y,
                panel.width,
                HEADER_HEIGHT,
            ))
            .with_z_index(MINIMAP_HEADER_Z_INDEX),
        )
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 0.0,
            radius: 4.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("electronics.canvas.minimap.title", UiNodeKind::Label)
                .with_layout(UiLayout::absolute(UiRect::new(
                    panel.x + MINIMAP_HEADER_PAD,
                    panel.y + 4.0,
                    (panel.width - BADGE_MIN_WIDTH - MINIMAP_HEADER_PAD * 3.0).max(1.0),
                    HEADER_HEIGHT - 8.0,
                )))
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
        .with_child(build_surface_badge(tokens, editor, panel));
    panel_node = panel_node.with_child(header);

    panel_node = panel_node.with_child(
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
                .with_text_key("app.schematic_empty")
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

fn build_surface_badge(tokens: UiTokens, editor: &NativeElectronicsEditor, panel: UiRect) -> UiNode {
    let label_key = surface_label_key(editor.active_surface());
    let badge_width = BADGE_MIN_WIDTH.max(panel.width * 0.28);
    UiNode::new("electronics.canvas.minimap.badge", UiNodeKind::Panel)
        .with_layout(
            UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                ..UiLayout::absolute(UiRect::new(
                    panel.x + panel.width - badge_width - MINIMAP_PAD,
                    panel.y + (HEADER_HEIGHT - BADGE_HEIGHT) * 0.5,
                    badge_width,
                    BADGE_HEIGHT,
                ))
            },
        )
        .with_style(UiStyle {
            fill: tokens.accent,
            border: tokens.accent_hot,
            text: [18, 18, 20, 255],
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
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
                    color: [18, 18, 20, 255],
                    inherit_color: false,
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

fn label_width(label: &str) -> f32 {
    (label.chars().count() as f32 * 7.0 + 8.0).clamp(MIN_LABEL_WIDTH, MAX_LABEL_WIDTH)
}

fn label_color(text: [u8; 4], muted: [u8; 4], kind: CadObjectKind) -> [u8; 4] {
    match kind {
        CadObjectKind::Pin | CadObjectKind::Pad => muted,
        CadObjectKind::NetLabel => [112, 224, 136, 220],
        CadObjectKind::DrcMarker => [255, 196, 96, 255],
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_root_is_clipped_to_its_canvas_surface() {
        let editor = NativeElectronicsEditor::empty("test");
        let surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            EditorRect::new(356.0, 86.0, 640.0, 480.0),
        );

        assert_eq!(surface.root.kind, UiNodeKind::Overlay);
        assert_eq!(surface.root.layout.overflow, UiOverflow::Clip);
        assert!(surface.root.layout.rect.is_none());
    }

    #[test]
    fn minimap_chrome_exposes_header_image_and_empty_state() {
        let editor = NativeElectronicsEditor::empty("test");
        let surface = build_electronics_canvas_overlay_surface(
            StudioUiPalette::IndustrialDark,
            &editor,
            EditorRect::new(0.0, 0.0, 640.0, 480.0),
        );

        let panel = surface
            .root
            .find("electronics.canvas.minimap")
            .expect("minimap panel");
        assert_eq!(panel.kind, UiNodeKind::Panel);
        assert_eq!(
            panel.tooltip_key.as_deref(),
            Some("electronics.minimap.tooltip")
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
    fn minimap_image_rect_sits_below_header_and_inside_panel() {
        let size = Vec2::new(640.0, 480.0);
        let panel = electronics_minimap::overlay_rect(size);
        let image = electronics_minimap::image_rect(size);
        assert!(image.y >= panel.y + HEADER_HEIGHT - f32::EPSILON);
        assert!(image.x >= panel.x - f32::EPSILON);
        assert!(image.x + image.width <= panel.x + panel.width + f32::EPSILON);
        assert!(image.y + image.height <= panel.y + panel.height + f32::EPSILON);
    }
}
