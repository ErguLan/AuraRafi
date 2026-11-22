//! Thumbnail, preview-state and capability decoration for Assets cards.
//!
//! Moved from the surface facade without behavior changes.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode,
    UiNodeKind,
};

use super::{AssetViewMode, AssetVisual};

pub(super) fn find_node_mut<'a>(node: &'a mut UiNode, id: &str) -> Option<&'a mut UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children
        .iter_mut()
        .find_map(|child| find_node_mut(child, id))
}

pub(super) fn decorate_asset_card(
    mut card: UiNode,
    visual: Option<&AssetVisual>,
    view: AssetViewMode,
    language: raf_core::Language,
    palette: StudioUiPalette,
) -> UiNode {
    use crate::asset_previews::PreviewState;
    let Some(visual) = visual else {
        return card;
    };
    if view == AssetViewMode::Grid {
        if let Some(icon) = card
            .children
            .iter_mut()
            .find(|node| node.id.ends_with(".icon"))
        {
            icon.layout.basis[1] = 64.0;
        }
        let badge = match &visual.preview {
            PreviewState::Loading => Some((
                "assets.preview_loading",
                raf_ui::components::UiStatusTone::Info,
                UiIconId::Refresh,
            )),
            PreviewState::Error(_) => Some((
                "assets.preview_error",
                raf_ui::components::UiStatusTone::Error,
                UiIconId::Warning,
            )),
            PreviewState::Unavailable(key) => Some((
                *key,
                raf_ui::components::UiStatusTone::Warning,
                UiIconId::Warning,
            )),
            PreviewState::Ready { .. } => None,
        };
        if let Some((label_key, tone, icon)) = badge {
            let theme = raf_ui::UiThemeMetrics::default();
            let metrics = theme.for_control(raf_ui::style::UiControlKind::Button);
            let mut badge = raf_ui::components::status_badge(
                format!("{}.preview-state", card.id),
                label_key,
                tone,
                palette,
                theme,
            );
            badge.icon = Some(UiIcon::new(icon).with_size(UiIconSize::Small));
            badge.layout.position_mode = raf_ui::UiPositionMode::Absolute;
            badge.layout.rect = Some(raf_ui::UiRect::new(8.0, 8.0, 0.0, metrics.height));
            badge.layout.width_mode = raf_ui::UiSizeMode::FitContent;
            badge.layout.height_mode = raf_ui::UiSizeMode::Fixed;
            badge.layout.basis = [0.0, metrics.height];
            badge.layout.z_index = 1;
            card.children.push(badge);
        }
    }
    if let raf_assets::authoring_catalog::AssetClass::Shape(shape) = visual.entry.class {
        if let Some(label) = card
            .children
            .iter_mut()
            .find(|node| node.id.ends_with(".label"))
        {
            label.text_value = None;
            label.text_key = Some(shape.descriptor().label_key.to_string());
        }
        card.accessibility_label_key = Some(shape.descriptor().label_key.to_string());
    }
    if let Some(kind)=visual.entry.path.strip_prefix("builtin://interface/") {
        if let Some(label)=card.children.iter_mut().find(|n|n.id.ends_with(".label")){label.text_value=None;label.text_key=Some(format!("ui.element.{kind}"));}
        card.accessibility_label_key=Some(format!("ui.element.{kind}"));
    }
    let mut info = format!(
        "{}\n{}: {}",
        visual.entry.path,
        raf_core::i18n::t("assets.usages", language),
        visual.entry.usages
    );
    let (state, detail) = match &visual.preview {
        PreviewState::Ready { key, dimensions } => {
            if let Some(icon) = card.children.iter_mut().find(|n| n.id.ends_with(".icon")) {
                raf_ui::components::set_thumbnail_image(
                    icon,
                    key.clone(),
                    raf_ui::UiImageFit::Contain,
                    raf_ui::UiThemeMetrics::default()
                        .for_control(raf_ui::style::UiControlKind::Image)
                        .corner_radius,
                );
                if view == AssetViewMode::Grid {
                    icon.layout.basis[1] = 64.0;
                }
            }
            let dimensions = dimensions
                .or(visual.entry.dimensions)
                .map(|s| format!("\n{:.2} × {:.2} × {:.2} m", s[0], s[1], s[2]))
                .unwrap_or_default();
            ("assets.preview_ready", dimensions)
        }
        PreviewState::Loading => ("assets.preview_loading", String::new()),
        PreviewState::Error(error) => ("assets.preview_error", format!("\n{error}")),
        PreviewState::Unavailable(key) => (*key, String::new()),
    };
    info.push_str(&format!(
        "\n{}{}",
        raf_core::i18n::t(state, language),
        detail
    ));
    card.tooltip_value = Some(info);
    card
}

pub(super) fn append_asset_actions(bar: &mut UiNode, visual: &AssetVisual, palette: StudioUiPalette) {
    use raf_assets::authoring_catalog::AssetClass;
    let entry = &visual.entry;
    let mut actions = Vec::new();
    if entry.class == AssetClass::Animation {
        actions.push(("animation.open", UiIconId::Play, format!("assets.open:{}", entry.path)));
    }
    if entry.class == AssetClass::Image {
        actions.push(("ui.designer.use_image",UiIconId::Frame,format!("ui.designer.asset-image:{}",entry.path)));
    }
    if entry.insertable() {
        actions.push((
            "assets.insert",
            UiIconId::Add,
            format!("assets.insert:{}", entry.path),
        ));
    }
    if entry.class == AssetClass::Image && visual.can_apply_texture {
        actions.push((
            "assets.apply_texture",
            UiIconId::Shaded,
            format!("assets.apply-texture:{}", entry.path),
        ));
    }
    actions.push((
        "assets.view_usages",
        UiIconId::Search,
        format!("assets.usages:{}", entry.path),
    ));
    for (key, icon, command) in actions {
        bar.children.push(
            UiNode::new(format!("assets.capability.{key}"), UiNodeKind::Button)
                .with_class("asset-card-action")
                .with_layout(UiLayout::fixed(26.0, 24.0))
                .with_icon(UiIcon::new(icon).with_tint(palette.tokens().text))
                .with_tooltip_key(key)
                .with_accessibility_label_key(key)
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }
}
