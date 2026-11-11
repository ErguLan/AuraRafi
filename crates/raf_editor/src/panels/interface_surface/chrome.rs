//! Scoped neutral authoring chrome; authored colors remain owned by the document.
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::*;

pub const ROW_HEIGHT: f32 = 30.0;
pub const GAP: f32 = 8.0;
pub const INSET: f32 = 12.0;

pub fn components(t: UiTokens) -> UiComponents {
    let mut metrics = UiTheme::raf_ui().metrics;
    metrics.control_height = ROW_HEIGHT;
    metrics.spacing = GAP;
    UiComponents::new(t, metrics)
}

pub fn tokens(palette: StudioUiPalette) -> UiTokens {
    let mut t = palette.tokens();
    let gray = |v| [v, v, v, 255];
    let dark = palette == StudioUiPalette::IndustrialDark;
    t.background = gray(if dark { 19 } else { 242 });
    t.surface = gray(if dark { 25 } else { 250 });
    t.surface_alt = gray(if dark { 31 } else { 238 });
    t.surface_raised = gray(if dark { 39 } else { 255 });
    t.border = gray(if dark { 57 } else { 207 });
    t.text = gray(if dark { 241 } else { 35 });
    t.text_muted = gray(if dark { 162 } else { 98 });
    t.accent = t.text;
    t.focus = gray(if dark { 220 } else { 65 });
    t.selection = gray(if dark { 61 } else { 222 });
    t
}
pub fn label(id: impl Into<String>, key: &str, t: UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(key)
        .with_style(UiStyle {
            text: t.text_muted,
            ..UiStyle::transparent()
        })
        .with_text_style(UiTextStyle::label(t.text_muted))
        .with_layout(UiLayout::fit_content())
}
pub fn value(id: impl Into<String>, text: impl Into<String>, t: UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_value(text)
        .with_style(UiStyle {
            text: t.text,
            ..UiStyle::transparent()
        })
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout::fit_content())
}
pub fn hint(id: impl Into<String>, key: &str, width: f32, t: UiTokens) -> UiNode {
    label(id, key, t)
        .with_layout(UiLayout::fit_content().fixed_width(width.max(1.0)))
        .with_text_overflow(UiTextOverflow::Wrap)
}
pub fn button(
    id: impl Into<String>,
    key: &str,
    command: impl Into<String>,
    icon: Option<UiIconId>,
    active: bool,
    t: UiTokens,
) -> UiNode {
    let mut node = components(t).button(id, key,
        UiAction::Command { name: command.into() }, UiButtonVariant::Quiet)
        .with_class("interface.button")
        .with_accessibility_selected(active);
    if let Some(icon) = icon {
        node = node.with_icon(UiIcon::new(icon).with_size(UiIconSize::Toolbar));
    }
    node
}
pub fn icon_button(
    id: impl Into<String>,
    key: &str,
    command: impl Into<String>,
    icon: UiIconId,
    t: UiTokens,
) -> UiNode {
    let mut node = button(id, key, command, Some(icon), false, t);
    node.text_key = None;
    node.accessibility_label_key = Some(key.into());
    node.layout = UiLayout::fixed(ROW_HEIGHT, ROW_HEIGHT);
    node
}
pub fn row(id: impl Into<String>) -> UiNode {
    UiNode::row(id, []).fit_content().with_gap(GAP).with_alignment(UiAlign::Center)
}
pub fn column(id: impl Into<String>) -> UiNode {
    UiNode::column(id, []).fill_width().fill_height().with_gap(GAP)
}
pub fn surface(id: &str, palette: StudioUiPalette, root: UiNode) -> UiSurface {
    let t = tokens(palette);
    UiSurface::new(id, palette, root).with_style_sheet(components(t).style_sheet())
}
pub fn element_icon(node: &UiNode) -> UiIconId {
    match node.kind {
        UiNodeKind::Label => UiIconId::File,
        UiNodeKind::Image => UiIconId::Assets,
        UiNodeKind::Button => UiIconId::Select,
        UiNodeKind::TextInput => UiIconId::Pencil,
        _ => UiIconId::Frame,
    }
}
