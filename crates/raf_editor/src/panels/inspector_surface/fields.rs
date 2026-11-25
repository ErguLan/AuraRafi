//! Field controls for the game Inspector: selectors, ranges, toggles, and color picking.

use super::*;

pub(super) fn texture_dropdown(
    palette: StudioUiPalette,
    current: Option<&str>,
    project_assets: &[String],
    open: Option<InspectorDropdown>,
) -> UiNode {
    let supported = project_assets
        .iter()
        .filter(|asset| is_supported_texture_path(asset))
        .take(512)
        .cloned()
        .collect::<Vec<_>>();
    let selected = current.unwrap_or_default();
    let mut options = Vec::with_capacity(supported.len() + 1);
    options.push(UiSelectOption::new("", "app.no_texture"));
    options.extend(
        supported
            .into_iter()
            .map(|asset| UiSelectOption::new(asset.clone(), asset)),
    );
    let selected_index = options
        .iter()
        .position(|option| option.value == selected)
        .unwrap_or(0);
    dropdown_field(
        palette,
        "inspector.base-color-texture",
        "app.base_color_texture",
        InspectorDropdown::Texture,
        open,
        options,
        selected_index,
    )
}

fn is_supported_texture_path(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return false;
    }
    matches!(
        std::path::Path::new(&normalized)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "bmp" | "tga" | "webp")
    )
}
pub(super) fn primitive_options(current: Primitive) -> (Vec<UiSelectOption>, usize) {
    let primitives = [
        Primitive::Empty,
        Primitive::Cube,
        Primitive::Sphere,
        Primitive::Plane,
        Primitive::Cylinder,
        Primitive::Capsule,
        Primitive::Wedge,
        Primitive::Torus,
        Primitive::Prism,
        Primitive::Sphere2D,
    ];
    let selected_index = primitives
        .iter()
        .position(|primitive| *primitive == current)
        .unwrap_or(0);
    let options = primitives
        .into_iter()
        .map(|primitive| {
            UiSelectOption::new(
                primitive.label().to_ascii_lowercase(),
                primitive_label_key(primitive),
            )
        })
        .collect();
    (options, selected_index)
}
pub(crate) fn dropdown_field(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    dropdown: InspectorDropdown,
    open: Option<InspectorDropdown>,
    options: Vec<UiSelectOption>,
    selected_index: usize,
) -> UiNode {
    let is_open = open == Some(dropdown);
    let value_key = id.to_string();
    let trigger_id = format!("{id}.trigger");
    let popup_id = format!("{id}.menu");
    let select = UiSelect::new(value_key.clone(), options.clone(), selected_index)
        .with_popup_id(popup_id.clone());
    let mut select = select;
    select.open = is_open;
    let mut field = UiNode::new(format!("{id}.field"), UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 3.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    field = field
        .with_child(section_label(palette, label_key))
        .with_child(
            select_trigger(trigger_id.clone(), select, palette)
                .with_class("inspector-dropdown-trigger")
                .with_layout(UiLayout::fixed(0.0, 29.0).with_width_mode(UiSizeMode::Fill))
                .with_accessibility_label_key(label_key),
        );
    if is_open {
        let menu_height = (options.len() as f32 * 28.0 + 8.0).clamp(36.0, 196.0);
        let mut menu = UiNode::new(format!("{id}.menu"), UiNodeKind::Menu)
            .with_class("inspector-dropdown-menu")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_style(UiStyle {
                fill: palette.tokens().surface_raised,
                border: palette.tokens().border,
                text: palette.tokens().text,
                border_width: 1.0,
                radius: 4.0,
                opacity: 1.0,
            })
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 1.0,
                padding: UiSpacing::same(3.0),
                overflow: UiOverflow::ScrollY,
                ..UiLayout::absolute(UiRect::new(0.0, 29.0, 252.0, menu_height)).with_z_index(24)
            });
        for (index, option) in options.iter().enumerate() {
            let selected = index == selected_index;
            menu = menu.with_child(
                UiNode::new(format!("{id}.option.{}", option.value), UiNodeKind::Button)
                    .with_class(if selected {
                        "inspector-dropdown-option-selected"
                    } else {
                        "inspector-dropdown-option"
                    })
                    .with_layout(UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill))
                    .with_text_key(option.label_key.clone())
                    .with_text_style(UiTextStyle::body(if selected {
                        palette.tokens().text
                    } else {
                        palette.tokens().text_muted
                    }))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_accessibility_role(raf_ui::UiAccessibilityRole::Option)
                    .with_accessibility_selected(selected)
                    .focusable()
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: raf_ui::UiAction::SetSelect {
                            key: value_key.clone(),
                            value: option.value.clone(),
                            index,
                        },
                    })
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: raf_ui::UiAction::SetSelectOpen {
                            id: trigger_id.clone(),
                            open: false,
                        },
                    }),
            );
        }
        field = field.with_child(menu);
    }
    field
}
const COLOR_PICKER_POPOVER_PADDING: f32 = 10.0;
const COLOR_PICKER_POPOVER_GAP: f32 = 7.0;
const COLOR_PICKER_VISUAL_SIZE: f32 = 220.0;
const COLOR_PICKER_IMAGE_SIZE: f32 = UiColorPicker::CANVAS_SIZE;
const COLOR_PICKER_SQUARE_START: f32 = UiColorPicker::SQUARE_START;
const COLOR_PICKER_SQUARE_SIDE: f32 = UiColorPicker::SQUARE_SIDE;
const COLOR_PICKER_RING_RADIUS: f32 =
    (UiColorPicker::RING_INNER_RADIUS + UiColorPicker::RING_OUTER_RADIUS) * 0.5;
const COLOR_PICKER_POPOVER_WIDTH: f32 = 304.0;
const COLOR_PICKER_POPOVER_HEIGHT: f32 = 382.0;

fn color_hex(color: NodeColor) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.r, color.g, color.b, color.a
    )
}

pub(super) fn color_picker(palette: StudioUiPalette, color: NodeColor, open: bool) -> UiNode {
    let tokens = palette.tokens();
    let hex = color_hex(color);
    let mut picker = UiNode::new("inspector.color", UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    let mut trigger = UiNode::new("inspector.color.trigger", UiNodeKind::Button)
        .with_class("inspector-color-trigger")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 9.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.color.swatch", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(44.0, 22.0))
                .with_style(UiStyle {
                    fill: [color.r, color.g, color.b, color.a.max(32)],
                    border: tokens.border,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 3.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("inspector.color.hex", UiNodeKind::Label)
                .with_text_value(hex)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_child(
            UiNode::new("inspector.color.chevron", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if open {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(raf_render::api_graphic_basic::ui_surface::UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "inspector.color.toggle",
        ));
    if open {
        trigger = trigger.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "inspector.color.toggle",
        ));
    }
    picker = picker.with_child(trigger);
    if open {
        let mut popover = UiNode::new("inspector.color.popover", UiNodeKind::Menu)
            .with_class("inspector-color-popover")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_style(UiStyle {
                fill: tokens.surface_raised,
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 5.0,
                opacity: 1.0,
            })
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: COLOR_PICKER_POPOVER_GAP,
                padding: UiSpacing::same(COLOR_PICKER_POPOVER_PADDING),
                ..UiLayout::absolute(UiRect::new(
                    0.0,
                    36.0,
                    COLOR_PICKER_POPOVER_WIDTH,
                    COLOR_PICKER_POPOVER_HEIGHT,
                ))
                .with_z_index(24)
            });
        popover = popover.with_child(
            UiNode::text_input(
                "inspector.color.hex.input",
                UiTextInput {
                    value_key: "inspector.color.hex".to_string(),
                    placeholder_key: None,
                    max_length: 9,
                    multiline: false,
                    password: false,
                    submit_command: Some("inspector.color.hex.commit".to_string()),
                },
            )
            .with_class("inspector-color-hex-input")
            .with_layout(UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill))
            .with_text_style(UiTextStyle::body(tokens.text)),
        );
        let hsv = rgb_to_hsv(color);
        popover = popover.with_child(color_picker_visual(palette, hsv));
        for (channel, key, value) in [
            ("r", "app.color_r", color.r),
            ("g", "app.color_g", color.g),
            ("b", "app.color_b", color.b),
        ] {
            popover = popover.with_child(range_labeled(
                palette,
                key,
                &format!("inspector.color.{channel}"),
                value as f32,
                0.0,
                255.0,
                1.0,
            ));
        }
        picker = picker.with_child(popover);
    }
    picker
}

fn color_picker_visual(palette: StudioUiPalette, hsv: crate::color_math::HsvColor) -> UiNode {
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
    let marker_size = 14.0;
    let visual = UiNode::new("inspector.color.visual", UiNodeKind::Panel)
        .with_class("inspector-color-visual")
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
                "inspector.color.visual.image",
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
        )
        .with_child(
            UiNode::color_picker(
                "inspector.color.visual.surface",
                UiColorPicker::new("inspector.color", hsv.hue, hsv.saturation, hsv.value),
            )
            .with_class("inspector-color-surface")
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
            .with_tooltip_key("app.material")
            .with_accessibility_label_key("app.material"),
        );

    let marker_style = |border: [u8; 4], width: f32, radius: f32| UiStyle {
        fill: [0, 0, 0, 0],
        border,
        text: tokens.text,
        border_width: width,
        radius,
        opacity: 1.0,
    };
    visual
        .with_child(
            UiNode::new("inspector.color.visual.sv-marker.outer", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        saturation_marker_center[0] - marker_size * 0.5,
                        saturation_marker_center[1] - marker_size * 0.5,
                        marker_size,
                        marker_size,
                    ))
                    .with_z_index(5),
                )
                .with_style(marker_style([0, 0, 0, 230], 3.0, marker_size * 0.5)),
        )
        .with_child(
            UiNode::new("inspector.color.visual.sv-marker.inner", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        saturation_marker_center[0] - 5.0,
                        saturation_marker_center[1] - 5.0,
                        10.0,
                        10.0,
                    ))
                    .with_z_index(6),
                )
                .with_style(marker_style(tokens.text, 2.0, 5.0)),
        )
        .with_child(
            UiNode::new("inspector.color.visual.hue-marker", UiNodeKind::Panel)
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        hue_marker_center[0] - 7.0,
                        hue_marker_center[1] - 7.0,
                        14.0,
                        14.0,
                    ))
                    .with_z_index(6),
                )
                .with_style(marker_style(tokens.text, 2.0, 7.0)),
        )
}
pub(crate) fn section_label(palette: StudioUiPalette, key: &str) -> UiNode {
    UiNode::new(format!("inspector.label.{key}"), UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fit_content())
}

pub(super) fn range_labeled(
    palette: StudioUiPalette,
    label_key: &str,
    value_key: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) -> UiNode {
    UiNode::new(format!("{value_key}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, label_key))
        .with_child(
            UiNode::range(value_key, UiRange::new(value_key, value, min, max, step))
                .with_class("inspector-range")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [54.0, 26.0],
                    ..UiLayout::fixed(0.0, 26.0)
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::PointerUp(raf_ui::UiPointerButton::Primary),
                    "inspector.range.end",
                )),
        )
        .with_child(
            UiNode::new(format!("{value_key}.value"), UiNodeKind::Label)
                .with_class("inspector-range-value")
                .with_text_value(
                    if value_key.ends_with(".a") || value_key.ends_with(".opacity") {
                        format!("{:.0}%", value / max.max(1.0) * 100.0)
                    } else if value_key == "inspector.physics.damping" {
                        format!("{value:.2}")
                    } else {
                        format!("{value:.0}")
                    },
                )
                .with_text_overflow(UiTextOverflow::Clip)
                .with_text_style(UiTextStyle::button(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(42.0, 24.0).with_text_safe_area(true)),
        )
}

pub(super) fn toggle_row(
    palette: StudioUiPalette,
    label_key: &str,
    value_key: &str,
    value: bool,
) -> UiNode {
    UiNode::new(format!("{value_key}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, label_key).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }))
        .with_child(
            UiNode::toggle(value_key, UiToggle::new(value_key, value))
                .with_class("inspector-toggle")
                .with_layout(UiLayout::fixed(36.0, 24.0))
                .focusable()
                .with_event(UiEventBinding {
                    event: UiEventKind::Click,
                    action: raf_ui::UiAction::SetToggle {
                        key: value_key.to_string(),
                        value: !value,
                    },
                }),
        )
}
pub(super) fn text_field(
    id: &str,
    value_key: &str,
    value: &str,
    palette: StudioUiPalette,
    grow: f32,
) -> UiNode {
    let _ = value;
    UiNode::text_input(
        id,
        UiTextInput {
            value_key: value_key.to_string(),
            placeholder_key: Some("app.name".to_string()),
            max_length: 256,
            multiline: false,
            password: false,
            submit_command: None,
        },
    )
    .with_class("inspector-input")
    .with_layout(UiLayout {
        grow,
        min_size: [48.0, 28.0],
        ..UiLayout::fixed(0.0, 28.0)
    })
    .with_text_style(UiTextStyle::body(palette.tokens().text))
}
pub(super) fn primitive_label_key(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Empty => "app.primitive_empty",
        Primitive::Cube => "app.primitive_cube",
        Primitive::Sphere => "app.primitive_sphere",
        Primitive::Plane => "app.primitive_plane",
        Primitive::Cylinder => "app.primitive_cylinder",
        Primitive::Capsule => "app.primitive_capsule",
        Primitive::Wedge => "app.primitive_wedge",
        Primitive::Torus => "app.primitive_torus",
        Primitive::Prism => "app.primitive_prism",
        Primitive::Sphere2D => "app.primitive_sphere2d",
    }
}
