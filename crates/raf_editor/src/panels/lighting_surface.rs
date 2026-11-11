//! Lighting workbench presentation. World data, never synthetic hierarchy entities.
use raf_core::environment::{
    EnvironmentMode, EnvironmentPropertyKind as Kind, EnvironmentSettings, LightingTechnology,
    SkyMode, SunControl, ENVIRONMENT_PROPERTIES,
};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiLayout, UiNode, UiNodeKind,
    UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiTextOverflow, UiTextInput, UiTextStyle, UiToggle,
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
fn sky_preview(palette: StudioUiPalette, env: &EnvironmentSettings) -> UiNode {
    let sky = env.sky;
    let strip = |id: &str, start: [u8; 3], end: [u8; 3], height: f32| {
        UiNode::new(id, UiNodeKind::Panel)
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
        ));
    if sky.mode == SkyMode::Gradient {
        stack = stack.with_child(strip(
            "lighting.skyview.ground",
            sky.horizon_color,
            sky.ground_color,
            24.0,
        ));
    }
    let stars = if sky.stars_enabled {
        format!("{:.2}", sky.stars_density)
    } else {
        "-".to_string()
    };
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
        ReadoutValue::Choice(choice) => {
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_key(choice)
                .with_text_overflow(UiTextOverflow::Clip)
        }
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
                .with_text_style(value_text_style),
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
        root = root.with_child(sky_preview(palette, env));
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
        "sky" => p.path.starts_with("sky."),
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
        let mut field = UiNode::new(format!("lighting.field.{path}"), UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 4.0,
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(format!("lighting.field-header.{path}"), UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 4.0,
                        ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        label(palette, format!("lighting.label.{path}"), property.label)
                            .with_layout(UiLayout {
                                grow: 1.0,
                                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                            }),
                    )
                    .with_child({
                        let mut reset = button(
                            palette,
                            format!("lighting.reset.{path}"),
                            "lighting.reset",
                            format!("lighting.reset:{path}"),
                            false,
                        );
                        reset.text_key = None;
                        reset
                            .with_icon(UiIcon::new(UiIconId::Refresh))
                            .with_layout(UiLayout::fixed(26.0, 26.0))
                            .disabled(disabled)
                    }),
            );
        match property.kind {
            Kind::Bool => {
                field = field.with_child(
                    UiNode::toggle(
                        format!("lighting.control.{path}"),
                        UiToggle::new(value_key(path), value.as_bool().unwrap()),
                    )
                    .with_accessibility_label_key(property.label)
                    .with_layout(UiLayout::fixed(32.0, 26.0)),
                )
            }
            Kind::Choice(choices) => {
                let mut group = UiNode::new(format!("lighting.choices.{path}"), UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::RowWrap,
                        gap: 4.0,
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
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
                            padding: UiSpacing::xy(8.0, 0.0),
                            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::FitContent)
                        }),
                    );
                }
                field = field.with_child(group);
            }
            Kind::Number { .. } if path == "shadows.resolution" => {
                for resolution in [256, 512, 1024] {
                    field = field.with_child(button(
                        palette,
                        format!("lighting.resolution.{resolution}"),
                        &format!("lighting.choice_{resolution}"),
                        format!("lighting.choose:{path}:{resolution}"),
                        value.as_f64() == Some(resolution as f64),
                    ));
                }
            }
            Kind::Number { .. } | Kind::Color => {
                let mut input = UiTextInput::new(value_key(path));
                input.max_length = 32;
                input.submit_command = Some(format!("lighting.commit:{path}"));
                let mut row = UiNode::new(format!("lighting.edit.{path}"), UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 4.0,
                        ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
                    });
                row = row.with_child(
                    UiNode::text_input(value_key(path), input)
                        .with_style(palette.subtle_panel_style())
                        .with_accessibility_label_key(property.label)
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(80.0, 30.0).with_width_mode(UiSizeMode::Fill)
                        })
                        .disabled(disabled),
                );
                row = row.with_child(
                    button(
                        palette,
                        format!("lighting.apply.{path}"),
                        "lighting.apply",
                        format!("lighting.commit:{path}"),
                        false,
                    )
                    .with_layout(UiLayout {
                        padding: UiSpacing {
                            left: 8.0,
                            right: 8.0,
                            ..UiSpacing::ZERO
                        },
                        ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::FitContent)
                    })
                    .disabled(disabled),
                );
                field = field.with_child(row);
                if matches!(property.kind, Kind::Color) {
                    let c = value.as_array().unwrap();
                    let mut swatch = palette.subtle_panel_style();
                    swatch.fill = [
                        c[0].as_u64().unwrap() as u8,
                        c[1].as_u64().unwrap() as u8,
                        c[2].as_u64().unwrap() as u8,
                        255,
                    ];
                    field = field.with_child(
                        UiNode::new(format!("lighting.swatch.{path}"), UiNodeKind::Panel)
                            .with_style(swatch)
                            .with_layout(
                                UiLayout::fixed(0.0, 10.0).with_width_mode(UiSizeMode::Fill),
                            ),
                    );
                } else {
                    field = field.with_child(label(
                        palette,
                        format!("lighting.unit.{path}"),
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
                        } else if matches!(path, "shadows.distance" | "shadows.bias") {
                            "lighting.unit_world"
                        } else {
                            "lighting.unit_factor"
                        },
                    ));
                }
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
        Kind::Number { display_scale, .. } => {
            format!("{:.5}", value.as_f64().unwrap() * display_scale as f64)
        }
        _ => value.to_string(),
    }
}
pub fn parse_value(path: &str, text: &str) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let property = raf_core::environment::environment_property(path)?;
    match property.kind {
        Kind::Color => {
            let s = text.trim().trim_start_matches('#');
            if s.len() != 6 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("Expected #RRGGBB".into());
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
            let n =
                text.trim().parse::<f64>().map_err(|_| "Expected number")? / display_scale as f64;
            if !n.is_finite() || n < min as f64 - 1e-5 || n > max as f64 + 1e-5 {
                return Err("Number outside range".into());
            }
            Ok(json!((n as f32).clamp(min, max)))
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
        let header = nodes.iter().find(|n| n.id == "lighting.skyview.header.value");
        assert_eq!(header.unwrap().text_value.as_deref(), Some("-"));
        // Stars that are actually on report their authored density.
        let mut starred = EnvironmentSettings::default();
        starred.sky.stars_enabled = true;
        let surface = inspector(StudioUiPalette::IndustrialDark, &starred, "sky", false, false);
        let nodes = collect_exact(&surface);
        let header = nodes.iter().find(|n| n.id == "lighting.skyview.header.value");
        assert_eq!(header.unwrap().text_value.as_deref(), Some("0.35"));
        // Solid skies drop the ground strip.
        let mut solid = EnvironmentSettings::default();
        solid.sky.mode = SkyMode::Solid;
        let surface = inspector(
            StudioUiPalette::IndustrialDark,
            &solid,
            "sky",
            false,
            false,
        );
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
            sun_readout
                .as_deref()
                .is_some_and(|t| t.contains('°')),
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
            assert!(
                nodes.iter().any(|n| n.id == row),
                "{row} missing"
            );
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
        assert!(mode.text_key.as_deref().is_some_and(|k| k.contains("scene")));
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
    }
}
