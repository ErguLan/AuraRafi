//! Lighting workbench presentation. World data, never synthetic hierarchy entities.
use raf_core::environment::{
    EnvironmentPropertyKind as Kind, EnvironmentSettings, SunControl, ENVIRONMENT_PROPERTIES,
};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiLayout, UiNode, UiNodeKind,
    UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiTextInput, UiTextStyle, UiToggle,
};

pub const SECTIONS: &[&str] = &["overview", "sun", "sky", "shadows", "time", "performance"];
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
        root = root.with_child(label(palette, "lighting.error", "lighting.invalid_value"));
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
    for property in ENVIRONMENT_PROPERTIES.iter().filter(|p| match section {
        "sun" => p.path.starts_with("sun."),
        "sky" => p.path.starts_with("sky."),
        "shadows" => p.path.starts_with("shadows."),
        "time" => p.path.starts_with("clock.") || p.path == "sun.control",
        "overview" => matches!(p.path, "mode" | "ambient_color" | "ambient_intensity"),
        _ => false,
    }) {
        let path = property.path;
        let value = env.get_property(path).expect("Core property catalog");
        let disabled =
            env.sun.control == SunControl::Clock && matches!(path, "sun.azimuth" | "sun.elevation");
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
                "lighting.clock_controls_angles",
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
    #[test]
    fn lighting_cpu_layout_is_scrollable_and_controls_stay_inside_the_inspector() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;
        let env = EnvironmentSettings::default();
        for (palette, name) in [
            (StudioUiPalette::IndustrialDark, "dark"),
            (StudioUiPalette::PaperLight, "light"),
        ] {
            for section in ["overview", "sun", "sky", "shadows", "time", "performance"] {
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
