//! RafUI projection of Electronics analysis, serial telemetry and captures.
use crate::agent_workspace_surface::button;
use crate::electronics_controller::NativeElectronicsEditor;
use crate::electronics_lab::LabView;

#[path = "electronics_lab_simulation_surface.rs"]
mod simulation;
use crate::panels::ai_chat::{AgentPanel, AgentReadiness};
use raf_core::{i18n::t, Language};
use raf_hardware::serial_session::{LinkState, Reading, Receipt};
use raf_ui::*;
pub(crate) use simulation::field_size;

pub(crate) fn assistant(
    panel: &AgentPanel,
    settings: &raf_core::EngineSettings,
    rect: UiRect,
    tokens: &UiTokens,
) -> UiNode {
    let busy = panel.runtime.status.blocks_input();
    let ready = panel.readiness(settings) == AgentReadiness::Ready;
    let mut input = UiTextInput::new("agent.input");
    input.placeholder_key = Some("lab.ask_hint".into());
    input.submit_command = Some("agent.submit".into());
    let mut strip = row("lab.assistant")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(10.0, 5.0),
            ..UiLayout::absolute(rect)
        })
        .with_style(UiStyle::transparent().with_fill(tokens.surface))
        .with_child(
            UiNode::text_input("agent.input", input)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(100.0, 30.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_text_value(panel.input_text.clone())
                .with_accessibility_label_key("lab.ask_hint")
                .with_style(
                    UiStyle::transparent()
                        .with_radius(4.0)
                        .with_fill(tokens.surface_raised),
                )
                .disabled(!ready || busy),
        )
        .with_child(
            button("lab.assistant.submit", "lab.ask", "agent.submit", tokens)
                .disabled(!ready || busy || panel.input_text.trim().is_empty()),
        );
    if busy {
        strip = strip.with_child(action(
            "assistant.activity",
            "lab.agent_busy",
            "chat",
            tokens,
        ));
    }
    strip
}

pub(crate) fn tabs(editor: &NativeElectronicsEditor, rect: UiRect, tokens: &UiTokens) -> UiNode {
    let mut bar = row("lab.tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 10.0,
            padding: UiSpacing::xy(16.0, 0.0),
            ..UiLayout::absolute(rect)
        })
        .with_style(UiStyle::transparent().with_fill(tokens.surface));
    for (view, key, verb) in [
        (LabView::Simulation, "lab.simulation", "simulation"),
        (LabView::Device, "lab.device", "device"),
        (LabView::Captures, "lab.captures", "captures"),
        (LabView::Chat, "lab.chat", "chat"),
    ] {
        let active = editor.lab.view == view;
        bar = bar.with_child(
            action(verb, key, verb, tokens)
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(13.0, 7.0),
                    ..UiLayout::fit_content().fixed_height(40.0)
                })
                .with_style(UiStyle::transparent().with_border(
                    if active { tokens.accent_hot } else { [0; 4] },
                    if active { 2.0 } else { 0.0 },
                ))
                .with_visual(UiVisual {
                    border_widths: Some([0.0, 0.0, 2.0, 0.0]),
                    ..Default::default()
                })
                .with_text_style(UiTextStyle::button(if active {
                    tokens.accent_hot
                } else {
                    tokens.text_muted
                }))
                .with_class("lab.nav")
                .with_accessibility_selected(active),
        );
    }
    bar
}

pub(crate) fn body(
    editor: &NativeElectronicsEditor,
    rect: UiRect,
    tokens: &UiTokens,
    language: Language,
) -> UiNode {
    let lab = &editor.lab;
    if lab.view == LabView::Simulation {
        return simulation::body(editor, rect, tokens, language);
    }
    let wide = rect.width >= 730.0;
    let panel_width = if wide { 255.0 } else { 0.0 };
    let center_width = (rect.width - panel_width - 20.0).max(100.0);
    let mut main = column("lab.main");
    let mut toolbar = row("lab.controls");
    let mut detail = column("lab.detail");

    match lab.view {
        LabView::Simulation => unreachable!("simulation has its own layout"),
        LabView::Device => {
            if lab.connected() {
                toolbar = toolbar
                    .with_child(action("identify", "lab.identify", "identify", tokens))
                    .with_child(action("disconnect", "lab.disconnect", "disconnect", tokens));
                if lab
                    .serial
                    .as_ref()
                    .is_some_and(|view| view.state == LinkState::Open)
                    || lab.recording.is_some()
                {
                    toolbar = toolbar.with_child(if lab.recording.is_some() {
                        action("stop", "lab.stop_recording", "stop", tokens)
                    } else {
                        action("record", "lab.record", "record", tokens)
                    });
                }
            } else {
                toolbar = toolbar.with_child(
                    action("scan", "lab.scan", "scan", tokens).disabled(lab.scanning()),
                );
            }
            main = main.with_child(toolbar);
            if let Some(view) = &lab.serial {
                let key = match view.state {
                    LinkState::Opening => "lab.opening",
                    LinkState::Open if view.device.is_some() => "lab.identified",
                    LinkState::Open => "lab.port_open",
                    LinkState::Failed(_) => "lab.link_failed",
                };
                let description = view.device.as_ref().map_or_else(String::new, |device| {
                    format!(" · {} / {}", device.board, device.firmware)
                });
                main = main.with_child(raw(
                    "lab.link",
                    format!(
                        "{} · {}{description}",
                        lab.port.as_deref().unwrap_or_default(),
                        t(key, language)
                    ),
                    tokens.text,
                ));
                if let LinkState::Failed(error) = &view.state {
                    main = main.with_child(raw("lab.link.error", error, tokens.text_muted));
                }
                main = main.with_child(raw(
                    "lab.link.counters",
                    format!(
                        "{} {}   ·   {} {}   ·   {} {}",
                        t("lab.samples", language),
                        view.received,
                        t("lab.invalid", language),
                        view.invalid,
                        t("lab.evicted", language),
                        view.evicted
                    ),
                    tokens.text_muted,
                ));
                if !view.diagnostics.is_empty() {
                    detail = detail.with_child(label(
                        "lab.serial.diagnostics",
                        "lab.diagnostics",
                        tokens.text,
                    ));
                    for (i, line) in view.diagnostics.iter().rev().take(4).enumerate() {
                        detail = detail.with_child(raw(
                            &format!("lab.serial.diagnostic.{i}"),
                            line,
                            tokens.text_muted,
                        ));
                    }
                }
                detail = detail.with_child(label("lab.channel.list", "lab.channels", tokens.text));
                for (i, point) in view.latest.values().enumerate() {
                    detail = detail.with_child(
                        value_button(
                            &format!("channel.{i}"),
                            format!("{}  ·  {:.4} {}", point.channel, point.value, point.unit),
                            &format!("channel:{i}"),
                            tokens,
                        )
                        .with_accessibility_selected(
                            lab.selected_channel.as_deref() == Some(&point.channel),
                        ),
                    );
                }
                if let Some(channel) = lab.selected_channel.as_deref() {
                    if let Some(point) = view.latest.get(channel) {
                        main = main.with_child(raw(
                            "lab.channel.live",
                            format!("{channel}  ·  {:.5} {}", point.value, point.unit),
                            tokens.text,
                        ));
                    }
                    main = main.with_child(plot(&lab.recent, center_width, 132.0, tokens));
                    if let Some(probe) = lab.binding(channel) {
                        if let Some(part) = editor
                            .schematic()
                            .components
                            .iter()
                            .find(|part| part.id == probe.component)
                        {
                            let pin = probe.pin.and_then(|index| part.pins.get(index));
                            let target = pin.map_or_else(
                                || part.designator.clone(),
                                |pin| format!("{}.{}", part.designator, pin.name),
                            );
                            main = main.with_child(raw(
                                "lab.binding",
                                format!("{channel} ↔ {target}"),
                                tokens.text,
                            ));
                            if let Some(dc) = lab
                                .run()
                                .filter(|run| run.fingerprint == editor.lab_fingerprint())
                                .and_then(|run| {
                                    run.values.iter().find(|value| {
                                        value.component == probe.component && value.pin == probe.pin
                                    })
                                })
                            {
                                if let Some(sample) = view
                                    .latest
                                    .get(channel)
                                    .filter(|sample| sample.unit == dc.unit)
                                {
                                    main = main.with_child(raw(
                                        "lab.compare",
                                        format!(
                                            "DC {:.5} {}  ·  Δ {:+.5} {}",
                                            dc.value,
                                            dc.unit,
                                            sample.value - dc.value,
                                            dc.unit
                                        ),
                                        tokens.text_muted,
                                    ));
                                }
                            }
                        }
                    }
                    detail = detail.with_child(
                        row("lab.bind.controls")
                            .with_child(
                                action("bind", "lab.bind", "bind", tokens)
                                    .disabled(lab.selected.is_none()),
                            )
                            .with_child(
                                action("unbind", "lab.unbind", "unbind", tokens)
                                    .disabled(lab.binding(channel).is_none()),
                            ),
                    );
                }
                if let Some(device) = view
                    .device
                    .as_ref()
                    .filter(|_| view.state == LinkState::Open)
                {
                    if !device.outputs.is_empty() {
                        main =
                            main.with_child(label("lab.output.title", "lab.outputs", tokens.text));
                    }
                    if let Some(receipt) = &view.receipt {
                        let (id, key) = match receipt {
                            Receipt::Pending(id) => (id, "lab.ack_pending"),
                            Receipt::Accepted(id) => (id, "lab.ack_ok"),
                            Receipt::Rejected(id) => (id, "lab.ack_rejected"),
                            Receipt::TimedOut(id) => (id, "lab.ack_timeout"),
                        };
                        main = main.with_child(raw(
                            "lab.output.receipt",
                            format!("{} #{id}", t(key, language)),
                            tokens.text_muted,
                        ));
                    }
                    for (i, output) in device.outputs.iter().enumerate() {
                        let id = format!("lab.output-value:{i}");
                        let mut input = UiTextInput::new(&id);
                        input.max_length = 24;
                        input.placeholder_key = Some("lab.value".into());
                        main = main.with_child(
                            row(&format!("lab.output.{i}"))
                                .with_child(raw(
                                    &format!("lab.output.label.{i}"),
                                    format!(
                                        "{}  [{:.3}, {:.3}]",
                                        output.id, output.min, output.max
                                    ),
                                    tokens.text,
                                ))
                                .with_child(
                                    UiNode::text_input(&id, input)
                                        .with_layout(UiLayout::fixed(85.0, 30.0))
                                        .with_text_value(
                                            lab.output_values.get(&i).cloned().unwrap_or_default(),
                                        )
                                        .with_accessibility_label_key("lab.value")
                                        .with_style(
                                            UiStyle::transparent().with_fill(tokens.surface_raised),
                                        ),
                                )
                                .with_child(
                                    action(
                                        &format!("output.apply.{i}"),
                                        "lab.apply",
                                        &format!("output:{i}"),
                                        tokens,
                                    )
                                    .disabled(matches!(view.receipt, Some(Receipt::Pending(_)))),
                                ),
                        );
                    }
                    if device.ota.is_some() {
                        main = main.with_child(
                            action("ota", "lab.ota", "ota", tokens).disabled(lab.preparing_ota()),
                        );
                    }
                    if let Some(ota) = &lab.ota {
                        main = main
                            .with_child(raw(
                                "lab.ota.summary",
                                format!("{} · {} · {} B", ota.filename, ota.target, ota.bytes),
                                tokens.text,
                            ))
                            .with_child(raw(
                                "lab.ota.hash",
                                format!("SHA-256 {}", ota.sha256),
                                tokens.text_muted,
                            ))
                            .with_child(action("ota.save", "lab.ota_save", "ota-save", tokens));
                    }
                    if let Some(path) = &lab.ota_export {
                        main = main.with_child(raw(
                            "lab.ota.path",
                            path.display().to_string(),
                            tokens.text,
                        ));
                    }
                }
            }
            if !lab.connected() {
                if lab.scanned && lab.ports.is_empty() {
                    main =
                        main.with_child(label("lab.port.empty", "lab.no_ports", tokens.text_muted));
                }
                main = main.with_child(label(
                    "lab.device.hint",
                    "lab.device_hint",
                    tokens.text_muted,
                ));
                let mut baud = row("lab.baud");
                for rate in [9_600, 57_600, 115_200, 230_400, 460_800, 921_600] {
                    baud = baud.with_child(
                        value_button(
                            &format!("baud.{rate}"),
                            rate.to_string(),
                            &format!("baud:{rate}"),
                            tokens,
                        )
                        .with_accessibility_selected(lab.baud_rate() == rate),
                    );
                }
                main = main.with_child(baud);
                for (i, port) in lab.ports.iter().enumerate() {
                    main = main.with_child(value_button(
                        &format!("port.{i}"),
                        format!("{}  {}", port.name, port.description),
                        &format!("connect:{i}"),
                        tokens,
                    ));
                }
            }
        }
        LabView::Captures => {
            main = main.with_child(label("lab.replay.title", "lab.replay", tokens.text));
            if let Some(capture) = lab.capture() {
                toolbar = toolbar
                    .with_child(action("play", "lab.play", "play", tokens))
                    .with_child(action("pause", "lab.pause", "pause", tokens));
                for percent in [0, 25, 50, 75, 100] {
                    toolbar = toolbar.with_child(value_button(
                        &format!("seek.{percent}"),
                        format!("{percent}%"),
                        &format!("seek:{percent}"),
                        tokens,
                    ));
                }
                main = main.with_child(toolbar).with_child(raw(
                    "lab.capture.cursor",
                    format!(
                        "{:.3} s  ·  {}  ·  {} {}",
                        lab.replay_ms as f64 / 1000.0,
                        capture.device.as_deref().unwrap_or(&capture.port),
                        capture.points.len(),
                        t("lab.samples", language)
                    ),
                    tokens.text,
                ));
                if capture.lost_prefix {
                    main = main.with_child(label(
                        "lab.capture.truncated",
                        "lab.truncated",
                        tokens.text_muted,
                    ));
                }
                let mut channels = std::collections::BTreeMap::new();
                for (i, point) in capture.points.iter().enumerate() {
                    channels.entry(&point.channel).or_insert(i);
                }
                let active = lab
                    .selected_channel
                    .as_ref()
                    .filter(|key| channels.contains_key(key))
                    .or_else(|| channels.keys().next().copied());
                let end = capture
                    .points
                    .partition_point(|point| point.ms <= lab.replay_ms);
                let points: Vec<_> = capture.points[..end]
                    .iter()
                    .filter(|point| active == Some(&point.channel))
                    .cloned()
                    .collect();
                if let Some(point) = points.last() {
                    main = main.with_child(raw(
                        "lab.capture.value",
                        format!("{} · {:.5} {}", point.channel, point.value, point.unit),
                        tokens.text,
                    ));
                }
                main = main.with_child(plot(&points, center_width, 150.0, tokens));
                for (channel, index) in channels {
                    detail = detail.with_child(value_button(
                        &format!("capture.channel.{index}"),
                        channel.clone(),
                        &format!("capture-channel:{index}"),
                        tokens,
                    ));
                }
            } else {
                main = main.with_child(label(
                    "lab.capture.empty",
                    "lab.no_capture",
                    tokens.text_muted,
                ));
            }
            for (i, capture) in lab.captures().iter().enumerate() {
                detail = detail.with_child(value_button(
                    &format!("capture.{i}"),
                    format!(
                        "{} · {} {}",
                        capture.port,
                        capture.points.len(),
                        t("lab.samples", language)
                    ),
                    &format!("capture:{}", capture.id),
                    tokens,
                ));
            }
        }
        LabView::Chat => {}
    }
    if lab.view != LabView::Captures {
        let mut probe = column("lab.probe");
        if let Some(part) = editor
            .schematic()
            .components
            .iter()
            .find(|part| Some(part.id) == lab.selected)
        {
            probe = probe.with_child(raw(
                "lab.probe.name",
                format!("{} · {}", part.designator, part.value),
                tokens.text,
            ));
            probe = probe.with_child(
                action("branch", "lab.branch", "pin", tokens)
                    .with_accessibility_selected(lab.selected_pin.is_none()),
            );
            for (i, pin) in part.pins.iter().take(12).enumerate() {
                probe = probe.with_child(
                    value_button(
                        &format!("pin.{i}"),
                        pin.name.clone(),
                        &format!("pin:{i}"),
                        tokens,
                    )
                    .with_accessibility_selected(lab.selected_pin == Some(i)),
                );
            }
            if let Some(value) = lab.run().and_then(|run| {
                run.values
                    .iter()
                    .find(|value| value.component == part.id && value.pin == lab.selected_pin)
            }) {
                probe = probe.with_child(raw(
                    "lab.probe.value",
                    format!("{:.6} {}", value.value, value.unit),
                    tokens.text,
                ));
            }
        } else {
            probe = probe.with_child(label(
                "lab.probe.empty",
                "lab.select_part",
                tokens.text_muted,
            ));
        }
        detail.children.insert(0, probe);
        if lab.view == LabView::Device {
            main = main.with_child(
                simulation::signal_map(
                    editor,
                    UiRect::new(0.0, 0.0, center_width, 245.0),
                    tokens,
                    language,
                    false,
                )
                .with_layout(UiLayout::fixed(center_width, 245.0)),
            );
        }
    }
    if let Some(error) = lab.error.as_deref().or(lab.dc_failure.as_deref()) {
        main.children
            .insert(0, raw("lab.error", error, [247, 151, 120, 255]));
    }
    let mut root = UiNode::new("lab.body", UiNodeKind::Panel).with_layout(UiLayout {
        flow: if wide { UiFlow::Row } else { UiFlow::Column },
        gap: 10.0,
        padding: UiSpacing::same(10.0),
        ..UiLayout::absolute(rect)
    });
    if wide {
        main.layout = UiLayout {
            flow: UiFlow::Column,
            gap: 9.0,
            ..UiLayout::fit_content().fixed_width(center_width)
        };
        detail.layout = UiLayout {
            flow: UiFlow::Column,
            gap: 9.0,
            padding: UiSpacing::same(12.0),
            ..UiLayout::fit_content().fixed_width(panel_width - 10.0)
        };
        detail.style = UiStyle::transparent()
            .with_radius(8.0)
            .with_fill(tokens.surface);
    }
    if wide {
        root = root
            .with_child(scroll(
                "lab.main.scroll",
                main,
                center_width,
                rect.height - 20.0,
            ))
            .with_child(scroll(
                "lab.detail.scroll",
                detail,
                panel_width - 10.0,
                rect.height - 20.0,
            ));
    } else {
        main = main.with_child(detail);
        root = root.with_child(scroll(
            "lab.main.scroll",
            main,
            center_width,
            rect.height - 20.0,
        ));
    }
    root
}

fn plot(points: &[Reading], width: f32, height: f32, tokens: &UiTokens) -> UiNode {
    let mut graph = UiNode::new("lab.plot", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(width, height))
        .with_style(
            UiStyle::transparent()
                .with_fill(tokens.surface)
                .with_radius(8.0)
                .with_border(tokens.border, 1.0),
        );
    let Some(first) = points.first() else {
        return graph.with_child(
            label("lab.plot.empty", "lab.await_samples", tokens.text_muted).with_layout(
                UiLayout::absolute(UiRect::new(12.0, 14.0, width - 24.0, 20.0)),
            ),
        );
    };
    let duration = points.last().unwrap().ms.saturating_sub(first.ms).max(1);
    let (min, max) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), point| {
            (min.min(point.value), max.max(point.value))
        });
    let mut bins = [None::<(f64, f64)>; 128];
    for point in points {
        let i =
            (point.ms.saturating_sub(first.ms).saturating_mul(127) / duration).min(127) as usize;
        let (low, high) = bins[i].unwrap_or((point.value, point.value));
        bins[i] = Some((low.min(point.value), high.max(point.value)));
    }
    let y = |value: f64| {
        if max == min {
            height / 2.0
        } else {
            let scale = max.abs().max(min.abs()).max(f64::MIN_POSITIVE);
            24.0 + (1.0 - (value / scale - min / scale) / (max / scale - min / scale)) as f32
                * (height - 50.0)
        }
    };
    for (i, values) in bins.into_iter().enumerate() {
        let Some((low, high)) = values else { continue };
        let x = 12.0 + (width - 24.0) * i as f32 / 127.0;
        graph = graph.with_child(segment(
            &format!("lab.sample.{i}"),
            [x, y(low)],
            [x, y(high)],
            tokens.text,
        ));
    }
    graph
        .with_child(
            raw(
                "lab.plot.range",
                format!("{min:.4} … {max:.4} {}", first.unit),
                tokens.text_muted,
            )
            .with_layout(UiLayout::absolute(UiRect::new(
                12.0,
                5.0,
                width - 24.0,
                18.0,
            ))),
        )
        .with_child(
            raw(
                "lab.plot.time",
                format!("Δt {:.3} s", duration as f64 / 1000.0),
                tokens.text_muted,
            )
            .with_layout(UiLayout::absolute(UiRect::new(
                12.0,
                height - 23.0,
                width - 24.0,
                18.0,
            ))),
        )
}

fn segment(id: &str, a: [f32; 2], b: [f32; 2], color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_style(UiStyle::transparent().with_fill(color))
        .with_layout(UiLayout::absolute(UiRect::new(
            a[0].min(b[0]),
            a[1].min(b[1]),
            (a[0] - b[0]).abs().max(2.0),
            (a[1] - b[1]).abs().max(2.0),
        )))
}
fn row(id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        ..UiLayout::fit_content()
    })
}
fn column(id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 7.0,
        ..UiLayout::fit_content()
    })
}
fn scroll(id: &str, child: UiNode, width: f32, height: f32) -> UiNode {
    UiNode::scroll_view(id, UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fixed(width.max(1.0), height.max(1.0))
        })
        .with_child(child)
}
fn label(id: &str, key: &str, color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fit_content())
}
fn raw(id: &str, value: impl Into<String>, color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_value(value)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fit_content())
}
fn action(id: &str, key: &str, command: &str, tokens: &UiTokens) -> UiNode {
    button(
        &format!("lab.{id}"),
        key,
        &format!("agent.lab.{command}"),
        tokens,
    )
}
fn value_button(id: &str, value: String, command: &str, tokens: &UiTokens) -> UiNode {
    let mut node = action(id, "lab.select", command, tokens).with_text_value(value);
    node.text_key = None;
    node.accessibility_label_key = None;
    node
}
