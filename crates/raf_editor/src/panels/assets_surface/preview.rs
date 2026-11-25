//! Thumbnail, preview-state and capability decoration for Assets cards.
//!
//! Moved from the surface facade without behavior changes.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode,
    UiNodeKind, UiStyle,
};

use super::{AssetViewMode, AssetVisual};

/// Honest card-level state derived from the preview pipeline. The panel does
/// not invent an importer state: loading means a preview job is queued,
/// missing means the catalog row has no file, and error carries a real failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetCardLifecycle {
    Importing,
    Ready,
    Missing,
    Error,
}

impl AssetCardLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Error => "error",
        }
    }
}

pub fn asset_card_lifecycle(preview: &crate::asset_previews::PreviewState) -> AssetCardLifecycle {
    use crate::asset_previews::PreviewState;
    match preview {
        PreviewState::Loading => AssetCardLifecycle::Importing,
        PreviewState::Ready { .. } => AssetCardLifecycle::Ready,
        PreviewState::Missing => AssetCardLifecycle::Missing,
        PreviewState::Error(_) => AssetCardLifecycle::Error,
        PreviewState::Unavailable(_) => AssetCardLifecycle::Missing,
    }
}

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
    let lifecycle = asset_card_lifecycle(&visual.preview);
    card = card.with_class(format!("asset-card-state-{}", lifecycle.as_str()));
    if view == AssetViewMode::Grid {
        if let Some(icon) = card
            .children
            .iter_mut()
            .find(|node| node.id.ends_with(".icon"))
        {
            icon.layout.basis[1] = 64.0;
        }
        // Built-in lights are authoring shortcuts, not files with previews:
        // showing "no preview" on them would be noise, not information.
        let badge = if super::is_builtin_light_row(&visual.entry.path) {
            None
        } else {
            match &visual.preview {
                PreviewState::Loading => Some((
                    "assets.preview_loading",
                    raf_ui::components::UiStatusTone::Info,
                    UiIconId::Refresh,
                )),
                PreviewState::Missing => Some((
                    "assets.missing_file",
                    raf_ui::components::UiStatusTone::Error,
                    UiIconId::Warning,
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
            }
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
            badge.tooltip_key = Some(label_key.to_string());
            badge.layout.position_mode = raf_ui::UiPositionMode::Absolute;
            badge.layout.rect = Some(raf_ui::UiRect::new(8.0, 8.0, 0.0, metrics.height));
            badge.layout.width_mode = raf_ui::UiSizeMode::FitContent;
            badge.layout.height_mode = raf_ui::UiSizeMode::Fixed;
            badge.layout.basis = [0.0, metrics.height];
            badge.layout.z_index = 1;
            card.children.push(badge);
        } else if matches!(visual.preview, PreviewState::Ready { .. })
            && !super::is_builtin_light_row(&visual.entry.path)
            && visual.entry.insertable()
        {
            // Ready cards expose the real authoring action instead of a passive
            // success dot: a compact translucent-white plus button.
            let mut add = UiNode::new(format!("{}.preview-add", card.id), UiNodeKind::Button)
                .with_class("asset-card-add")
                .with_icon(
                    UiIcon::new(UiIconId::Add)
                        .with_size(UiIconSize::Small)
                        .with_tint([255, 255, 255, 242]),
                )
                .with_tooltip_key("assets.insert")
                .with_accessibility_role(raf_ui::UiAccessibilityRole::Button)
                .with_accessibility_label_key("assets.insert")
                .focusable()
                .with_style(UiStyle {
                    fill: [255, 255, 255, 34],
                    border: [255, 255, 255, 112],
                    text: [255, 255, 255, 242],
                    border_width: 1.0,
                    radius: 12.0,
                    opacity: 1.0,
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    super::catalog::asset_open_command(&visual.entry.path),
                ));
            add.layout.position_mode = raf_ui::UiPositionMode::Absolute;
            add.layout.rect = Some(raf_ui::UiRect::new(10.0, 8.0, 24.0, 24.0));
            add.layout.width_mode = raf_ui::UiSizeMode::Fixed;
            add.layout.height_mode = raf_ui::UiSizeMode::Fixed;
            add.layout.basis = [24.0, 24.0];
            add.layout.z_index = 1;
            card.children.push(add);
        }
    }
    if let raf_assets::authoring_catalog::AssetClass::Shape(shape) = visual.entry.class {
        if let Some(label) = card
            .children
            .iter_mut()
            .find(|node| node.id.ends_with(".label"))
        {
            let label_key = shape.descriptor().label_key;
            if raf_core::i18n::t(label_key, language) == label_key {
                // Locale key not authored yet (wedge/torus/prism/sphere2d):
                // keep the readable literal instead of painting the raw key.
                label.text_value = Some(super::primitive_card_name(shape).to_string());
                label.text_key = None;
            } else {
                label.text_value = None;
                label.text_key = Some(label_key.to_string());
            }
        }
        let label_key = shape.descriptor().label_key;
        card.accessibility_label_key =
            Some(if raf_core::i18n::t(label_key, language) == label_key {
                super::primitive_card_name(shape).to_string()
            } else {
                label_key.to_string()
            });
    }
    if let Some(kind) = visual.entry.path.strip_prefix("builtin://interface/") {
        if let Some(label) = card.children.iter_mut().find(|n| n.id.ends_with(".label")) {
            label.text_value = None;
            label.text_key = Some(format!("ui.element.{kind}"));
        }
        card.accessibility_label_key = Some(format!("ui.element.{kind}"));
    }
    let mut info = format!(
        "{}\n{}: {}",
        visual.entry.path,
        raf_core::i18n::t("assets.usages", language),
        visual.entry.usages,
    );
    if visual.entry.class == raf_assets::authoring_catalog::AssetClass::Audio {
        if let Some(meta) = visual.entry.audio_metadata() {
            info.push_str(&format!(
                "\n{:.2} s · {} ch · {} Hz · {} bit",
                meta.duration_seconds, meta.channels, meta.sample_rate, meta.bits_per_sample
            ));
        }
    }
    if super::is_builtin_light_row(&visual.entry.path) {
        info.push_str(&format!(
            "\n{}",
            raf_core::i18n::t("assets.insert", language)
        ));
        card.tooltip_value = Some(info);
        return card;
    }
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
                .map(|s| format!("{:.2} × {:.2} × {:.2} m", s[0], s[1], s[2]))
                .unwrap_or_else(|| "—".to_string());
            (
                "assets.preview_ready",
                format!(
                    "\n{}: {}",
                    raf_core::i18n::t("app.dimensions", language),
                    dimensions
                ),
            )
        }
        PreviewState::Loading => ("assets.preview_loading", String::new()),
        PreviewState::Missing => ("assets.missing_file", String::new()),
        PreviewState::Error(error) => ("assets.preview_error", format!("\n{error}")),
        PreviewState::Unavailable(key) => (*key, String::new()),
    };
    if matches!(
        visual.entry.class,
        raf_assets::authoring_catalog::AssetClass::Model
    ) {
        // `AuthoringAsset` currently exposes dimensions only. Triangles and
        // materials stay an explicit dash until the catalog carries them.
        let dimensions = match &visual.preview {
            PreviewState::Ready {
                dimensions: Some(dimensions),
                ..
            } => Some(*dimensions),
            _ => visual.entry.dimensions,
        }
        .map(|s| format!("{:.2} × {:.2} × {:.2} m", s[0], s[1], s[2]))
        .unwrap_or_else(|| "—".to_string());
        info.push_str(&format!(
            "\n{}: {}\nTriangles: —\nMaterials: —",
            raf_core::i18n::t("app.dimensions", language),
            dimensions
        ));
    }
    info.push_str(&format!(
        "\n{}{}",
        raf_core::i18n::t(state, language),
        detail
    ));
    card.tooltip_value = Some(info);
    card
}

pub(super) fn append_asset_actions(
    bar: &mut UiNode,
    visual: &AssetVisual,
    palette: StudioUiPalette,
) {
    use raf_assets::authoring_catalog::AssetClass;
    let entry = &visual.entry;
    let mut actions = Vec::new();
    if entry.class == AssetClass::Model
        && matches!(
            entry
                .path
                .rsplit('.')
                .next()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("glb" | "gltf")
        )
    {
        actions.push((
            "animation.import_rig",
            UiIconId::Play,
            format!("assets.place-rig:{}", entry.path),
        ));
    }
    if entry.class == AssetClass::Animation {
        actions.push((
            "animation.open",
            UiIconId::Play,
            format!("assets.open:{}", entry.path),
        ));
    }
    if entry.class == AssetClass::Image {
        actions.push((
            "ui.designer.use_image",
            UiIconId::Frame,
            format!("ui.designer.asset-image:{}", entry.path),
        ));
    }
    if entry.class == AssetClass::Audio {
        actions.push((
            "assets.audio_preview",
            UiIconId::Play,
            format!("assets.audio.preview:{}", entry.path),
        ));
    }
    // Virtual lights are not `insertable()` in the catalog, but the host can
    // create them from the documented `hierarchy.create-light:*` command.
    if let Some(command) = super::builtin_asset_command(&entry.path) {
        actions.push(("assets.insert", UiIconId::Add, command));
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
