//! Lighting workbench presentation. World data, never synthetic hierarchy entities.
use raf_core::environment::{
    environment_presets, EnvironmentMode, EnvironmentPropertyKind as Kind, EnvironmentSettings,
    LightingTechnology, SkyMode, SunControl, ENVIRONMENT_PROPERTIES,
};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    color_field, color_field_rules, number_field, number_field_rules, UiAlign, UiColorFieldSpec,
    UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiLayout, UiNode, UiNodeKind,
    UiNumberFieldSpec, UiPointerButton, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiTextOverflow, UiTextStyle, UiToggle,
};

pub const SECTIONS: &[&str] = &[
    "overview",
    "technologies",
    "sun",
    "sky",
    "shadows",
    "time",
    "performance",
];
pub fn value_key(path: &str) -> String {
    format!("lighting.value.{path}")
}

/// Transient interaction state that does not belong in the policy model.
pub struct LightingUiState<'a> {
    pub open_color: Option<&'a str>,
    pub eyedropper: Option<&'a str>,
    pub recent_colors: &'a [[u8; 3]],
}

impl Default for LightingUiState<'_> {
    fn default() -> Self {
        Self {
            open_color: None,
            eyedropper: None,
            recent_colors: &[],
        }
    }
}
fn label(palette: StudioUiPalette, id: impl Into<String>, key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(palette.tokens().text))
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill))
}
fn button(
    palette: StudioUiPalette,
    id: impl Into<String>,
    key: &str,
    command: impl Into<String>,
    active: bool,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("lighting-action")
        .with_class(if active {
            "lighting-choice-active"
        } else {
            "lighting-choice"
        })
        .with_text_key(key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_style(UiStyle {
            fill: if active {
                palette.tokens().selection
            } else {
                [0; 4]
            },
            radius: 5.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_selected(active)
        .with_accessibility_label_key(key)
        .with_tooltip_key(key)
        .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn section_icon(section: &str) -> UiIconId {
    match section {
        "sun" | "overview" => UiIconId::Sun,
        "sky" => UiIconId::Sky,
        "shadows" => UiIconId::Shadows,
        "time" => UiIconId::Clock,
        _ => UiIconId::Settings,
    }
}

/// Live vertical gradient strip pair: zenith→horizon above the eye line and
/// horizon→ground below it. Source colors come from the authored environment,
/// so the Inspector always previews the exact sky the scene will render.
fn preset_block(palette: StudioUiPalette, env: &EnvironmentSettings) -> UiNode {
    let tokens = palette.tokens();
    let mut block = UiNode::new("lighting.presets", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("lighting.presets.title", UiNodeKind::Label)
                .with_text_key("lighting.presets")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)),
        );
    let presets = environment_presets();
    for (row_index, chunk) in presets.chunks(2).enumerate() {
        let mut row = UiNode::new(
            format!("lighting.presets.row.{row_index}"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        });
        for preset in chunk {
            let sky = preset.settings.sky;
            let mut button =
                UiNode::new(format!("lighting.preset.{}", preset.id), UiNodeKind::Button)
                    .with_class(if *env == preset.settings {
                        "lighting-preset lighting-preset-active"
                    } else {
                        "lighting-preset"
                    })
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_tooltip_key(preset.description_key.clone())
                    .with_accessibility_label_key(preset.name_key.clone())
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 5.0,
                        padding: UiSpacing::xy(5.0, 0.0),
                        grow: 1.0,
                        min_size: [0.0, 26.0],
                        width_mode: UiSizeMode::Fill,
                        ..UiLayout::fixed(0.0, 28.0)
                    })
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("lighting.preset:{}", preset.id),
                    ))
                    .with_child(
                        UiNode::new(
                            format!("lighting.preset.{}.preview", preset.id),
                            UiNodeKind::Panel,
                        )
                        .with_layout(UiLayout::fixed(22.0, 16.0))
                        .with_visual(raf_ui::UiVisual {
                            gradient: Some(raf_ui::UiLinearGradient {
                                start: [sky.top_color[0], sky.top_color[1], sky.top_color[2], 255],
                                end: [
                                    sky.horizon_color[0],
                                    sky.horizon_color[1],
                                    sky.horizon_color[2],
                                    255,
                                ],
                                direction: [0.0, 1.0],
                            }),
                            corners: Some([3.0; 4]),
                            ..raf_ui::UiVisual::default()
                        })
                        .with_style(UiStyle {
                            border: tokens.border,
                            text: tokens.text,
                            border_width: 1.0,
                            radius: 3.0,
                            ..UiStyle::transparent()
                        }),
                    );
            button.text_key = None;
            button = button.with_child(
                UiNode::new(
                    format!("lighting.preset.{}.name", preset.id),
                    UiNodeKind::Label,
                )
                .with_text_key(preset.name_key.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
            );
            row = row.with_child(button);
        }
        block = block.with_child(row);
    }
    block
}

fn sky_preview(
    palette: StudioUiPalette,
    env: &EnvironmentSettings,
    ui: &LightingUiState<'_>,
) -> UiNode {
    let sky = env.sky;
    let strip = |id: &str, start: [u8; 3], end: [u8; 3], height: f32, edit: &str| {
        let mut node = UiNode::new(id, UiNodeKind::Button)
            .with_class(if ui.open_color == Some(edit) {
                "lighting-sky-band lighting-sky-band-active"
            } else {
                "lighting-sky-band"
            })
            .with_layout(UiLayout::fixed(0.0, height).with_width_mode(UiSizeMode::Fill))
            .with_visual(raf_ui::UiVisual {
                gradient: Some(raf_ui::UiLinearGradient {
                    start: [start[0], start[1], start[2], 255],
                    end: [end[0], end[1], end[2], 255],
                    direction: [0.0, 1.0],
                }),
                corners: Some([2.0; 4]),
                ..raf_ui::UiVisual::default()
            })
            .focusable()
            .with_tooltip_key("lighting.sky_edit_color")
            .with_accessibility_label_key("lighting.sky_edit_color")
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("lighting.sky.edit:{edit}"),
            ));
        node = node.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("Enter".to_string()),
            format!("lighting.sky.edit:{edit}"),
        ));
        node
    };
    let mut stack = UiNode::new("lighting.skyview.stack", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(strip(
            "lighting.skyview.sky",
            sky.top_color,
            sky.horizon_color,
            56.0,
            "sky.top_color",
        ));
    if sky.mode == SkyMode::Gradient {
        stack = stack
            .with_child(strip(
                "lighting.skyview.horizon",
                sky.horizon_color,
                sky.horizon_color,
                8.0,
                "sky.horizon_color",
            ))
            .with_child(strip(
                "lighting.skyview.ground",
                sky.horizon_color,
                sky.ground_color,
                24.0,
                "sky.ground_color",
            ));
    }
    let stars = if sky.stars_enabled {
        format!("{:.2}", sky.stars_density)
    } else {
        "-".to_string()
    };
    let mut swatches =
        UiNode::new("lighting.skyview.edit-row", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    for (id, label_key, path, color) in [
        (
            "lighting.skyview.edit.top",
            "lighting.top_color",
            "sky.top_color",
            sky.top_color,
        ),
        (
            "lighting.skyview.edit.horizon",
            "lighting.horizon_color",
            "sky.horizon_color",
            sky.horizon_color,
        ),
        (
            "lighting.skyview.edit.ground",
            "lighting.ground_color",
            "sky.ground_color",
            sky.ground_color,
        ),
    ] {
        if path != "sky.top_color" && sky.mode != SkyMode::Gradient {
            continue;
        }
        swatches = swatches.with_child(
            UiNode::new(id, UiNodeKind::Button)
                .with_class(if ui.open_color == Some(path) {
                    "lighting-sky-chip lighting-sky-chip-active"
                } else {
                    "lighting-sky-chip"
                })
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    padding: UiSpacing::xy(5.0, 0.0),
                    ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::FitContent)
                })
                .focusable()
                .with_tooltip_key("lighting.sky_edit_color")
                .with_accessibility_label_key(label_key)
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("lighting.sky.edit:{path}"),
                ))
                .with_child(
                    UiNode::new(format!("{id}.swatch"), UiNodeKind::Panel)
                        .with_layout(UiLayout::fixed(16.0, 16.0))
                        .with_style(UiStyle {
                            fill: [color[0], color[1], color[2], 255],
                            border: palette.tokens().border,
                            text: palette.tokens().text,
                            border_width: 1.0,
                            radius: 3.0,
                            opacity: 1.0,
                        }),
                )
                .with_child(
                    UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                        .with_text_key(label_key)
                        .with_text_style(UiTextStyle {
                            size_px: 10.0,
                            line_height_px: 13.0,
                            ..UiTextStyle::body(palette.tokens().text_muted)
                        })
                        .with_layout(UiLayout::fit_content()),
                ),
        );
    }
    UiNode::new("lighting.skyview", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(readout_row(
            palette,
            "lighting.skyview.header",
            "lighting.sky_preview",
            ReadoutValue::Text(stars),
        ))
        .with_child(stack)
        .with_child(swatches)
}

/// Direct sun placement on a top-down hemisphere. Radius maps elevation
/// (zenith at the center, horizon at the rim); angle maps azimuth clockwise
/// from north. Pointer handlers live in the workbench so the widget remains a
/// pure retained document.
fn sun_dial(palette: StudioUiPalette, env: &EnvironmentSettings) -> UiNode {
    let tokens = palette.tokens();
    let size = 184.0;
    let center = size * 0.5;
    let radius = size * 0.5 - 12.0;
    let grid = [tokens.border[0], tokens.border[1], tokens.border[2], 70];
    let elevation = env.sun.elevation.clamp(-1.5708, 1.5708);
    let disk_radius = (1.0 - elevation / 1.5708).clamp(0.0, 1.0);
    let azimuth = env.sun.azimuth;
    let marker = [
        center + azimuth.sin() * disk_radius * radius,
        center - azimuth.cos() * disk_radius * radius,
    ];
    let line = |id: &str, x: f32, y: f32, width: f32, height: f32| {
        UiNode::new(id, UiNodeKind::Panel)
            .with_layout(UiLayout::absolute(raf_ui::UiRect::new(x, y, width, height)))
            .with_style(UiStyle {
                fill: grid,
                ..UiStyle::transparent()
            })
    };
    let cardinal = |id: &str, key: &str, x: f32, y: f32| {
        UiNode::new(id, UiNodeKind::Label)
            .with_text_key(key)
            .with_text_style(UiTextStyle {
                size_px: 10.0,
                line_height_px: 12.0,
                ..UiTextStyle::body(tokens.text_muted)
            })
            .with_layout(UiLayout::absolute(raf_ui::UiRect::new(x, y, 20.0, 12.0)))
    };
    let mut dial = UiNode::new("lighting.sun.dial", UiNodeKind::Button)
        .with_class("lighting-sun-dial")
        .with_layout(UiLayout::fixed(size, size).with_width_mode(UiSizeMode::Fixed))
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: size * 0.5,
            opacity: 1.0,
        })
        .focusable()
        .with_tooltip_key("lighting.sun_drag_hint")
        .with_accessibility_label_key("lighting.sun_position")
        .with_event(UiEventBinding::command(
            UiEventKind::PointerDown(UiPointerButton::Primary),
            "lighting.sun.dial.press",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            "lighting.sun.dial.drag",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::PointerUp(UiPointerButton::Primary),
            "lighting.sun.dial.release",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            "lighting.sun.dial.release",
        ))
        .with_child(line("lighting.sun.dial.h", 12.0, center, size - 24.0, 1.0))
        .with_child(line("lighting.sun.dial.v", center, 12.0, 1.0, size - 24.0))
        .with_child(cardinal(
            "lighting.sun.dial.n",
            "lighting.cardinal_n",
            center - 10.0,
            2.0,
        ))
        .with_child(cardinal(
            "lighting.sun.dial.s",
            "lighting.cardinal_s",
            center - 10.0,
            size - 14.0,
        ))
        .with_child(cardinal(
            "lighting.sun.dial.e",
            "lighting.cardinal_e",
            size - 22.0,
            center - 6.0,
        ))
        .with_child(cardinal(
            "lighting.sun.dial.w",
            "lighting.cardinal_w",
            2.0,
            center - 6.0,
        ))
        .with_child(
            UiNode::new("lighting.sun.dial.marker", UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(raf_ui::UiRect::new(
                    marker[0] - 9.0,
                    marker[1] - 9.0,
                    18.0,
                    18.0,
                )))
                .with_style(UiStyle {
                    fill: [env.sun.color[0], env.sun.color[1], env.sun.color[2], 255],
                    border: tokens.text,
                    text: [255; 4],
                    border_width: 2.0,
                    radius: 9.0,
                    opacity: 1.0,
                }),
        );
    dial = dial.with_child(
        UiNode::new("lighting.sun.dial.elevation-ring", UiNodeKind::Panel)
            .with_layout(UiLayout::absolute(raf_ui::UiRect::new(
                center - radius * 0.5,
                center - radius * 0.5,
                radius,
                radius,
            )))
            .with_style(UiStyle {
                fill: [0, 0, 0, 0],
                border: grid,
                text: tokens.text,
                border_width: 1.0,
                radius: radius * 0.5,
                opacity: 1.0,
            }),
    );
    UiNode::new("lighting.sun.dial.frame", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(0.0, 4.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(readout_row(
            palette,
            "lighting.sun.dial.readout",
            "lighting.sun_position",
            ReadoutValue::Text(format!(
                "{} / {}",
                degrees(env.sun.azimuth.to_degrees()),
                format!("{:.0}°", env.sun.elevation.to_degrees())
            )),
        ))
        .with_child(dial)
}

fn readout_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: &str,
    value: ReadoutValue,
) -> UiNode {
    let tokens = palette.tokens();
    let id = id.into();
    let value_child_style = UiStyle {
        fill: tokens.surface_alt,
        radius: 4.0,
        ..UiStyle::transparent()
    };
    let value_text_style = UiTextStyle {
        size_px: 10.0,
        line_height_px: 13.0,
        ..UiTextStyle::body(tokens.text)
    };
    let value_label = match value {
        ReadoutValue::Text(text) => UiNode::new(format!("{id}.value"), UiNodeKind::Label)
            .with_text_value(text)
            .with_text_overflow(UiTextOverflow::Clip),
        ReadoutValue::Choice(choice) => UiNode::new(format!("{id}.value"), UiNodeKind::Label)
            .with_text_key(choice)
            .with_text_overflow(UiTextOverflow::Clip),
    };
    UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle {
                    size_px: 10.0,
                    line_height_px: 13.0,
                    ..UiTextStyle::body(tokens.text_muted)
                })
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_child(
            value_label
                .with_style(value_child_style)
                .with_text_style(value_text_style)
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(6.0, 0.0),
                    min_size: [72.0, 20.0],
                    ..UiLayout::fit_content()
                }),
        )
}

/// Summary values are either literal runtime numbers or localized enums.
enum ReadoutValue {
    Text(String),
    Choice(&'static str),
}

fn degrees(value: f32) -> String {
    format!("{:.0}°", value.rem_euclid(360.0))
}

/// The angles the world clock is actually driving right now. Clock mode
/// disables the manual angle inputs, so without this readout the panel gives
/// no clue where the Sun is.
fn clock_position_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    env: &EnvironmentSettings,
) -> UiNode {
    let direction = env.sun_direction();
    let elevation = direction.y.clamp(-1.0, 1.0).asin().to_degrees();
    let azimuth = direction.x.atan2(direction.z).to_degrees();
    readout_row(
        palette,
        id,
        "lighting.sun_readout",
        ReadoutValue::Text(format!("{} / {}", degrees(elevation), degrees(azimuth))),
    )
}
fn performance_summary(palette: StudioUiPalette, env: &EnvironmentSettings) -> UiNode {
    let mut rows = UiNode::new("lighting.summary", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("lighting.summary.technology", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("lighting.summary.technology.label", UiNodeKind::Label)
                        .with_text_key("lighting.technology")
                        .with_text_style(UiTextStyle {
                            size_px: 10.0,
                            line_height_px: 13.0,
                            ..UiTextStyle::body(palette.tokens().text_muted)
                        })
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                        }),
                )
                .with_child(
                    UiNode::new("lighting.summary.technology.value", UiNodeKind::Label)
                        .with_text_key(match env.technology {
                            LightingTechnology::OptimizedCurrent => "lighting.summary_current",
                            LightingTechnology::OptimizedImproved => "lighting.summary_improved",
                        })
                        .with_text_overflow(UiTextOverflow::Clip)
                        .with_style(UiStyle {
                            fill: palette.tokens().surface_alt,
                            radius: 4.0,
                            ..UiStyle::transparent()
                        })
                        .with_text_style(UiTextStyle {
                            size_px: 10.0,
                            line_height_px: 13.0,
                            ..UiTextStyle::body(palette.tokens().text)
                        }),
                ),
        );
    let push_row = |rows: UiNode, id: &str, key: &'static str, value: ReadoutValue| -> UiNode {
        rows.with_child(readout_row(
            palette,
            format!("lighting.summary.{id}"),
            key,
            value,
        ))
    };
    rows = push_row(
        rows,
        "exposure",
        "lighting.exposure",
        ReadoutValue::Text(format!("{:.2}", env.exposure)),
    );
    rows = push_row(
        rows,
        "toon",
        "lighting.toon_enabled",
        ReadoutValue::Text(if env.sun.toon_enabled {
            format!("{:.0}", env.sun.toon_steps.ceil())
        } else {
            "-".to_string()
        }),
    );
    rows = push_row(
        rows,
        "mode",
        "lighting.mode",
        ReadoutValue::Choice(match env.mode {
            EnvironmentMode::Neutral => "lighting.summary_legacy",
            EnvironmentMode::Scene => "lighting.summary_scene",
        }),
    );
    rows = push_row(
        rows,
        "shadows",
        "lighting.shadow_enabled",
        ReadoutValue::Text(if env.shadows.enabled {
            format!("{:.0}", env.shadows.resolution)
        } else {
            "-".to_string()
        }),
    );
    rows = push_row(
        rows,
        "filter",
        "lighting.shadow_filter",
        ReadoutValue::Text(match (env.shadows.enabled, env.shadows.soft_filter) {
            (true, true) => "4".to_string(),
            (true, false) => "1".to_string(),
            (false, _) => "-".to_string(),
        }),
    );
    rows = push_row(
        rows,
        "sky",
        "lighting.sky_mode",
        ReadoutValue::Choice(match env.sky.mode {
            SkyMode::Solid => "lighting.summary_solid",
            SkyMode::Gradient => "lighting.summary_gradient",
        }),
    );
    rows = push_row(
        rows,
        "stars",
        "lighting.stars_enabled",
        ReadoutValue::Text(if env.sky.stars_enabled {
            format!("{:.2}", env.sky.stars_density)
        } else {
            "-".to_string()
        }),
    );
    rows
}

fn instrument_styles(palette: StudioUiPalette) -> raf_ui::UiStyleSheet {
    let mut sheet = super::hierarchy_surface::hierarchy_style_sheet(palette);
    for (state, patch) in [
        (
            UiStyleRuleState::Hovered,
            UiStylePatch {
                fill: Some(palette.tokens().surface_raised),
                ..Default::default()
            },
        ),
        (
            UiStyleRuleState::Focused,
            UiStylePatch {
                border: Some(palette.tokens().focus),
                border_width: Some(1.0),
                ..Default::default()
            },
        ),
        (
            UiStyleRuleState::Active,
            UiStylePatch {
                fill: Some(palette.tokens().selection),
                ..Default::default()
            },
        ),
    ] {
        sheet.rules.push(
            UiStyleRule::new(UiStyleSelector::Class("lighting-action".into()), patch).when(state),
        );
    }
    sheet.rules.extend(number_field_rules(palette));
    sheet.rules.extend(color_field_rules(palette));
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-choice".into()),
            UiStylePatch {
                fill: Some(palette.tokens().surface_alt),
                border: Some(palette.tokens().border),
                border_width: Some(1.0),
                radius: Some(4.0),
                text: Some(palette.tokens().text),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-choice".into()),
            UiStylePatch {
                fill: Some(palette.tokens().surface_raised),
                border: Some(palette.tokens().accent),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-choice-active".into()),
            UiStylePatch {
                fill: Some(palette.tokens().selection),
                border: Some(palette.tokens().accent),
                border_width: Some(1.0),
                text: Some(palette.tokens().text),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-preset".into()),
            UiStylePatch {
                fill: Some(palette.tokens().surface_alt),
                border: Some(palette.tokens().border),
                border_width: Some(1.0),
                radius: Some(4.0),
                text: Some(palette.tokens().text),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-preset".into()),
            UiStylePatch {
                fill: Some(palette.tokens().surface_raised),
                border: Some(palette.tokens().accent),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-preset-active".into()),
            UiStylePatch {
                fill: Some([116, 67, 24, 64]),
                border: Some(palette.tokens().accent),
                border_width: Some(1.0),
                text: Some(palette.tokens().text),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sky-band".into()),
            UiStylePatch {
                border: Some([0, 0, 0, 0]),
                border_width: Some(1.0),
                radius: Some(2.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sky-band".into()),
            UiStylePatch {
                border: Some(palette.tokens().accent),
                border_width: Some(1.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sky-band-active".into()),
            UiStylePatch {
                border: Some(palette.tokens().accent_hot),
                border_width: Some(1.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sky-chip".into()),
            UiStylePatch {
                fill: Some(palette.tokens().surface_alt),
                border: Some(palette.tokens().border),
                border_width: Some(1.0),
                radius: Some(11.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sky-chip".into()),
            UiStylePatch {
                fill: Some(palette.tokens().surface_raised),
                border: Some(palette.tokens().accent),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sky-chip-active".into()),
            UiStylePatch {
                border: Some(palette.tokens().accent_hot),
                border_width: Some(1.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-sun-dial".into()),
            UiStylePatch {
                border: Some(palette.tokens().border),
                border_width: Some(1.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );
    sheet.rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("lighting-eyedropper-hint".into()),
            UiStylePatch {
                fill: Some([116, 67, 24, 54]),
                border: Some(palette.tokens().accent),
                border_width: Some(1.0),
                radius: Some(4.0),
                ..Default::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );
    sheet
}
pub fn navigation(palette: StudioUiPalette, section: &str) -> UiSurface {
    let mut root = UiNode::new("lighting.navigation", UiNodeKind::Root)
        .with_style(palette.panel_style())
        .with_layout(UiLayout {
            padding: UiSpacing::same(6.0),
            gap: 6.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(super::hierarchy_surface::header(palette))
        .with_child(super::hierarchy_surface::tabs(palette, true, "lighting"))
        .with_child(label(palette, "lighting.world-note", "lighting.world_note"));
    let mut sections = UiNode::scroll_view("lighting.navigation.sections", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            grow: 1.0,
            gap: 6.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    for name in SECTIONS {
        let selected = *name == section;
        let mut row = button(
            palette,
            format!("lighting.section.{name}"),
            &format!("lighting.section_{name}"),
            format!("lighting.section:{name}"),
            selected,
        );
        row.text_key = None;
        row = row
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 10.0,
                align_items: UiAlign::Center,
                padding: UiSpacing::xy(6.0, 4.0),
                ..UiLayout::fixed(0.0, 50.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(format!("lighting.edge.{name}"), UiNodeKind::Panel)
                    .with_layout(UiLayout::fixed(2.0, 28.0))
                    .with_style(UiStyle {
                        fill: if selected {
                            palette.tokens().accent
                        } else {
                            [0; 4]
                        },
                        ..UiStyle::transparent()
                    }),
            )
            .with_child(
                UiNode::new(format!("lighting.icon.{name}"), UiNodeKind::Label)
                    .with_icon(UiIcon::new(section_icon(name)).with_tint(if selected {
                        palette.tokens().accent
                    } else {
                        palette.tokens().text_muted
                    }))
                    .with_layout(UiLayout::fixed(22.0, 22.0)),
            )
            .with_child(
                UiNode::new(format!("lighting.titles.{name}"), UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        grow: 1.0,
                        gap: 1.0,
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        label(
                            palette,
                            format!("lighting.name.{name}"),
                            &format!("lighting.section_{name}"),
                        )
                        .with_text_style(UiTextStyle::panel_title(palette.tokens().text)),
                    )
                    .with_child(
                        label(
                            palette,
                            format!("lighting.description.{name}"),
                            &format!("lighting.description_{name}"),
                        )
                        .with_text_style(UiTextStyle {
                            size_px: 10.0,
                            line_height_px: 13.0,
                            ..UiTextStyle::body(palette.tokens().text_muted)
                        }),
                    ),
            );
        sections = sections.with_child(row);
    }
    sections = sections.with_child(label(
        palette,
        "lighting.navigation.hint",
        "lighting.navigation_hint",
    ));
    root = root.with_child(sections);
    let mut surface = UiSurface::new("editor.lighting.navigation", palette, root);
    surface.style_sheet = instrument_styles(palette);
    surface
}
pub fn inspector(
    palette: StudioUiPalette,
    env: &EnvironmentSettings,
    section: &str,
    neutral_preview: bool,
    error: bool,
) -> UiSurface {
    inspector_with_state(
        palette,
        env,
        section,
        neutral_preview,
        error,
        &LightingUiState::default(),
    )
}

pub fn inspector_with_state(
    palette: StudioUiPalette,
    env: &EnvironmentSettings,
    section: &str,
    neutral_preview: bool,
    error: bool,
    ui: &LightingUiState<'_>,
) -> UiSurface {
    let mut root = UiNode::new("lighting.inspector.content", UiNodeKind::Panel)
        .with_style(palette.panel_style())
        .with_layout(UiLayout {
            padding: UiSpacing {
                right: 24.0,
                ..UiSpacing::same(12.0)
            },
            gap: 10.0,
            ..UiLayout::column()
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        })
        .with_child(
            label(
                palette,
                "lighting.title",
                &format!("lighting.section_{section}"),
            )
            .with_icon(UiIcon::new(section_icon(section)).with_tint(palette.tokens().accent))
            .with_text_style(UiTextStyle::panel_title(palette.tokens().text))
            .with_layout(UiLayout {
                padding: UiSpacing::xy(0.0, 6.0),
                ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
            }),
        )
        .with_child(label(palette, "lighting.scope", "lighting.world_note"))
        .with_child(
            UiNode::toggle(
                "lighting.preview",
                UiToggle::new("lighting.preview_neutral", neutral_preview),
            )
            .with_text_key("lighting.preview_neutral")
            .with_text_style(UiTextStyle::body(palette.tokens().text))
            .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)),
        );
    if error {
        root = root.with_child(
            label(palette, "lighting.error", "lighting.invalid_value")
                .with_text_style(UiTextStyle::body(palette.tokens().danger)),
        );
    }
    if section == "technologies" {
        root = root.with_child(label(
            palette,
            "lighting.technology-note",
            "lighting.description_technology",
        ));
    }
    if section == "shadows" || section == "performance" {
        root = root.with_child(label(
            palette,
            "lighting.status",
            if section == "shadows" {
                "lighting.shadows_note"
            } else {
                "lighting.performance_note"
            },
        ));
    }
    if section == "sky" {
        root = root.with_child(sky_preview(palette, env, ui));
    }
    if section == "overview" {
        root = root.with_child(preset_block(palette, env));
    }
    if section == "sun" && env.sun.control == SunControl::Manual {
        root = root.with_child(sun_dial(palette, env));
    }
    if ui.eyedropper.is_some() {
        let mut hint = UiNode::new("lighting.eyedropper.hint", UiNodeKind::Toolbar)
            .with_class("lighting-eyedropper-hint")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(8.0, 4.0),
                ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("lighting.eyedropper.hint.icon", UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(UiIconId::Eye)
                            .with_size(raf_ui::UiIconSize::Small)
                            .with_tint(palette.tokens().accent_hot),
                    )
                    .with_layout(UiLayout::fixed(18.0, 18.0)),
            )
            .with_child(
                UiNode::new("lighting.eyedropper.hint.label", UiNodeKind::Label)
                    .with_text_key("lighting.eyedropper_pick")
                    .with_text_style(UiTextStyle::body(palette.tokens().text))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    }),
            );
        hint = hint.with_child(
            UiNode::new("lighting.eyedropper.cancel", UiNodeKind::Button)
                .with_class("lighting-action")
                .with_text_key("app.cancel")
                .with_text_style(UiTextStyle::button(palette.tokens().text))
                .with_layout(UiLayout::fixed(64.0, 22.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "lighting.eyedropper.cancel",
                )),
        );
        root = root.with_child(hint);
    }
    if section == "sun" && env.sun.control == SunControl::Clock {
        root = root.with_child(clock_position_row(palette, "lighting.live.sun", env));
    }
    if section == "time" {
        let hours = env.clock.time_seconds / 3600.0;
        let clock_time = format!(
            "{:02}:{:02}",
            hours as u32 as i64 % 24,
            (hours.fract() * 60.0).round() as u32 % 60
        );
        root = root
            .with_child(readout_row(
                palette,
                "lighting.live.clock",
                "lighting.clock_readout",
                ReadoutValue::Text(clock_time),
            ))
            .with_child(clock_position_row(palette, "lighting.live.time.sun", env));
    }
    if section == "performance" {
        root = root.with_child(performance_summary(palette, env));
    }
    for property in ENVIRONMENT_PROPERTIES.iter().filter(|p| match section {
        "sun" => p.path.starts_with("sun."),
        "sky" => {
            p.path.starts_with("sky.")
                && !(env.sky.mode == SkyMode::Solid
                    && matches!(p.path, "sky.horizon_color" | "sky.ground_color"))
        }
        "shadows" => p.path.starts_with("shadows."),
        "time" => p.path.starts_with("clock.") || p.path == "sun.control",
        "technologies" => p.path == "technology",
        "overview" => matches!(
            p.path,
            "mode" | "ambient_color" | "ambient_intensity" | "exposure"
        ),
        _ => false,
    }) {
        let path = property.path;
        let value = env.get_property(path).expect("Core property catalog");
        let needs_improved = env.technology
            == raf_core::environment::LightingTechnology::OptimizedCurrent
            && matches!(path, "exposure" | "sun.toon_softness");
        let disabled = needs_improved
            || env.sun.control == SunControl::Clock
                && matches!(path, "sun.azimuth" | "sun.elevation");
        let modified = property_is_modified(env, path);
        let mut field = UiNode::new(format!("lighting.field.{path}"), UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 3.0,
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            });
        match property.kind {
            Kind::Bool => {
                field = field.with_child(field_header(
                    palette,
                    path,
                    property.label,
                    modified,
                    disabled,
                ));
                field = field.with_child(
                    UiNode::toggle(
                        format!("lighting.control.{path}"),
                        UiToggle::new(value_key(path), value.as_bool().unwrap()),
                    )
                    .with_accessibility_label_key(property.label)
                    .with_layout(UiLayout::fixed(32.0, 26.0)),
                );
            }
            Kind::Choice(choices) => {
                field = field.with_child(field_header(
                    palette,
                    path,
                    property.label,
                    modified,
                    disabled,
                ));
                let mut group = UiNode::new(format!("lighting.choices.{path}"), UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 4.0,
                        ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
                    });
                for choice in choices {
                    group = group.with_child(
                        button(
                            palette,
                            format!("lighting.choice.{path}.{choice}"),
                            &format!("lighting.choice_{choice}"),
                            format!("lighting.choose:{path}:{choice}"),
                            value.as_str() == Some(choice),
                        )
                        .with_layout(UiLayout {
                            grow: 1.0,
                            min_size: [48.0, 28.0],
                            width_mode: UiSizeMode::Fill,
                            ..UiLayout::fixed(0.0, 28.0)
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .disabled(disabled),
                    );
                }
                field = field.with_child(group);
            }
            Kind::Number { .. } if path == "shadows.resolution" => {
                field = field.with_child(field_header(
                    palette,
                    path,
                    property.label,
                    modified,
                    disabled,
                ));
                let mut group = UiNode::new(format!("lighting.choices.{path}"), UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 4.0,
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    });
                for resolution in [256, 512, 1024] {
                    group = group.with_child(
                        button(
                            palette,
                            format!("lighting.resolution.{resolution}"),
                            &format!("lighting.choice_{resolution}"),
                            format!("lighting.choose:{path}:{resolution}"),
                            value.as_f64() == Some(resolution as f64),
                        )
                        .with_text_value(resolution.to_string())
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout {
                            grow: 1.0,
                            min_size: [40.0, 28.0],
                            width_mode: UiSizeMode::Fill,
                            ..UiLayout::fixed(0.0, 28.0)
                        })
                        .disabled(disabled),
                    );
                }
                field = field.with_child(group);
            }
            Kind::Number {
                min,
                max,
                display_scale,
                step,
            } => {
                let stored_step = (step / display_scale.max(f32::EPSILON)).max(1e-6);
                let spec = UiNumberFieldSpec::new(
                    format!("lighting.field.{path}"),
                    property.label,
                    path,
                    value_key(path),
                    value.as_f64().unwrap_or(min as f64) as f32,
                    min,
                    max,
                    stored_step,
                )
                .with_modified(modified)
                .with_disabled(disabled)
                .with_hint_key(unit_hint_key(path))
                .with_copy_text(display_value(env, path));
                field = field.with_child(number_field(palette, &spec));
            }
            Kind::Color => {
                let color = value_rgb(&value);
                let spec = UiColorFieldSpec::new(
                    format!("lighting.field.{path}"),
                    property.label,
                    path,
                    color,
                )
                .with_open(ui.open_color == Some(path))
                .with_modified(modified)
                .with_recent(ui.recent_colors.to_vec())
                .with_eyedropper_active(ui.eyedropper == Some(path));
                field = field.with_child(color_field(palette, &spec));
            }
        }
        if disabled {
            field = field.with_child(label(
                palette,
                format!("lighting.driver.{path}"),
                if needs_improved {
                    "lighting.requires_improved"
                } else {
                    "lighting.clock_controls_angles"
                },
            ));
        }
        field = field.with_child(
            UiNode::new(format!("lighting.divider.{path}"), UiNodeKind::Panel)
                .with_style(UiStyle {
                    fill: palette.tokens().border,
                    opacity: 0.35,
                    ..UiStyle::transparent()
                })
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
        );
        root = root.with_child(field);
    }

    let mut surface = UiSurface::new(
        "editor.lighting.inspector",
        palette,
        UiNode::scroll_view("lighting.inspector", UiScrollAxis::Vertical)
            .with_style(palette.panel_style())
            .with_layout(UiLayout::fill(UiFlow::Column))
            .with_child(root),
    );
    surface.style_sheet = instrument_styles(palette);
    surface
}
fn field_header(
    palette: StudioUiPalette,
    path: &str,
    label_key: &str,
    modified: bool,
    disabled: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let mut header = UiNode::new(format!("lighting.field-header.{path}"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            label(palette, format!("lighting.label.{path}"), label_key)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                }),
        );
    if modified {
        header = header.with_child(
            UiNode::new(format!("lighting.reset.{path}"), UiNodeKind::Button)
                .with_class("ui-field-icon")
                .with_layout(UiLayout::fixed(18.0, 18.0))
                .with_icon(
                    UiIcon::new(UiIconId::Refresh)
                        .with_size(raf_ui::UiIconSize::Custom(11))
                        .with_tint(tokens.text_muted),
                )
                .with_tooltip_key("lighting.reset")
                .with_accessibility_label_key("lighting.reset")
                .focusable()
                .disabled(disabled)
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("lighting.reset:{path}"),
                )),
        );
    }
    header
}

fn property_is_modified(env: &EnvironmentSettings, path: &str) -> bool {
    let Ok(current) = env.get_property(path) else {
        return false;
    };
    let defaults = EnvironmentSettings::default();
    let Ok(default) = defaults.get_property(path) else {
        return false;
    };
    current != default
}

fn value_rgb(value: &serde_json::Value) -> [u8; 3] {
    let array = value.as_array().expect("Color property is an array");
    [
        array[0].as_u64().unwrap_or(255) as u8,
        array[1].as_u64().unwrap_or(255) as u8,
        array[2].as_u64().unwrap_or(255) as u8,
    ]
}

fn unit_hint_key(path: &str) -> &'static str {
    if path.contains("azimuth")
        || path.contains("elevation")
        || path.contains("radius")
        || path.contains("offset")
    {
        "lighting.unit_degrees"
    } else if path == "clock.time_seconds" {
        "lighting.unit_hours"
    } else if path == "clock.day_duration" {
        "lighting.unit_seconds"
    } else if path == "sun.toon_steps" {
        "lighting.unit_steps"
    } else if matches!(path, "shadows.distance" | "shadows.bias") {
        "lighting.unit_world"
    } else {
        "lighting.unit_factor"
    }
}

fn step_decimals(step: f32) -> usize {
    if !step.is_finite() || step <= 0.0 {
        return 2;
    }
    (-step.log10()).ceil().clamp(0.0, 4.0) as usize
}

pub fn color_popover_surface(
    palette: StudioUiPalette,
    env: &EnvironmentSettings,
    path: &str,
    recent: &[[u8; 3]],
    eyedropper: Option<&str>,
) -> Option<UiSurface> {
    let value = env.get_property(path).ok()?;
    let color = value_rgb(&value);
    let label_key = raf_core::environment::environment_property(path)
        .ok()?
        .label;
    let spec = UiColorFieldSpec::new(format!("lighting.field.{path}"), label_key, path, color)
        .with_open(true)
        .with_recent(recent.to_vec())
        .with_eyedropper_active(eyedropper == Some(path));
    let mut surface = UiSurface::new(
        "editor.lighting.color-popover",
        palette,
        raf_ui::color_popover(palette, &spec),
    );
    surface.style_sheet = instrument_styles(palette);
    Some(surface)
}

pub fn display_value(env: &EnvironmentSettings, path: &str) -> String {
    let value = env.get_property(path).expect("Property catalog");
    match raf_core::environment::environment_property(path)
        .unwrap()
        .kind
    {
        Kind::Color => {
            let v = value.as_array().unwrap();
            format!(
                "#{:02X}{:02X}{:02X}",
                v[0].as_u64().unwrap(),
                v[1].as_u64().unwrap(),
                v[2].as_u64().unwrap()
            )
        }
        Kind::Number {
            display_scale,
            step,
            ..
        } => {
            let decimals = step_decimals(step);
            let mut text = format!(
                "{:.*}",
                decimals,
                value.as_f64().unwrap() * display_scale as f64
            );
            if text.contains('.') {
                while text.ends_with('0') {
                    text.pop();
                }
                if text.ends_with('.') {
                    text.pop();
                }
            }
            text
        }
        _ => value.to_string(),
    }
}

pub fn parse_value(path: &str, text: &str) -> Result<serde_json::Value, String> {
    parse_value_with_mode(path, text, false)
}

/// Live edits clamp to the authored range instead of rejecting a number the
/// user is still tuning. Enter/commit keeps strict validation.
pub fn parse_value_clamped(path: &str, text: &str) -> Result<serde_json::Value, String> {
    parse_value_with_mode(path, text, true)
}

fn parse_value_with_mode(path: &str, text: &str, clamp: bool) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let property = raf_core::environment::environment_property(path)?;
    match property.kind {
        Kind::Color => {
            let s = text.trim().trim_start_matches('#');
            if (s.len() != 6 && s.len() != 8) || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("Expected #RRGGBB or #RRGGBBAA".into());
            }
            Ok(json!([
                u8::from_str_radix(&s[0..2], 16).unwrap(),
                u8::from_str_radix(&s[2..4], 16).unwrap(),
                u8::from_str_radix(&s[4..6], 16).unwrap()
            ]))
        }
        Kind::Number {
            min,
            max,
            display_scale,
            ..
        } => {
            let cleaned = text
                .trim()
                .trim_end_matches(|c: char| c.is_alphabetic() || c == '°' || c == '%')
                .trim()
                .replace(',', ".");
            let n = cleaned.parse::<f64>().map_err(|_| "Expected number")? / display_scale as f64;
            if !n.is_finite() {
                return Err("Expected finite number".into());
            }
            if clamp {
                Ok(json!((n as f32).clamp(min, max)))
            } else if n < min as f64 - 1e-5 || n > max as f64 + 1e-5 {
                Err("Number outside range".into())
            } else {
                Ok(json!((n as f32).clamp(min, max)))
            }
        }
        _ => Err("Use the choice or checkbox control".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn collect<'a>(node: &'a UiNode, nodes: &mut Vec<&'a UiNode>) {
        nodes.push(node);
        for child in &node.children {
            collect(child, nodes);
        }
    }
    fn collect_exact(surface: &UiSurface) -> Vec<&UiNode> {
        let mut nodes = Vec::new();
        collect(&surface.root, &mut nodes);
        nodes
    }
    fn collect_env_nodes(surface: &UiSurface) -> Vec<&UiNode> {
        collect_exact(surface)
    }
    fn inspector_default_sky() -> UiSurface {
        inspector(
            StudioUiPalette::IndustrialDark,
            &EnvironmentSettings::default(),
            "sky",
            false,
            false,
        )
    }

    #[test]
    fn sky_preview_live_position_and_performance_summary_surface_runtime_state() {
        // Sky section previews the authored gradient strips always and reports
        // star density in the header chip.
        let surface = inspector_default_sky();
        let nodes = collect_exact(&surface);
        assert!(nodes
            .iter()
            .any(|n| n.id == "lighting.skyview.sky" && n.visual.gradient.is_some()));
        assert!(nodes
            .iter()
            .any(|n| n.id == "lighting.skyview.ground" && n.visual.gradient.is_some()));
        let header = nodes
            .iter()
            .find(|n| n.id == "lighting.skyview.header.value");
        assert_eq!(header.unwrap().text_value.as_deref(), Some("-"));
        // Stars that are actually on report their authored density.
        let mut starred = EnvironmentSettings::default();
        starred.sky.stars_enabled = true;
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &starred,
            "sky",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        let header = nodes
            .iter()
            .find(|n| n.id == "lighting.skyview.header.value");
        assert_eq!(header.unwrap().text_value.as_deref(), Some("0.35"));
        // Solid skies drop the ground strip.
        let mut solid = EnvironmentSettings::default();
        solid.sky.mode = SkyMode::Solid;
        let surface = inspector(StudioUiPalette::IndustrialDark, &solid, "sky", false, false);
        let nodes = collect_exact(&surface);
        assert!(!nodes.iter().any(|n| n.id == "lighting.skyview.ground"));
        // Manual sun keeps its angle inputs and shows no live row in Sun.
        let mut manual = EnvironmentSettings::default();
        manual.sun.control = SunControl::Manual;
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &manual,
            "sun",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        assert!(nodes.iter().any(|n| n.id == "lighting.field.sun.azimuth"));
        assert!(!nodes.iter().any(|n| n.id == "lighting.live.sun.value"));
        // Time shows the live clock readout even with a manual driver.
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &manual,
            "time",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        let clock = nodes
            .iter()
            .find(|n| n.id == "lighting.live.clock.value")
            .and_then(|n| n.text_value.clone());
        assert!(
            clock.as_deref().is_some_and(|t| t.ends_with(":00")),
            "{clock:?}"
        );
        // Clock-driven angles render the live sun position readout.
        let mut clocked = EnvironmentSettings::default();
        clocked.sun.control = SunControl::Clock;
        clocked.clock.time_seconds = 54000.0;
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &clocked,
            "sun",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        let sun_readout = nodes
            .iter()
            .find(|n| n.id == "lighting.live.sun.value")
            .and_then(|n| n.text_value.clone());
        assert!(
            sun_readout.as_deref().is_some_and(|t| t.contains('°')),
            "{sun_readout:?}"
        );
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &clocked,
            "time",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        assert!(nodes
            .iter()
            .any(|n| n.id == "lighting.live.time.sun.value" && n.text_value.is_some()));
        // Performance surfaces every trade-off as localized rows with values.
        let mut env = EnvironmentSettings::default();
        env.mode = EnvironmentMode::Scene;
        env.shadows.enabled = true;
        env.shadows.resolution = 1024.0;
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &env,
            "performance",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        for row in [
            "lighting.summary.technology.value",
            "lighting.summary.exposure.value",
            "lighting.summary.mode.value",
            "lighting.summary.shadows.value",
        ] {
            assert!(nodes.iter().any(|n| n.id == row), "{row} missing");
        }
        let shadows = nodes
            .iter()
            .find(|n| n.id == "lighting.summary.shadows.value")
            .and_then(|n| n.text_value.clone())
            .unwrap();
        assert_eq!(shadows, "1024");
        let mode = nodes
            .iter()
            .find(|n| n.id == "lighting.summary.mode.value")
            .unwrap();
        assert!(mode
            .text_key
            .as_deref()
            .is_some_and(|k| k.contains("scene")));
        // The error path styles in the danger token, not the body text color.
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &EnvironmentSettings::default(),
            "overview",
            false,
            true,
        );
        let nodes = collect_exact(&surface);
        let error = nodes.iter().find(|n| n.id == "lighting.error").unwrap();
        assert_eq!(
            error.text_style.as_ref().unwrap().color,
            StudioUiPalette::IndustrialDark.tokens().danger
        );
    }
    #[test]
    fn lighting_technologies_exposes_exactly_two_modes_and_gates_improved_only_controls() {
        for improved in [false, true] {
            let mut env = EnvironmentSettings::default();
            if improved {
                env.technology = raf_core::environment::LightingTechnology::OptimizedImproved;
            }
            let default_surface = inspector(
                StudioUiPalette::IndustrialDark,
                &env,
                "technologies",
                false,
                false,
            );
            let nodes = collect_env_nodes(&default_surface);
            let choices: Vec<_> = nodes
                .iter()
                .filter(|n| n.id.starts_with("lighting.choice.technology."))
                .collect();
            assert_eq!(choices.len(), 2);
            assert!(choices.iter().all(|n| !n.disabled));
            for (section, path) in [("overview", "exposure"), ("sun", "sun.toon_softness")] {
                let surface =
                    inspector(StudioUiPalette::IndustrialDark, &env, section, false, false);
                let nodes = collect_env_nodes(&surface);
                assert_eq!(
                    nodes
                        .iter()
                        .find(|n| n.id == value_key(path))
                        .unwrap()
                        .disabled,
                    !improved
                );
            }
        }
    }
    #[test]
    fn lighting_cpu_layout_is_scrollable_and_controls_stay_inside_the_inspector() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;
        let env = EnvironmentSettings::default();
        for (palette, name) in [
            (StudioUiPalette::IndustrialDark, "dark"),
            (StudioUiPalette::PaperLight, "light"),
        ] {
            for &section in SECTIONS {
                let mut host = CpuUiSurfaceHost::new(
                    inspector(palette, &env, section, false, false),
                    [0, 0, 0, 255],
                );
                for p in ENVIRONMENT_PROPERTIES {
                    if matches!(p.kind, Kind::Number { .. } | Kind::Color) {
                        host.session_mut().interaction.controls.set_text(
                            value_key(p.path),
                            display_value(&env, p.path),
                            32,
                        );
                    }
                }
                let frame = host.render([320, 800], |key| {
                    raf_core::i18n::t(key, raf_core::Language::Spanish)
                });
                for node in frame.frame.layout_boxes.iter().filter(|n| {
                    n.id.starts_with("lighting.value.") || n.id.starts_with("lighting.apply.")
                }) {
                    assert!(
                        node.rect.width > 0.0 && node.rect.x >= 0.0 && node.rect.right() <= 320.1,
                        "{}: {:?}",
                        node.id,
                        node.rect
                    );
                }
                for node in frame
                    .frame
                    .layout_boxes
                    .iter()
                    .filter(|n| n.id.starts_with("lighting.choice."))
                {
                    assert!(
                        node.rect.x >= 0.0 && node.rect.right() <= 296.1,
                        "{}: {:?}",
                        node.id,
                        node.rect
                    );
                }
                if section == "sun" {
                    assert!(frame
                        .frame
                        .scroll_metrics
                        .iter()
                        .any(|m| m.max_offset[1] > 0.0));
                }
                if let Some(dir) = std::env::var_os("RAF_LIGHTING_QA_DIR") {
                    let dir = std::path::PathBuf::from(dir);
                    std::fs::create_dir_all(&dir).unwrap();
                    image::save_buffer(
                        dir.join(format!("lighting-{name}-{section}.png")),
                        frame.pixels,
                        frame.size[0],
                        frame.size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
    #[test]
    fn lighting_surfaces_have_bilingual_labels_and_exact_units_without_a_selection() {
        fn walk(node: &UiNode) {
            if let Some(key) = &node.text_key {
                for language in [raf_core::Language::English, raf_core::Language::Spanish] {
                    assert_ne!(raf_core::i18n::t(key, language), *key, "{key}");
                }
            }
            for child in &node.children {
                walk(child);
            }
        }
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            for section in SECTIONS {
                walk(&navigation(palette, section).root);
                walk(
                    &inspector(
                        palette,
                        &EnvironmentSettings::default(),
                        section,
                        false,
                        false,
                    )
                    .root,
                );
            }
        }
        assert!(
            (parse_value("sun.elevation", "90")
                .unwrap()
                .as_f64()
                .unwrap()
                - std::f64::consts::FRAC_PI_2)
                .abs()
                < 1e-6
        );
        assert_eq!(
            parse_value("clock.time_seconds", "12").unwrap(),
            serde_json::json!(43200.0)
        );
        assert_eq!(
            parse_value("sun.color", "#FF8040").unwrap(),
            serde_json::json!([255, 128, 64])
        );
        assert!(parse_value("sun.color", "#notrgb").is_err());
        assert!(parse_value("sun.intensity", "NaN").is_err());
        assert!(
            (parse_value("sun.intensity", "0,7")
                .unwrap()
                .as_f64()
                .unwrap()
                - 0.7)
                .abs()
                < 1e-5
        );
    }

    #[test]
    fn number_fields_expose_scrub_step_slider_and_compact_formatting() {
        let env = EnvironmentSettings::default();
        let surface = inspector(StudioUiPalette::IndustrialDark, &env, "sun", false, false);
        let nodes = collect_exact(&surface);
        let find = |id: &str| nodes.iter().find(|node| node.id == id).copied();

        let input = find(&value_key("sun.intensity")).expect("numeric text input");
        assert!(input.control.text_input().is_some());
        let scrub = find("lighting.field.sun.intensity.scrub").expect("scrub handle");
        assert!(scrub.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "lighting.scrub.move:sun.intensity"
        )));
        assert!(scrub.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::KeyPress("ArrowUp".to_string())
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == "lighting.step:sun.intensity:1"
                )
        }));
        let step = find("lighting.field.sun.intensity.step.1").expect("step button");
        assert!(step.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "lighting.step:sun.intensity:1"
        )));
        let range = find("lighting.field.sun.intensity.range").expect("range");
        assert!(range.control.range().is_some());
        assert!(find("lighting.field.sun.intensity.reset").is_none());
        assert_eq!(display_value(&env, "sun.intensity"), "0.7");
        assert_eq!(display_value(&env, "sun.azimuth"), "34");
    }

    #[test]
    fn modified_number_fields_offer_a_reset_and_copy_paste() {
        let mut env = EnvironmentSettings::default();
        env.sun.intensity = 2.25;
        let surface = inspector(StudioUiPalette::IndustrialDark, &env, "sun", false, false);
        let nodes = collect_exact(&surface);
        let find = |id: &str| nodes.iter().find(|node| node.id == id).copied();

        assert!(find("lighting.field.sun.intensity.reset").is_some());
        assert!(find("lighting.field.sun.intensity.copy").is_some());
        let paste = find("lighting.field.sun.intensity.paste").expect("paste");
        assert!(paste.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "lighting.paste:sun.intensity"
        )));
    }

    #[test]
    fn color_fields_use_a_trigger_and_a_separate_overlay_popover() {
        let env = EnvironmentSettings::default();
        let mut ui = LightingUiState::default();
        ui.open_color = Some("ambient_color");
        let surface = inspector_with_state(
            StudioUiPalette::IndustrialDark,
            &env,
            "overview",
            false,
            false,
            &ui,
        );
        let nodes = collect_exact(&surface);
        let trigger = nodes
            .iter()
            .find(|node| node.id == "lighting.field.ambient_color.trigger")
            .expect("color trigger");
        assert!(trigger
            .classes
            .iter()
            .any(|class| class == "ui-color-trigger-open"));
        assert!(!nodes
            .iter()
            .any(|node| node.id == "lighting.field.ambient_color.popover"));
        let popover = color_popover_surface(
            StudioUiPalette::IndustrialDark,
            &env,
            "ambient_color",
            &[],
            None,
        )
        .expect("popover surface");
        assert!(popover
            .root
            .find("lighting.field.ambient_color.popover")
            .is_some());
        assert!(popover
            .root
            .find("lighting.field.ambient_color.visual.surface")
            .is_some());
    }

    #[test]
    fn sun_section_places_a_draggable_hemisphere_dial() {
        let env = EnvironmentSettings::default();
        let surface = inspector(StudioUiPalette::IndustrialDark, &env, "sun", false, false);
        let nodes = collect_exact(&surface);
        let dial = nodes
            .iter()
            .find(|node| node.id == "lighting.sun.dial")
            .expect("sun dial");
        assert!(dial.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "lighting.sun.dial.press"
        )));
        assert!(nodes
            .iter()
            .any(|node| node.id == "lighting.sun.dial.marker"));
        let readout = nodes
            .iter()
            .find(|node| node.id == "lighting.sun.dial.readout.value")
            .expect("sun readout");
        assert_eq!(readout.text_value.as_deref(), Some("34° / 48°"));
        let mut clocked = EnvironmentSettings::default();
        clocked.sun.control = SunControl::Clock;
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &clocked,
            "sun",
            false,
            false,
        );
        assert!(!collect_exact(&surface)
            .iter()
            .any(|node| node.id == "lighting.sun.dial"));
    }

    #[test]
    fn overview_renders_data_driven_environment_presets() {
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &EnvironmentSettings::default(),
            "overview",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        for preset in environment_presets() {
            let id = format!("lighting.preset.{}", preset.id);
            let button = nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("{id} missing"));
            assert!(button.event_handlers.iter().any(|binding| matches!(
                &binding.action,
                raf_ui::UiAction::Command { name }
                    if name == &format!("lighting.preset:{}", preset.id)
            )));
            let label = nodes
                .iter()
                .find(|node| node.id == format!("lighting.preset.{}.name", preset.id))
                .expect("preset label");
            assert_eq!(label.text_key.as_deref(), Some(preset.name_key.as_str()));
        }
    }

    #[test]
    fn color_popover_cpu_layout_has_room_for_picker_hex_and_channels() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;
        let mut env = EnvironmentSettings::default();
        env.ambient_color = [255, 128, 64];
        let surface = color_popover_surface(
            StudioUiPalette::IndustrialDark,
            &env,
            "ambient_color",
            &[[255, 128, 64], [32, 64, 128]],
            None,
        )
        .expect("color popover");
        let mut host = CpuUiSurfaceHost::new(surface, [0, 0, 0, 255]);
        host.session_mut().interaction.controls.set_text(
            "lighting.color.ambient_color.hex",
            "#FF8040",
            9,
        );
        let frame = host.render([292, 420], |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });
        for node in &frame.frame.layout_boxes {
            assert!(
                node.rect.x >= -0.1
                    && node.rect.y >= -0.1
                    && node.rect.right() <= 292.1
                    && node.rect.bottom() <= 420.1,
                "{}: {:?}",
                node.id,
                node.rect
            );
        }
        assert!(frame
            .frame
            .layout_boxes
            .iter()
            .any(|node| node.id == "lighting.field.ambient_color.visual.surface"));
        assert!(frame
            .frame
            .layout_boxes
            .iter()
            .any(|node| node.id == "lighting.field.ambient_color.hex.input"
                || node.id == "lighting.color.ambient_color.hex"));
        assert!(frame
            .frame
            .layout_boxes
            .iter()
            .any(|node| node.id == "lighting.field.ambient_color.channel-row.r"));
        if let Some(dir) = std::env::var_os("RAF_LIGHTING_QA_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image::save_buffer(
                dir.join("lighting-color-popover.png"),
                frame.pixels,
                frame.size[0],
                frame.size[1],
                image::ColorType::Rgba8,
            )
            .unwrap();
            let light = color_popover_surface(
                StudioUiPalette::PaperLight,
                &env,
                "ambient_color",
                &[[255, 128, 64], [32, 64, 128]],
                None,
            )
            .expect("light color popover");
            let mut light_host = CpuUiSurfaceHost::new(light, [255, 255, 255, 255]);
            light_host.session_mut().interaction.controls.set_text(
                "lighting.color.ambient_color.hex",
                "#FF8040",
                9,
            );
            let light_frame = light_host.render([292, 420], |key| {
                raf_core::i18n::t(key, raf_core::Language::Spanish)
            });
            image::save_buffer(
                dir.join("lighting-color-popover-light.png"),
                light_frame.pixels,
                light_frame.size[0],
                light_frame.size[1],
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }

    #[test]
    fn eyedropper_state_renders_the_pick_hint_and_cancel_action() {
        let env = EnvironmentSettings::default();
        let ui = LightingUiState {
            open_color: None,
            eyedropper: Some("sun.color"),
            recent_colors: &[],
        };
        let surface = inspector_with_state(
            StudioUiPalette::IndustrialDark,
            &env,
            "sun",
            false,
            false,
            &ui,
        );
        let nodes = collect_exact(&surface);
        assert!(nodes
            .iter()
            .any(|node| node.id == "lighting.eyedropper.hint"));
        let cancel = nodes
            .iter()
            .find(|node| node.id == "lighting.eyedropper.cancel")
            .expect("cancel");
        assert!(cancel.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "lighting.eyedropper.cancel"
        )));
    }

    #[test]
    fn active_preset_is_marked_when_the_world_matches_it() {
        let preset = environment_presets().first().expect("preset");
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &preset.settings,
            "overview",
            false,
            false,
        );
        let nodes = collect_exact(&surface);
        let button = nodes
            .iter()
            .find(|node| node.id == format!("lighting.preset.{}", preset.id))
            .expect("active preset");
        assert!(button
            .classes
            .iter()
            .any(|class| class == "lighting-preset-active"));
    }

    #[test]
    fn sky_bands_and_chips_open_the_authored_color_picker() {
        let env = EnvironmentSettings::default();
        let surface = inspector(StudioUiPalette::IndustrialDark, &env, "sky", false, false);
        let nodes = collect_exact(&surface);
        for (id, path) in [
            ("lighting.skyview.edit.top", "sky.top_color"),
            ("lighting.skyview.edit.horizon", "sky.horizon_color"),
            ("lighting.skyview.edit.ground", "sky.ground_color"),
        ] {
            let chip = nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("{id} missing"));
            assert!(chip.event_handlers.iter().any(|binding| matches!(
                &binding.action,
                raf_ui::UiAction::Command { name }
                    if name == &format!("lighting.sky.edit:{path}")
            )));
        }
        let top = nodes
            .iter()
            .find(|node| node.id == "lighting.skyview.sky")
            .expect("top band");
        assert!(top.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "lighting.sky.edit:sky.top_color"
        )));
    }
}
