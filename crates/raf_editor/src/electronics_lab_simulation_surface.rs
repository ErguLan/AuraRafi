//! Layout and instrument readings for the DC laboratory.
use super::{action, column, label, raw, scroll, value_button};
use crate::electronics_controller::NativeElectronicsEditor;
use crate::electronics_lab::{DcRun, LabView};
use crate::electronics_lab_field::{Field, IMAGE_KEY, PART_SIZE};
use raf_core::{i18n::t, Language};
use raf_ui::*;

const GAP: f32 = 12.0;

struct Geometry {
    left: Option<UiRect>,
    field: UiRect,
    comparison: Option<UiRect>,
    right: UiRect,
}

impl Geometry {
    fn resolve(rect: UiRect) -> Self {
        let left_width = if rect.width >= 1180.0 { 196.0 } else { 0.0 };
        let right_width = if rect.width >= 690.0 { 244.0 } else { 0.0 };
        let x = GAP
            + if left_width > 0.0 {
                left_width + GAP
            } else {
                0.0
            };
        let width = (rect.width
            - x
            - GAP
            - if right_width > 0.0 {
                right_width + GAP
            } else {
                0.0
            })
        .max(80.0);
        let height = (rect.height - GAP * 2.0).max(180.0);
        let comparison_height = if height >= 400.0 { 144.0 } else { 0.0 };
        let field_height = height
            - if comparison_height > 0.0 {
                comparison_height + GAP
            } else {
                0.0
            };
        Self {
            left: (left_width > 0.0).then_some(UiRect::new(GAP, GAP, left_width, height)),
            field: UiRect::new(x, GAP, width, field_height),
            comparison: (comparison_height > 0.0).then_some(UiRect::new(
                x,
                GAP * 2.0 + field_height,
                width,
                comparison_height,
            )),
            right: if right_width > 0.0 {
                UiRect::new(x + width + GAP, GAP, right_width, height)
            } else {
                UiRect::new(GAP, GAP, 0.0, 0.0)
            },
        }
    }
}

pub(crate) fn field_size(rect: UiRect, view: LabView) -> Option<[f32; 2]> {
    match view {
        LabView::Simulation => {
            let field = Geometry::resolve(rect).field;
            Some([field.width, field.height])
        }
        LabView::Device => {
            let right = if rect.width >= 730.0 { 255.0 } else { 0.0 };
            Some([(rect.width - right - 20.0).max(100.0), 245.0])
        }
        _ => None,
    }
}

pub(super) fn body(
    editor: &NativeElectronicsEditor,
    rect: UiRect,
    tokens: &UiTokens,
    language: Language,
) -> UiNode {
    let geometry = Geometry::resolve(rect);
    let run = editor.lab.run();
    let current = run.filter(|run| run.fingerprint == editor.lab_fingerprint());
    let mut shell =
        UiNode::new("lab.body", UiNodeKind::Panel).with_layout(UiLayout::absolute(rect));
    let mut map = signal_map(editor, geometry.field, tokens, language, true);
    if geometry.left.is_none() {
        let x = (geometry.field.width - 152.0).max(0.0);
        map = map.with_child(
            run_button(editor, tokens)
                .with_layout(UiLayout::absolute(UiRect::new(x, 14.0, 134.0, 32.0))),
        );
    }
    if geometry.right.width == 0.0 {
        let y = (geometry.field.height - 57.0).max(58.0);
        let summary = editor
            .lab
            .selected
            .and_then(|id| reading(current, id, editor.lab.selected_pin))
            .map(|value| {
                format!(
                    "{}    {}",
                    value.designator,
                    format_value(value.value, &value.unit)
                )
            })
            .unwrap_or_else(|| t("lab.select_part", language));
        map = map.with_child(raw("lab.compact.probe", summary, tokens.text).with_layout(
            UiLayout::absolute(UiRect::new(15.0, y, geometry.field.width - 30.0, 21.0)),
        ));
    }
    shell = shell.with_child(map);
    if let Some(area) = geometry.left {
        shell = shell.with_child(instruments(editor, area, tokens, language));
    }
    if geometry.right.width > 0.0 {
        shell = shell.with_child(inspector(editor, geometry.right, tokens, language));
    }
    if let Some(area) = geometry.comparison {
        shell = shell.with_child(comparison(editor, current, area, tokens, language));
    }
    shell
}

pub(super) fn signal_map(
    editor: &NativeElectronicsEditor,
    rect: UiRect,
    tokens: &UiTokens,
    language: Language,
    show_title: bool,
) -> UiNode {
    let lab = &editor.lab;
    let [width, height] = [rect.width, rect.height];
    let field = Field::new(lab, width, height);
    let mut root = UiNode::new("lab.circuit", UiNodeKind::Panel)
        .with_layout(UiLayout::absolute(rect))
        .with_style(
            UiStyle::transparent()
                .with_fill(tokens.surface)
                .with_radius(10.0)
                .with_border(tokens.border, 1.0),
        )
        .with_visual(UiVisual {
            clip_children: true,
            ..Default::default()
        })
        .with_child(
            UiNode::image(
                "lab.circuit.field",
                UiImage {
                    source: UiImageSource::new(IMAGE_KEY),
                    fit: UiImageFit::Stretch,
                    tint: None,
                    options: UiImageOptions {
                        placeholder: UiImagePlaceholder::Hidden,
                        ..Default::default()
                    },
                },
            )
            .with_layout(UiLayout::absolute(UiRect::new(0.0, 0.0, width, height))),
        );
    if show_title {
        root = root
            .with_child(
                label("lab.circuit.title", "lab.signal_map", tokens.text)
                    .with_text_style(UiTextStyle::panel_title(tokens.text).with_size(17.0))
                    .with_layout(UiLayout::absolute(UiRect::new(
                        17.0,
                        12.0,
                        (width - 177.0).max(90.0),
                        24.0,
                    ))),
            )
            .with_child(
                raw(
                    "lab.circuit.document",
                    editor.schematic().name.as_str(),
                    tokens.text_muted,
                )
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::absolute(UiRect::new(
                    17.0,
                    37.0,
                    (width - 177.0).max(90.0),
                    21.0,
                ))),
            );
    }
    if field.parts.is_empty() {
        return root.with_child(
            label("lab.circuit.empty", "lab.circuit_empty", tokens.text_muted).with_layout(
                UiLayout::absolute(UiRect::new(17.0, height * 0.52, width - 34.0, 24.0)),
            ),
        );
    }
    if field.parts.len() <= 18 {
        for (i, net) in field.net_labels.iter().enumerate() {
            let color = if field.highlighted(net) {
                tokens.accent_hot
            } else {
                tokens.text_muted
            };
            root = root.with_child(
                raw(&format!("lab.net.{i}"), net.name, color)
                    .with_text_style(UiTextStyle::label(color).with_size(10.0))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout::absolute(UiRect::new(
                        net.at.x * width + 10.0,
                        net.at.y * height + 4.0,
                        66.0,
                        16.0,
                    ))),
            );
        }
    }
    for part in &field.parts {
        let id = part.model.id;
        let x = part.at.x * width;
        let y = part.at.y * height;
        let selected = lab.selected == Some(id);
        let color = if selected {
            tokens.accent_hot
        } else {
            tokens.text
        };
        root = root.with_child(
            UiNode::new(format!("lab.part.{id}"), UiNodeKind::Button)
                .with_text_value(part.model.name.clone())
                .with_text_style(UiTextStyle::button(color))
                .with_visual(UiVisual {
                    content: UiContentLayout {
                        align: UiContentAlign::Center,
                        ..Default::default()
                    },
                    focus_ring: Some(UiFocusRing {
                        color: tokens.focus,
                        width: 1.0,
                        offset: -1.0,
                    }),
                    ..Default::default()
                })
                .with_style(
                    UiStyle::transparent()
                        .with_fill(tokens.surface_raised)
                        .with_border(
                            if selected {
                                tokens.accent_hot
                            } else {
                                tokens.border
                            },
                            1.0,
                        )
                        .with_radius(5.0),
                )
                .with_layout(UiLayout::absolute(UiRect::new(
                    x - PART_SIZE[0] * 0.5,
                    y - PART_SIZE[1] * 0.5,
                    PART_SIZE[0],
                    PART_SIZE[1],
                )))
                .with_class("lab.signal-target")
                .with_accessibility_selected(selected)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.lab.part:{id}"),
                )),
        );
        if selected && show_title {
            let run = lab
                .run()
                .filter(|run| run.converged && run.fingerprint == editor.lab_fingerprint());
            if let Some(value) = reading(run, id, lab.selected_pin) {
                let tag_width = 110.0;
                let tag_x = if x + 36.0 + tag_width < width {
                    x + 36.0
                } else {
                    x - tag_width - 36.0
                }
                .max(12.0);
                let tag_y = (y - 42.0).max(59.0);
                root = root.with_child(
                    raw(
                        "lab.circuit.probe_tag",
                        format!(
                            "{}  {}",
                            part.model.name,
                            format_value(value.value, &value.unit)
                        ),
                        tokens.accent_hot,
                    )
                    .with_text_style(UiTextStyle::button(tokens.accent_hot))
                    .with_layout(UiLayout {
                        padding: UiSpacing::xy(8.0, 2.0),
                        ..UiLayout::absolute(UiRect::new(tag_x, tag_y, tag_width, 24.0))
                            .with_z_index(8)
                    })
                    .with_style(
                        UiStyle::transparent()
                            .with_fill(tokens.surface_raised)
                            .with_border(tokens.accent_hot, 1.0)
                            .with_radius(4.0),
                    ),
                );
            }
        }
    }
    let source = if field.limited {
        t("lab.circuit_limited", language)
    } else {
        format!(
            "{}  /  {}",
            t("lab.topology", language),
            t("lab.no_time_axis", language)
        )
    };
    root.with_child(
        raw("lab.circuit.legend", source, tokens.text_muted)
            .with_text_style(UiTextStyle::label(tokens.text_muted))
            .with_layout(UiLayout::absolute(UiRect::new(
                17.0,
                height - 27.0,
                width - 34.0,
                18.0,
            ))),
    )
}

fn instruments(
    editor: &NativeElectronicsEditor,
    rect: UiRect,
    tokens: &UiTokens,
    language: Language,
) -> UiNode {
    let lab = &editor.lab;
    let status = if lab.dc_requested {
        "lab.solving"
    } else if let Some(run) = lab.run() {
        if run.fingerprint != editor.lab_fingerprint() {
            "lab.old_revision"
        } else if run.converged {
            "lab.converged"
        } else {
            "lab.not_converged"
        }
    } else {
        "lab.no_run"
    };
    let mut content = column("lab.instrument.content")
        .with_child(section("lab.analysis.heading", "lab.analysis", tokens.text))
        .with_child(
            raw("lab.analysis.mode", "DC", tokens.text)
                .with_text_style(UiTextStyle::panel_title(tokens.text).with_size(28.0)),
        )
        .with_child(label(
            "lab.dc.status",
            status,
            if status == "lab.converged" {
                tokens.positive
            } else {
                tokens.text_muted
            },
        ))
        .with_child(run_button(editor, tokens));
    if let Some(graph) = &lab.graph {
        content = content.with_child(raw(
            "lab.analysis.graph",
            format!(
                "{} {}  ·  {} {}",
                graph.parts.len(),
                t("lab.components", language),
                graph.nets.len(),
                t("lab.networks", language),
            ),
            tokens.text_muted,
        ));
        if let Some((source, run)) = graph.parts.iter().find(|part| part.is_source).zip(
            lab.run()
                .filter(|run| run.converged && run.fingerprint == editor.lab_fingerprint()),
        ) {
            let voltage = reading(Some(run), source.id, Some(0))
                .zip(reading(Some(run), source.id, Some(1)))
                .map(|(positive, negative)| positive.value - negative.value);
            content = content
                .with_child(rule("lab.supply.rule", tokens))
                .with_child(section("lab.supply.title", "lab.supply", tokens.text));
            if let Some(voltage) = voltage {
                content = content.with_child(raw(
                    "lab.supply.voltage",
                    format!("{}   {voltage:.3} V", source.name),
                    tokens.text,
                ));
            }
            if let Some(current) = reading(Some(run), source.id, None) {
                content = content.with_child(raw(
                    "lab.supply.current",
                    format!("I   {}", format_value(current.value.abs(), &current.unit)),
                    tokens.text_muted,
                ));
            }
        }
    }
    if let Some(error) = lab.error.as_deref().or(lab.dc_failure.as_deref()) {
        content = content.with_child(
            raw("lab.dc.error", error, tokens.accent_hot).with_text_overflow(UiTextOverflow::Wrap),
        );
    }
    content = content
        .with_child(rule("lab.analysis.rule", tokens))
        .with_child(section("lab.history.title", "lab.runs", tokens.text));
    if lab.runs().is_empty() {
        content = content.with_child(label(
            "lab.history.empty",
            "lab.history_empty",
            tokens.text_muted,
        ));
    }
    for (i, run) in lab.runs().iter().enumerate() {
        let when = chrono::DateTime::from_timestamp_millis(run.unix_ms)
            .map(|date| {
                date.with_timezone(&chrono::Local)
                    .format("%d/%m  %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let selected = lab.run().is_some_and(|active| active.id == run.id);
        content = content.with_child(
            value_button(
                &format!("run.{i}"),
                when,
                &format!("run:{}", run.id),
                tokens,
            )
            .with_style(UiStyle::transparent().with_fill(if selected {
                tokens.selection
            } else {
                tokens.surface
            }))
            .with_text_style(UiTextStyle::button(if selected {
                tokens.text
            } else {
                tokens.text_muted
            }))
            .with_accessibility_selected(selected),
        );
    }
    instrument_panel("lab.instrument", rect, tokens, content)
}

fn inspector(
    editor: &NativeElectronicsEditor,
    rect: UiRect,
    tokens: &UiTokens,
    language: Language,
) -> UiNode {
    let lab = &editor.lab;
    let run = lab.run();
    let current = run.filter(|run| run.fingerprint == editor.lab_fingerprint() && run.converged);
    let mut content = column("lab.scope.content").with_child(section(
        "lab.scope.title",
        "lab.probes",
        tokens.text,
    ));
    if let Some(part) = editor
        .schematic()
        .components
        .iter()
        .find(|part| Some(part.id) == lab.selected)
    {
        content = content
            .with_child(
                raw(
                    "lab.probe.designator",
                    part.designator.as_str(),
                    tokens.text,
                )
                .with_text_style(UiTextStyle::panel_title(tokens.text).with_size(23.0)),
            )
            .with_child(raw(
                "lab.probe.part_value",
                part.value.as_str(),
                tokens.text_muted,
            ));
        if let Some(value) = reading(current, part.id, lab.selected_pin) {
            content = content.with_child(
                raw(
                    "lab.probe.value",
                    format_value(value.value, &value.unit),
                    tokens.text,
                )
                .with_text_style(UiTextStyle::panel_title(tokens.text).with_size(30.0)),
            );
        } else {
            content = content.with_child(label(
                "lab.probe.unavailable",
                "lab.no_reading",
                tokens.text_muted,
            ));
        }
        let mut channels = vec![(None, t("lab.branch", language))];
        channels.extend(
            part.pins
                .iter()
                .take(12)
                .enumerate()
                .map(|(i, pin)| (Some(i), pin.name.clone())),
        );
        for (pin, name) in channels {
            let selected = lab.selected_pin == pin;
            let value = reading(current, part.id, pin)
                .map_or("—".to_string(), |v| format_value(v.value, &v.unit));
            let command = pin.map_or("pin".to_string(), |i| format!("pin:{i}"));
            content = content.with_child(
                value_button(
                    &format!("scope.{command}"),
                    format!("{name}   {value}"),
                    &command,
                    tokens,
                )
                .with_style(UiStyle::transparent().with_fill(if selected {
                    tokens.selection
                } else {
                    tokens.surface
                }))
                .with_text_style(UiTextStyle::button(if selected {
                    tokens.text
                } else {
                    tokens.text
                }))
                .with_accessibility_selected(selected),
            );
        }
    } else {
        content = content.with_child(
            label("lab.probe.empty", "lab.select_part", tokens.text_muted)
                .with_text_overflow(UiTextOverflow::Wrap),
        );
    }
    if let Some(run) = run {
        content = content
            .with_child(rule("lab.scope.rule", tokens))
            .with_child(section(
                "lab.scope.balance.title",
                "lab.integrity",
                tokens.text,
            ))
            .with_child(raw(
                "lab.scope.kcl",
                format!("KCL     {:.2e} A", run.kcl_residual_a),
                tokens.text_muted,
            ))
            .with_child(raw(
                "lab.scope.power",
                format!("ΔP        {:.2e} W", run.power_residual_w),
                tokens.text_muted,
            ))
            .with_child(
                action("details", "lab.model_details", "model-details", tokens)
                    .with_accessibility_expanded(lab.show_model_details),
            );
        if lab.show_model_details {
            for (i, message) in run.messages.iter().take(8).enumerate() {
                content = content.with_child(
                    raw(&format!("lab.dc.message.{i}"), message, tokens.text_muted)
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_layout(
                            UiLayout::fit_content().fixed_width((rect.width - 28.0).max(1.0)),
                        ),
                );
            }
        }
    }
    if let Some(run) = current {
        if let Some(graph) = &lab.graph {
            let values: Vec<_> = graph
                .nets
                .iter()
                .filter_map(|(id, name)| {
                    let (part, pin, _) = graph.edges.iter().find(|(_, _, net)| net == id)?;
                    let reading =
                        reading(Some(run), *part, Some(*pin)).filter(|value| value.unit == "V")?;
                    Some((name, reading.value))
                })
                .take(12)
                .collect();
            if !values.is_empty() {
                let peak = values
                    .iter()
                    .map(|(_, value)| value.abs())
                    .fold(1e-12_f64, f64::max);
                content = content
                    .with_child(rule("lab.scope.net.rule", tokens))
                    .with_child(section(
                        "lab.scope.net.title",
                        "lab.net_voltage",
                        tokens.text,
                    ));
                for (index, (name, value)) in values.into_iter().enumerate() {
                    content = content
                        .with_child(raw(
                            &format!("lab.scope.net.{index}"),
                            format!("{name}    {value:.4} V"),
                            tokens.text_muted,
                        ))
                        .with_child(
                            UiNode::new(format!("lab.scope.net_bar.{index}"), UiNodeKind::Panel)
                                .with_style(
                                    UiStyle::transparent()
                                        .with_fill(tokens.text_muted)
                                        .with_radius(1.0),
                                )
                                .with_layout(UiLayout::fixed(
                                    ((rect.width - 32.0) * (value.abs() / peak) as f32).max(1.0),
                                    3.0,
                                )),
                        );
                }
            }
        }
    }
    instrument_panel("lab.scope", rect, tokens, content)
}

fn comparison(
    editor: &NativeElectronicsEditor,
    run: Option<&DcRun>,
    rect: UiRect,
    tokens: &UiTokens,
    language: Language,
) -> UiNode {
    let mut root = UiNode::new("lab.compare", UiNodeKind::Panel)
        .with_layout(UiLayout::absolute(rect))
        .with_style(
            UiStyle::transparent()
                .with_fill(tokens.surface)
                .with_radius(8.0)
                .with_border(tokens.border, 1.0),
        )
        .with_child(
            section("lab.compare.title", "lab.branch_comparison", tokens.text).with_layout(
                UiLayout::absolute(UiRect::new(15.0, 10.0, rect.width - 30.0, 20.0)),
            ),
        );
    let capacity = ((rect.width - 32.0) / 55.0).floor().clamp(1.0, 16.0) as usize;
    let graph = editor.lab.graph.as_ref();
    let has_leds = graph.is_some_and(|graph| graph.parts.iter().any(|part| part.is_led));
    let values: Vec<_> = run
        .filter(|run| run.converged)
        .into_iter()
        .flat_map(|run| run.values.iter())
        .filter(|value| {
            let part =
                graph.and_then(|graph| graph.parts.iter().find(|part| part.id == value.component));
            value.pin.is_none()
                && value.unit == "A"
                && part.is_none_or(|part| {
                    if has_leds {
                        part.is_led
                    } else {
                        !part.is_source
                    }
                })
        })
        .take(capacity)
        .collect();
    if values.is_empty() {
        return root.with_child(
            label("lab.compare.empty", "lab.no_run", tokens.text_muted).with_layout(
                UiLayout::absolute(UiRect::new(15.0, 57.0, rect.width - 30.0, 20.0)),
            ),
        );
    }
    let peak = values
        .iter()
        .map(|value| value.value.abs())
        .fold(1e-12_f64, f64::max);
    let cell = (rect.width - 32.0) / values.len() as f32;
    for (i, value) in values.into_iter().enumerate() {
        let x = 16.0 + i as f32 * cell;
        let selected = editor.lab.selected == Some(value.component);
        let color = if selected {
            tokens.accent_hot
        } else {
            tokens.text
        };
        let length = ((value.value.abs() / peak) as f32 * (cell - 14.0)).max(2.0);
        root = root.with_child(
            UiNode::new(format!("lab.comparison.{i}"), UiNodeKind::Button)
                .with_layout(UiLayout::absolute(UiRect::new(x, 40.0, cell, 84.0)))
                .with_style(UiStyle::transparent().with_fill(if selected {
                    tokens.selection
                } else {
                    tokens.surface
                }))
                .with_class("lab.signal-target")
                .with_text_value(format!(
                    "{}  {}",
                    value.designator,
                    format_value(value.value, &value.unit)
                ))
                .with_text_style(UiTextStyle::button([0; 4]))
                .with_accessibility_selected(selected)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.lab.part:{}", value.component),
                ))
                .with_child(
                    raw(
                        &format!("lab.comparison.name.{i}"),
                        value.designator.as_str(),
                        tokens.text_muted,
                    )
                    .with_layout(UiLayout::absolute(UiRect::new(
                        5.0,
                        5.0,
                        cell - 10.0,
                        17.0,
                    ))),
                )
                .with_child(
                    raw(
                        &format!("lab.comparison.value.{i}"),
                        format!("{:+.2}", value.value * 1000.0),
                        color,
                    )
                    .with_text_style(UiTextStyle::button(color).with_size(13.0))
                    .with_layout(UiLayout::absolute(UiRect::new(
                        5.0,
                        27.0,
                        cell - 10.0,
                        20.0,
                    ))),
                )
                .with_child(
                    UiNode::new(format!("lab.comparison.bar.{i}"), UiNodeKind::Panel)
                        .with_style(UiStyle::transparent().with_fill(color).with_radius(2.0))
                        .with_layout(UiLayout::absolute(UiRect::new(5.0, 59.0, length, 4.0))),
                ),
        );
    }
    root.with_child(
        raw(
            "lab.compare.unit",
            t("lab.current_scale", language),
            tokens.text_muted,
        )
        .with_text_style(UiTextStyle::label(tokens.text_muted).with_size(10.0))
        .with_layout(UiLayout::absolute(UiRect::new(
            15.0,
            125.0,
            rect.width - 30.0,
            16.0,
        ))),
    )
}

fn reading(
    run: Option<&DcRun>,
    component: uuid::Uuid,
    pin: Option<usize>,
) -> Option<&crate::electronics_lab::DcValue> {
    run?.values
        .iter()
        .find(|value| value.component == component && value.pin == pin)
}

fn format_value(value: f64, unit: &str) -> String {
    if unit == "A" && value.abs() < 1.0 {
        format!("{:.3} mA", value * 1000.0)
    } else {
        format!("{value:.4} {unit}")
    }
}

fn instrument_panel(id: &str, rect: UiRect, tokens: &UiTokens, mut content: UiNode) -> UiNode {
    content.layout = UiLayout {
        flow: UiFlow::Column,
        gap: 10.0,
        padding: UiSpacing::same(14.0),
        ..UiLayout::fit_content().fixed_width(rect.width)
    };
    scroll(id, content, rect.width, rect.height)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(rect)
        })
        .with_style(
            UiStyle::transparent()
                .with_fill(tokens.surface)
                .with_border(tokens.border, 1.0)
                .with_radius(8.0),
        )
}

fn section(id: &str, key: &str, color: [u8; 4]) -> UiNode {
    label(id, key, color).with_text_style(UiTextStyle::panel_title(color))
}

fn rule(id: &str, tokens: &UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(1.0, 1.0).with_width_mode(UiSizeMode::Fill))
        .with_style(UiStyle::transparent().with_fill(tokens.border))
}

fn run_button(editor: &NativeElectronicsEditor, tokens: &UiTokens) -> UiNode {
    if editor.lab.dc_requested {
        return action("cancel", "lab.cancel", "cancel", tokens);
    }
    action("run", "lab.run", "simulate", tokens)
        .with_style(
            UiStyle::transparent()
                .with_radius(6.0)
                .with_fill(tokens.surface_raised),
        )
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_class("lab.run-primary")
        .disabled(editor.analysis_running())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_service::AgentService;
    use crate::agent_workspace_layout::WorkspaceLayout;
    use crate::agent_workspace_surface::{build_workspace_surface_with_laboratory, EvidenceTab};
    use crate::electronics_lab_field::{paint, texture_size};
    use raf_core::{EngineSettings, Project, ProjectType};
    use raf_electronics::component::ElectronicComponent;
    use raf_electronics::{PcbLayout, Schematic};
    use raf_render::api_graphic_basic::ui_surface::{CpuUiSurfaceHost, StudioUiPalette};
    use std::time::{Duration, Instant};

    fn circuit() -> Schematic {
        if let Some(path) = std::env::var_os("RAF_LAB_QA_SCHEMATIC") {
            return ron::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        }
        let mut circuit = Schematic::new("Banco de cinco LED");
        let mut source = ElectronicComponent::dc_source(9.0);
        source.position = glam::Vec2::new(-100.0, -60.0);
        let pin_at = raf_electronics::schematic::component_pin_world_position;
        let plus = pin_at(&source, &source.pins[0]);
        let minus = pin_at(&source, &source.pins[1]);
        circuit.add_component(source);
        for branch in 0..5 {
            let y = branch as f32 * 88.0;
            let mut resistor = ElectronicComponent::resistor("1k");
            resistor.position = glam::Vec2::new(100.0, y);
            let mut led = ElectronicComponent::led();
            led.position = glam::Vec2::new(260.0, y);
            let r0 = pin_at(&resistor, &resistor.pins[0]);
            let r1 = pin_at(&resistor, &resistor.pins[1]);
            let a = pin_at(&led, &led.pins[0]);
            let k = pin_at(&led, &led.pins[1]);
            circuit.add_component(resistor);
            circuit.add_component(led);
            circuit.add_wire(plus, r0, "VCC");
            circuit.add_wire(r1, a, &format!("BRANCH_{}", branch + 1));
            circuit.add_wire(k, minus, "RETURN");
        }
        circuit
    }

    #[test]
    fn electronics_lab_renders_signal_map_and_readings_without_agent_dividers() {
        let folder = std::env::temp_dir().join(format!("raf-lab-render-{}", uuid::Uuid::new_v4()));
        let project = Project::create("Banco", ProjectType::Electronics, &folder).unwrap();
        let mut editor =
            NativeElectronicsEditor::from_documents(&project, circuit(), PcbLayout::new("Banco"));
        editor.start_lab_simulation(None, true).unwrap();
        let limit = Instant::now() + Duration::from_secs(4);
        while editor.lab.dc_requested && Instant::now() < limit {
            editor.poll_lab(true);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(editor.lab.run().is_some_and(|run| run.converged));
        assert!(editor.lab.selected.is_some());
        let settings = EngineSettings {
            language: Language::Spanish,
            ..Default::default()
        };
        let mut service = AgentService::default();
        service.request(project.path.clone());
        let limit = Instant::now() + Duration::from_secs(4);
        while service.loading() && Instant::now() < limit {
            service.poll_loads(&settings);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(service.selected_workspace().is_some());
        let project_rows = [crate::studio_surface::HubSurfaceProject {
            name: editor.schematic().name.clone(),
            path: project.path.clone(),
            project_type: ProjectType::Electronics,
            last_opened_label: String::new(),
        }];
        for (index, size) in [[1600, 1000], [960, 640]].into_iter().enumerate() {
            let logical = size.map(|px| px as f32);
            let surface = build_workspace_surface_with_laboratory(
                StudioUiPalette::IndustrialDark,
                &service,
                &project_rows,
                &settings,
                logical,
                0.0,
                0,
                EvidenceTab::Changes,
                true,
                &WorkspaceLayout::default(),
                Some(&editor),
            );
            let mut host = CpuUiSurfaceHost::new(surface, [8, 13, 21, 255]);
            let project_width = WorkspaceLayout::default()
                .resolve(logical, false, false)
                .projects;
            let [width, height] = field_size(
                UiRect::new(
                    project_width,
                    96.0,
                    logical[0] - project_width,
                    logical[1] - 140.0,
                ),
                LabView::Simulation,
            )
            .unwrap();
            let field = Field::new(&editor.lab, width, height);
            assert!(field.net_labels.len() >= 3);
            let texture = texture_size(width, height);
            host.images_mut()
                .insert_rgba(
                    IMAGE_KEY,
                    texture,
                    paint(
                        &field,
                        texture,
                        &crate::agent_workspace_surface::workspace_tokens(
                            StudioUiPalette::IndustrialDark,
                        ),
                    ),
                )
                .unwrap();
            let render = host.render(size, |key| t(key, settings.language));
            if index == 0 {
                if let Some(path) = std::env::var_os("RAF_LAB_QA_CAPTURE") {
                    image::save_buffer(
                        path,
                        render.pixels,
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            let map = host.layout_rect("lab.circuit").unwrap();
            let rail = host.layout_rect("agent.workspace.projects").unwrap();
            assert!(map.x >= rail.x + rail.width);
            assert!(host
                .layout_rect("agent.workspace.divider.projects")
                .is_none());
            assert!(host.layout_rect("lab.run").is_some());
            assert!(host.layout_rect("lab.probe.value").is_some());
            assert!(host.layout_rect("lab.circuit.probe_tag").is_some());
            assert!(host.layout_rect("lab.scope.net.0").is_some());
            assert!(host.layout_rect("lab.compare").unwrap().y >= map.y + map.height);
            assert!(map.x + map.width <= logical[0]);
        }
        drop(service);
        drop(editor);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
