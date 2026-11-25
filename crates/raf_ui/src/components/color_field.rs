//! Shared color-row recipe: swatch trigger, HSV popover, hex, RGB, recents
//! and viewport eyedropper hook. Hosts own the actual color mutation.

use crate::controls::{UiColorPicker, UiRange, UiTextInput};
use crate::events::{UiAction, UiEventBinding, UiEventKind};
use crate::geometry::{UiRect, UiSpacing};
use crate::icons::{UiIcon, UiIconId, UiIconSize};
use crate::layout::{UiAlign, UiFlow, UiLayout, UiSizeMode};
use crate::node::{UiNode, UiNodeKind};
use crate::style::{
    StudioUiPalette, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiSurfaceMaterial,
};
use crate::text::{UiFontWeight, UiTextOverflow, UiTextStyle};
use crate::{UiImage, UiImageFit, UiImageSource};

const COLOR_PICKER_POPOVER_PADDING: f32 = 10.0;
const COLOR_PICKER_POPOVER_GAP: f32 = 7.0;
const COLOR_PICKER_VISUAL_SIZE: f32 = 210.0;
const COLOR_PICKER_IMAGE_SIZE: f32 = UiColorPicker::CANVAS_SIZE;
const COLOR_PICKER_SQUARE_START: f32 = UiColorPicker::SQUARE_START;
const COLOR_PICKER_SQUARE_SIDE: f32 = UiColorPicker::SQUARE_SIDE;
const COLOR_PICKER_RING_RADIUS: f32 =
    (UiColorPicker::RING_INNER_RADIUS + UiColorPicker::RING_OUTER_RADIUS) * 0.5;
pub const COLOR_PICKER_POPOVER_WIDTH: f32 = 292.0;
pub const COLOR_PICKER_POPOVER_HEIGHT: f32 = 420.0;

#[derive(Debug, Clone)]
pub struct UiColorFieldSpec {
    pub id: String,
    pub label_key: String,
    pub path: String,
    pub command_prefix: String,
    pub color: [u8; 3],
    pub open: bool,
    pub modified: bool,
    pub recent: Vec<[u8; 3]>,
    pub eyedropper_active: bool,
}

impl UiColorFieldSpec {
    pub fn new(
        id: impl Into<String>,
        label_key: impl Into<String>,
        path: impl Into<String>,
        color: [u8; 3],
    ) -> Self {
        Self {
            id: id.into(),
            label_key: label_key.into(),
            path: path.into(),
            command_prefix: "lighting.color".to_string(),
            color,
            open: false,
            modified: false,
            recent: Vec::new(),
            eyedropper_active: false,
        }
    }

    pub fn with_open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn with_modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    pub fn with_recent(mut self, recent: Vec<[u8; 3]>) -> Self {
        self.recent = recent;
        self
    }

    pub fn with_eyedropper_active(mut self, active: bool) -> Self {
        self.eyedropper_active = active;
        self
    }

    fn command(&self, suffix: &str) -> String {
        format!("{}.{}:{}", self.command_prefix, suffix, self.path)
    }

    fn picker_key(&self) -> String {
        format!("{}.{}", self.command_prefix, self.path)
    }

    fn hex_key(&self) -> String {
        format!("{}.{}.hex", self.command_prefix, self.path)
    }

    fn channel_key(&self, channel: &str) -> String {
        format!("{}.{}.{}", self.command_prefix, self.path, channel)
    }
}

/// Renders the swatch trigger and label row only. The host composes
/// [`color_popover`] as a separate overlay so the picker is never clipped by a
/// scrolling inspector.
pub fn color_field(palette: StudioUiPalette, spec: &UiColorFieldSpec) -> UiNode {
    let tokens = palette.tokens();
    let hex = color_hex(spec.color);
    let mut root = UiNode::new(spec.id.clone(), UiNodeKind::Panel)
        .with_class("ui-color-field")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    let mut header =
        UiNode::new(format!("{}.header", spec.id), UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        });
    header = header.with_child(
        UiNode::new(format!("{}.label", spec.id), UiNodeKind::Label)
            .with_text_key(spec.label_key.clone())
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            }),
    );
    header = header.with_child(icon_action(
        format!("{}.copy", spec.id),
        UiIconId::Copy,
        "app.copy",
        UiAction::SetClipboard { text: hex.clone() },
    ));
    header = header.with_child(icon_command(
        format!("{}.paste", spec.id),
        UiIconId::File,
        "app.paste",
        spec.command("paste"),
    ));
    if spec.modified {
        header = header.with_child(icon_command(
            format!("{}.reset", spec.id),
            UiIconId::Refresh,
            "lighting.reset",
            spec.command("reset"),
        ));
    }
    root = root.with_child(header);

    let mut trigger = UiNode::new(format!("{}.trigger", spec.id), UiNodeKind::Button)
        .with_class(if spec.open {
            "ui-color-trigger ui-color-trigger-open"
        } else {
            "ui-color-trigger"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .focusable()
        .with_accessibility_label_key(spec.label_key.clone())
        .with_tooltip_key(spec.label_key.clone())
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            spec.command("toggle"),
        ))
        .with_child(
            UiNode::new(format!("{}.swatch", spec.id), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(48.0, 22.0))
                .with_style(UiStyle {
                    fill: [spec.color[0], spec.color[1], spec.color[2], 255],
                    border: tokens.border,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 4.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new(format!("{}.hex", spec.id), UiNodeKind::Label)
                .with_text_value(hex)
                .with_text_style(UiTextStyle {
                    role: crate::text::UiTextRole::Monospace,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_child(
            UiNode::new(format!("{}.chevron", spec.id), UiNodeKind::Label)
                .with_icon(UiIcon::new(if spec.open {
                    UiIconId::ChevronDown
                } else {
                    UiIconId::ChevronRight
                }))
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    if spec.open {
        trigger = trigger.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            spec.command("close"),
        ));
    }
    root = root.with_child(trigger);
    root
}

/// Full HSV/hex/RGB/recent picker body. Compose it in an overlay surface.
pub fn color_popover(palette: StudioUiPalette, spec: &UiColorFieldSpec) -> UiNode {
    let tokens = palette.tokens();
    let hsv = rgb_to_hsv(spec.color);
    let mut popover = UiNode::new(format!("{}.popover", spec.id), UiNodeKind::Menu)
        .with_class("ui-color-popover")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: COLOR_PICKER_POPOVER_GAP,
            padding: UiSpacing::same(COLOR_PICKER_POPOVER_PADDING),
            ..UiLayout::fill(UiFlow::Column)
        });

    let mut header = UiNode::new(format!("{}.popover.header", spec.id), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{}.popover.title", spec.id), UiNodeKind::Label)
                .with_text_key(spec.label_key.clone())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_child(
            UiNode::new(format!("{}.space", spec.id), UiNodeKind::Label)
                .with_text_value("sRGB")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(34.0, 16.0)),
        );
    let mut eyedropper = UiNode::new(format!("{}.eyedropper", spec.id), UiNodeKind::Button)
        .with_class(if spec.eyedropper_active {
            "ui-color-eyedropper ui-color-eyedropper-active"
        } else {
            "ui-color-eyedropper"
        })
        .with_layout(UiLayout::fixed(24.0, 22.0))
        .with_icon(
            UiIcon::new(UiIconId::Eye)
                .with_size(UiIconSize::Custom(13))
                .with_tint(if spec.eyedropper_active {
                    tokens.accent_hot
                } else {
                    tokens.text_muted
                }),
        )
        .with_tooltip_key("lighting.eyedropper")
        .with_accessibility_label_key("lighting.eyedropper")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            spec.command("eyedropper"),
        ));
    if spec.eyedropper_active {
        eyedropper = eyedropper.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            spec.command("eyedropper"),
        ));
    }
    header = header.with_child(eyedropper);
    header = header.with_child(icon_command(
        format!("{}.close", spec.id),
        UiIconId::Close,
        "app.close",
        spec.command("close"),
    ));
    popover = popover.with_child(header);

    popover = popover.with_child(color_visual(palette, spec, hsv));
    popover = popover.with_child(
        UiNode::text_input(
            spec.hex_key(),
            UiTextInput {
                value_key: spec.hex_key(),
                placeholder_key: Some("lighting.hex_color".to_string()),
                max_length: 9,
                multiline: false,
                password: false,
                submit_command: Some(spec.command("hex.commit")),
            },
        )
        .with_class("ui-color-hex-input")
        .with_layout(UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill))
        .with_text_style(UiTextStyle {
            role: crate::text::UiTextRole::Monospace,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Regular,
            color: tokens.text,
            inherit_color: false,
        })
        .with_accessibility_label_key("lighting.hex_color"),
    );

    if !spec.recent.is_empty() {
        let mut recents = UiNode::new(format!("{}.recents", spec.id), UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 4.0,
                ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(format!("{}.recents.label", spec.id), UiNodeKind::Label)
                    .with_text_key("lighting.recent_colors")
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            );
        for (index, color) in spec.recent.iter().take(8).enumerate() {
            recents = recents.with_child(
                UiNode::new(format!("{}.recent.{index}", spec.id), UiNodeKind::Button)
                    .with_class("ui-color-recent")
                    .with_layout(UiLayout::fixed(20.0, 20.0))
                    .with_style(UiStyle {
                        fill: [color[0], color[1], color[2], 255],
                        border: tokens.border,
                        text: tokens.text,
                        border_width: 1.0,
                        radius: 3.0,
                        opacity: 1.0,
                    })
                    .with_tooltip_value(color_hex(*color))
                    .with_accessibility_label_key("lighting.use_recent_color")
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("{}.recent:{}:{index}", spec.command_prefix, spec.path),
                    )),
            );
        }
        popover = popover.with_child(recents);
    }

    for (channel, label_key, value) in [
        ("r", "app.color_r", spec.color[0]),
        ("g", "app.color_g", spec.color[1]),
        ("b", "app.color_b", spec.color[2]),
    ] {
        popover = popover.with_child(channel_row(palette, spec, channel, label_key, value));
    }
    popover
}

fn color_visual(palette: StudioUiPalette, spec: &UiColorFieldSpec, hsv: HsvColor) -> UiNode {
    let tokens = palette.tokens();
    let scale = COLOR_PICKER_VISUAL_SIZE / COLOR_PICKER_IMAGE_SIZE;
    let center = COLOR_PICKER_IMAGE_SIZE * 0.5 * scale;
    let ring_radius = COLOR_PICKER_RING_RADIUS * scale;
    let square_start = COLOR_PICKER_SQUARE_START * scale;
    let square_extent = (COLOR_PICKER_SQUARE_SIDE * scale - scale).max(0.0);
    let hue_angle = hsv.hue.to_radians() - std::f32::consts::FRAC_PI_2;
    let hue_marker_center = [
        center + hue_angle.cos() * ring_radius,
        center + hue_angle.sin() * ring_radius,
    ];
    let saturation_marker_center = [
        square_start + hsv.saturation * square_extent,
        square_start + (1.0 - hsv.value) * square_extent,
    ];
    let mut visual = UiNode::new(format!("{}.visual", spec.id), UiNodeKind::Panel)
        .with_class("ui-color-visual")
        .with_layout(UiLayout {
            align_self: Some(UiAlign::Center),
            ..UiLayout::fixed(COLOR_PICKER_VISUAL_SIZE, COLOR_PICKER_VISUAL_SIZE)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::image(
                format!("{}.visual.image", spec.id),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(format!(
                        "builtin://color-picker/hsv/{:.0}",
                        hsv.hue
                    )),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(
                COLOR_PICKER_VISUAL_SIZE,
                COLOR_PICKER_VISUAL_SIZE,
            )),
        );
    let mut surface = UiNode::color_picker(
        format!("{}.visual.surface", spec.id),
        UiColorPicker::new(spec.picker_key(), hsv.hue, hsv.saturation, hsv.value),
    )
    .with_class("ui-color-surface")
    .with_layout(
        UiLayout::absolute(UiRect::new(
            0.0,
            0.0,
            COLOR_PICKER_VISUAL_SIZE,
            COLOR_PICKER_VISUAL_SIZE,
        ))
        .with_z_index(3),
    )
    .with_style(UiStyle::transparent())
    .with_tooltip_key(spec.label_key.clone())
    .with_accessibility_label_key(spec.label_key.clone())
    .with_event(UiEventBinding::command(
        UiEventKind::PointerDown(crate::UiPointerButton::Primary),
        spec.command("gesture.begin"),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::PointerUp(crate::UiPointerButton::Primary),
        spec.command("gesture.end"),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        spec.command("gesture.end"),
    ));
    surface = surface.with_child(marker(
        format!("{}.visual.sv-marker.outer", spec.id),
        saturation_marker_center,
        16.0,
        [0, 0, 0, 190],
    ));
    surface = surface.with_child(marker(
        format!("{}.visual.sv-marker.inner", spec.id),
        saturation_marker_center,
        10.0,
        [255, 255, 255, 255],
    ));
    surface = surface.with_child(marker(
        format!("{}.visual.hue-marker", spec.id),
        hue_marker_center,
        14.0,
        [255, 255, 255, 255],
    ));
    visual = visual.with_child(surface);
    visual
}

fn marker(id: String, center: [f32; 2], size: f32, fill: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(
            UiLayout::absolute(UiRect::new(
                center[0] - size * 0.5,
                center[1] - size * 0.5,
                size,
                size,
            ))
            .with_z_index(4),
        )
        .with_style(UiStyle {
            fill,
            border: [0, 0, 0, 70],
            text: [255; 4],
            border_width: 1.0,
            radius: size * 0.5,
            opacity: 1.0,
        })
}

fn channel_row(
    palette: StudioUiPalette,
    spec: &UiColorFieldSpec,
    channel: &str,
    label_key: &str,
    value: u8,
) -> UiNode {
    let tokens = palette.tokens();
    let key = spec.channel_key(channel);
    let range = UiNode::range(
        format!("{}.channel.{channel}", spec.id),
        UiRange::new(key, f32::from(value), 0.0, 255.0, 1.0),
    )
    .with_class("ui-color-channel")
    .with_layout(UiLayout {
        grow: 1.0,
        min_size: [80.0, 18.0],
        ..UiLayout::fixed(0.0, 18.0)
    })
    .with_event(UiEventBinding::command(
        UiEventKind::PointerDown(crate::UiPointerButton::Primary),
        spec.command("gesture.begin"),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::PointerUp(crate::UiPointerButton::Primary),
        spec.command("gesture.end"),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        spec.command("gesture.end"),
    ));
    UiNode::new(
        format!("{}.channel-row.{channel}", spec.id),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        ..UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("{}.channel-label.{channel}", spec.id),
            UiNodeKind::Label,
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle {
            size_px: 11.0,
            line_height_px: 16.0,
            ..UiTextStyle::body(tokens.text_muted)
        })
        .with_layout(UiLayout::fixed(16.0, 16.0)),
    )
    .with_child(range)
    .with_child(
        UiNode::new(
            format!("{}.channel-value.{channel}", spec.id),
            UiNodeKind::Label,
        )
        .with_text_value(value.to_string())
        .with_text_style(UiTextStyle {
            role: crate::text::UiTextRole::Monospace,
            size_px: 10.0,
            line_height_px: 14.0,
            weight: UiFontWeight::Regular,
            color: tokens.text_muted,
            inherit_color: false,
        })
        .with_layout(UiLayout::fixed(26.0, 16.0)),
    )
}

fn icon_command(
    id: impl Into<String>,
    icon: UiIconId,
    tooltip_key: &str,
    command: String,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("ui-field-icon")
        .with_layout(UiLayout::fixed(20.0, 20.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Custom(12)))
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn icon_action(
    id: impl Into<String>,
    icon: UiIconId,
    tooltip_key: &str,
    action: UiAction,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("ui-field-icon")
        .with_layout(UiLayout::fixed(20.0, 20.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Custom(12)))
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action,
        })
}

fn color_hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

#[derive(Debug, Clone, Copy)]
struct HsvColor {
    hue: f32,
    saturation: f32,
    value: f32,
}

fn rgb_to_hsv(color: [u8; 3]) -> HsvColor {
    let r = f32::from(color[0]) / 255.0;
    let g = f32::from(color[1]) / 255.0;
    let b = f32::from(color[2]) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    HsvColor {
        hue: hue.rem_euclid(360.0),
        saturation: if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        },
        value: max,
    }
}

pub fn color_field_rules(palette: StudioUiPalette) -> Vec<UiStyleRule> {
    let tokens = palette.tokens();
    vec![
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-trigger".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_alt),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(4.0),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-trigger".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.accent),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-trigger-open".to_string()),
            UiStylePatch {
                fill: Some([116, 67, 24, 54]),
                border: Some(tokens.accent_hot),
                border_width: Some(1.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-popover".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(6.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-hex-input".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_alt),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(3.0),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-eyedropper".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(3.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-eyedropper".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.accent),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-color-eyedropper-active".to_string()),
            UiStylePatch {
                fill: Some([116, 67, 24, 72]),
                border: Some(tokens.accent_hot),
                border_width: Some(1.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
    ]
}
