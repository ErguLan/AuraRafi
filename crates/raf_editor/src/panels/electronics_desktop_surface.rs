//! Electronics-only document chrome. All tools dispatch to the live CAD controller.
use crate::editor_layout::EditorRect;
use crate::electronics_controller::{ElectronicsTool, NativeElectronicsEditor};
use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::*;

pub const DOCUMENT_HEIGHT: f32 = 32.0;
pub const TOOL_RAIL_WIDTH: f32 = 48.0;
pub const RULER_WIDTH: f32 = 32.0;
pub const RULER_HEIGHT: f32 = 22.0;

fn button(
    id: &str,
    label: &str,
    icon: UiIconId,
    command: &str,
    active: bool,
    tokens: UiTokens,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("electronics-desktop-button")
        .with_layout(UiLayout {
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 2.0),
            ..UiLayout::fixed(92.0, 28.0)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Custom(26)))
        .with_text_key(label)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_style(UiStyle {
            fill: if active {
                tokens.selection
            } else {
                tokens.surface_alt
            },
            border: if active { tokens.accent } else { tokens.border },
            text: tokens.text,
            border_width: if active { 1.0 } else { 0.0 },
            radius: 3.0,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(label)
        .with_accessibility_selected(active)
        .with_tooltip_key(label)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub fn build_electronics_document_chrome(
    palette: StudioUiPalette,
    editor: &NativeElectronicsEditor,
    area: EditorRect,
) -> UiSurface {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let toolbar = 0.0;
    let pcb = editor.active_surface() == CadSurfaceKind::Pcb;
    let cad = EditorRect::new(0.0, 0.0, area.width, area.height);
    let viewport = crate::editor_layout::EditorFrameLayout::cad_content_rect(cad, pcb);
    let mut root = UiNode::new("electronics.desktop", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(area.width, area.height))
        .with_style(UiStyle::transparent());
    if pcb {
        root = root.with_child(
            UiNode::new("electronics.desktop.corner", UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(UiRect::new(
                    0.0,
                    DOCUMENT_HEIGHT,
                    TOOL_RAIL_WIDTH + RULER_WIDTH,
                    viewport.y - DOCUMENT_HEIGHT,
                )))
                .with_style(UiStyle::transparent().with_fill(t.surface)),
        );
    }
    let mut documents = UiNode::new("electronics.desktop.documents", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            ..UiLayout::absolute(UiRect::new(0.0, toolbar, area.width, DOCUMENT_HEIGHT))
        })
        .with_style(UiStyle::transparent().with_fill(t.surface_alt));
    let mut document = button(
        "electronics.desktop.document",
        if pcb {
            "app.electronics_pcb_tab"
        } else {
            "app.electronics_schematic_tab"
        },
        if pcb {
            UiIconId::Pcb
        } else {
            UiIconId::Schematic
        },
        if pcb {
            "electronics.mode.pcb"
        } else {
            "electronics.mode.schematic"
        },
        true,
        t,
    );
    document.text_key = None;
    document.text_value = Some(format!(
        "{}  /  {}",
        editor.schematic().name,
        if pcb { "PCB" } else { "SCH" }
    ));
    document.layout.basis[0] = 220.0_f32.min((area.width - 110.0).max(100.0));
    document.style = UiStyle {
        fill: t.surface_raised,
        border: t.border,
        text: t.text,
        border_width: 1.0,
        radius: 0.0,
        opacity: 1.0,
    };
    document.layout.flow = UiFlow::Column;
    document.layout.justify_content = UiJustify::Start;
    document = document
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_child(
            UiNode::new("electronics.desktop.document.edge", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(0.0, 2.0).fill_width())
                .with_style(UiStyle::transparent().with_fill(t.accent)),
        );
    documents = documents.with_child(document).with_child(
        UiNode::new("electronics.desktop.document-spacer", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 24.0).fill_width().grow(1.0)),
    );
    root = root.with_child(documents);
    let mut tools = UiNode::new("electronics.desktop.tools", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(4.0, 8.0),
            ..UiLayout::absolute(UiRect::new(
                0.0,
                viewport.y,
                TOOL_RAIL_WIDTH,
                viewport.height,
            ))
        })
        .with_style(UiStyle::transparent().with_fill(t.surface));
    for (id, key, icon, cmd, tool) in [
        (
            "select",
            "electronics.toolbar.select",
            UiIconId::Select,
            "electronics.select",
            ElectronicsTool::Select,
        ),
        (
            "pan",
            "electronics.toolbar.pan",
            UiIconId::Move,
            "electronics.pan",
            ElectronicsTool::Pan,
        ),
        (
            "connect",
            if pcb {
                "electronics.toolbar.route"
            } else {
                "electronics.toolbar.wire"
            },
            if pcb {
                UiIconId::CadRoute
            } else {
                UiIconId::Wire
            },
            if pcb {
                "electronics.route"
            } else {
                "electronics.wire"
            },
            if pcb {
                ElectronicsTool::Route
            } else {
                ElectronicsTool::Wire
            },
        ),
        (
            "outline",
            "electronics.toolbar.board",
            UiIconId::CadOutline,
            "electronics.board-outline",
            ElectronicsTool::BoardOutline,
        ),
        (
            "place",
            "electronics.toolbar.place",
            UiIconId::CadPlace,
            "electronics.place",
            ElectronicsTool::Place,
        ),
    ] {
        if !pcb && id == "outline" {
            continue;
        }
        let mut item = button(
            &format!("electronics.desktop.tool.{id}"),
            key,
            icon,
            cmd,
            editor.tool() == tool,
            t,
        );
        item.text_key = None;
        item.layout.basis = [36.0, 36.0];
        tools = tools.with_child(item);
    }
    root = root.with_child(tools);

    if editor.pcb_preview_open() {
        let width = (area.width - 70.0).clamp(180.0, 360.0);
        root = root.with_child(
            UiNode::new("electronics.desktop.preview-panel", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 12.0,
                    padding: UiSpacing::same(18.0),
                    ..UiLayout::absolute(UiRect::new(
                        (area.width - width) * 0.5,
                        viewport.y + 36.0,
                        width,
                        184.0,
                    ))
                    .with_z_index(40)
                })
                .with_style(UiStyle {
                    fill: t.surface_raised,
                    border: t.border,
                    text: t.text,
                    border_width: 1.0,
                    radius: 6.0,
                    opacity: 1.0,
                })
                .with_child(
                    UiNode::new("electronics.desktop.preview-title", UiNodeKind::Label)
                        .with_text_key("electronics.desktop.preview_beta")
                        .with_text_style(UiTextStyle::panel_title(t.text))
                        .with_layout(UiLayout::fixed(0.0, 24.0).fill_width()),
                )
                .with_child(
                    UiNode::new("electronics.desktop.preview-copy", UiNodeKind::Label)
                        .with_text_key("electronics.desktop.preview_pending")
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle::body(t.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 62.0).fill_width()),
                )
                .with_child(button(
                    "electronics.desktop.preview-close",
                    "app.close",
                    UiIconId::Close,
                    "electronics.pcb.preview.toggle",
                    false,
                    t,
                )),
        );
    }
    let mut surface = UiSurface::new("electronics.desktop", palette, root);
    for (state, fill, border) in [
        (UiStyleRuleState::Hovered, t.surface_raised, t.border),
        (UiStyleRuleState::Focused, t.surface_raised, t.focus),
    ] {
        surface.style_sheet.rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-desktop-button".into()),
                UiStylePatch {
                    fill: Some(fill),
                    border: Some(border),
                    border_width: Some(1.0),
                    ..Default::default()
                },
            )
            .when(state),
        );
    }
    surface
}

pub fn ruler_overlay(
    editor: &NativeElectronicsEditor,
    size: glam::Vec2,
    tokens: UiTokens,
) -> UiNode {
    let camera = editor.camera();
    let zoom = camera.zoom.max(0.001);
    let raw = 64.0 / zoom;
    let power = 10.0_f32.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|n| n * power)
        .find(|n| *n >= raw)
        .unwrap_or(power * 10.0);
    let corner = camera.world_from_screen(glam::Vec2::ZERO, size);
    let mut root = UiNode::new("electronics.desktop.rulers", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(size.x + RULER_WIDTH, size.y + RULER_HEIGHT))
        .with_style(UiStyle::transparent());
    for (id, rect) in [
        (
            "top",
            UiRect::new(0.0, 0.0, size.x + RULER_WIDTH, RULER_HEIGHT),
        ),
        ("left", UiRect::new(0.0, RULER_HEIGHT, RULER_WIDTH, size.y)),
    ] {
        root = root.with_child(
            UiNode::new(format!("electronics.desktop.ruler.{id}"), UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(rect))
                .with_style(UiStyle::transparent().with_fill(tokens.surface)),
        );
    }
    root = root.with_child(
        UiNode::new("electronics.desktop.ruler.unit", UiNodeKind::Label)
            .with_text_value("mm")
            .with_text_style(UiTextStyle {
                size_px: 10.0,
                line_height_px: 14.0,
                ..UiTextStyle::body(tokens.text_muted)
            })
            .with_layout(UiLayout::absolute(UiRect::new(6.0, 2.0, 24.0, 18.0))),
    );
    for (axis, start, extent) in [(0, corner.x, size.x), (1, corner.y, size.y)] {
        let count = ((extent / (step * zoom)).ceil() as i32 + 2).clamp(0, 64);
        for i in 0..count {
            let value = (start / step).ceil() * step + i as f32 * step;
            let at = (value - start) * zoom;
            if at < 24.0 || at > extent {
                continue;
            }
            let (x, y, w, h) = if axis == 0 {
                (at + RULER_WIDTH + 3.0, 1.0, 48.0, 17.0)
            } else {
                (2.0, at + RULER_HEIGHT + 2.0, 28.0, 17.0)
            };
            let precision = (-step.log10().floor()).clamp(0.0, 6.0) as usize;
            let value = if value.abs() < step * 0.0001 {
                0.0
            } else {
                value
            };
            let label = format!("{value:.precision$}");
            root = root.with_child(
                UiNode::new(
                    format!("electronics.desktop.ruler.{axis}.{i}"),
                    UiNodeKind::Label,
                )
                .with_layout(UiLayout::absolute(UiRect::new(x, y, w, h)))
                .with_text_value(label)
                .with_text_style(UiTextStyle {
                    size_px: 10.0,
                    line_height_px: 14.0,
                    ..UiTextStyle::body(tokens.text_muted)
                })
                .with_text_overflow(UiTextOverflow::Clip),
            );
            for minor in 0..5 {
                let pos = at + minor as f32 * step * zoom / 5.0;
                if pos > extent {
                    break;
                }
                let length = if minor == 0 { 7.0 } else { 3.0 };
                let rect = if axis == 0 {
                    UiRect::new(pos + RULER_WIDTH, RULER_HEIGHT - length, 1.0, length)
                } else {
                    UiRect::new(RULER_WIDTH - length, pos + RULER_HEIGHT, length, 1.0)
                };
                root = root.with_child(
                    UiNode::new(
                        format!("electronics.desktop.ruler.tick.{axis}.{i}.{minor}"),
                        UiNodeKind::Separator,
                    )
                    .with_layout(UiLayout::absolute(rect))
                    .with_style(UiStyle::transparent().with_fill(tokens.border)),
                );
            }
        }
    }
    root
}
