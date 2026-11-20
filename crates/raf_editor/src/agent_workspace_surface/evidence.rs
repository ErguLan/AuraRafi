//! Evidence tabs, gallery, comparison and read-only 3D preview.

use super::*;
use super::layout::label;
use super::tools::{world_panel, WorldPanelModel};

pub(super) fn evidence_surface(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    language: Language,
    rect: UiRect,
    tab: EvidenceTab,
    game: bool,
    project: Option<&raf_core::project::Project>,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new("agent.workspace.evidence", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 12.0,
            padding: UiSpacing::same(12.0),
            ..UiLayout::absolute(rect).with_z_index(30)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        });
    let mut tabs =
        UiNode::new("agent.workspace.evidence.tabs", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        });
    for (id, key, value) in [
        ("changes", "agent_workspace.changes", EvidenceTab::Changes),
        ("world", "agent_workspace.world", EvidenceTab::World),
        (
            "artifacts",
            "agent_workspace.artifacts",
            EvidenceTab::Artifacts,
        ),
        (
            "verification",
            "agent_workspace.verification",
            EvidenceTab::Verification,
        ),
        (
            "preview",
            "agent_workspace.preview_3d",
            EvidenceTab::Preview,
        ),
    ] {
        let mut item = button(
            &format!("agent.workspace.tab.{id}"),
            key,
            &format!("agent.workspace.tab.{id}"),
            &tokens,
        )
        .with_accessibility_selected(tab == value);
        item.layout = UiLayout {
            padding: UiSpacing::xy(4.0, 6.0),
            ..UiLayout::fixed(
                match value {
                    EvidenceTab::Changes => 66.0,
                    EvidenceTab::World => 56.0,
                    EvidenceTab::Artifacts => 74.0,
                    EvidenceTab::Verification => 94.0,
                    EvidenceTab::Preview => 34.0,
                },
                32.0,
            )
        };
        if value == EvidenceTab::Preview {
            item.text_key = None;
            item.icon = Some(UiIcon::new(UiIconId::ScenePreview).with_tint(tokens.text));
            item.tooltip_key = Some(key.into());
        }
        item.style.fill = if tab == value {
            tokens.surface_raised
        } else {
            [0; 4]
        };
        tabs = tabs.with_child(item);
    }
    root = root.with_child(tabs);
    if tab == EvidenceTab::World {
        let Some(project) = project.filter(|_| game) else {
            return root.with_child(
                label(
                    "agent.workspace.world.unavailable",
                    "agent_workspace.world_game_only",
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        };
        let model = WorldPanelModel::load(project, panel);
        return root.with_child(world_panel(palette, language, &model));
    }
    if tab == EvidenceTab::Preview {
        if !game {
            return root.with_child(
                label(
                    "agent.workspace.preview.unavailable",
                    "agent_workspace.preview_game_only",
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
        root.style.fill = [0; 4];
        root.layout.padding = UiSpacing::ZERO;
        root.children[0].layout = UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::absolute(UiRect::new(12.0, 12.0, rect.width - 24.0, 34.0))
        };
        for (id, area) in [
            ("top", UiRect::new(0.0, 0.0, rect.width, 86.0)),
            (
                "bottom",
                UiRect::new(0.0, rect.height - 46.0, rect.width, 46.0),
            ),
            ("left", UiRect::new(0.0, 0.0, 8.0, rect.height)),
            (
                "right",
                UiRect::new(rect.width - 8.0, 0.0, 8.0, rect.height),
            ),
        ] {
            root.children.insert(
                0,
                UiNode::new(
                    format!("agent.workspace.preview.background.{id}"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout::absolute(area))
                .with_style(UiStyle {
                    fill: tokens.surface,
                    ..UiStyle::transparent()
                }),
            );
        }
        let icon_button = |id: &str, key: &str, command: &str, icon| {
            let mut node = button(id, key, command, &tokens)
                .with_icon(UiIcon::new(icon).with_tint(tokens.text))
                .with_tooltip_key(key);
            node.text_key = None;
            node.layout = UiLayout::fixed(30.0, 28.0);
            node
        };
        return root
            .with_child(
                UiNode::new("agent.workspace.preview.controls", UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 8.0,
                        align_items: UiAlign::Center,
                        ..UiLayout::absolute(UiRect::new(12.0, 50.0, rect.width - 24.0, 28.0))
                    })
                    .with_child(
                        label(
                            "agent.workspace.preview.readonly",
                            "agent_workspace.preview_readonly",
                            &tokens,
                        )
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        }),
                    )
                    .with_child(icon_button(
                        "agent.workspace.preview.fit",
                        "agent_workspace.preview_fit",
                        "agent.workspace.preview.fit",
                        UiIconId::Focus,
                    ))
                    .with_child(icon_button(
                        "agent.workspace.preview.close",
                        "agent_workspace.preview_close",
                        "agent.workspace.preview.close",
                        UiIconId::Close,
                    )),
            )
            .with_child(
                UiNode::new("agent.workspace.preview.canvas", UiNodeKind::Canvas)
                    .with_layout(UiLayout::absolute(UiRect::new(
                        8.0,
                        86.0,
                        rect.width - 16.0,
                        (rect.height - 132.0).max(1.0),
                    )))
                    .with_style(UiStyle::transparent())
                    .with_accessibility_label_key("agent_workspace.preview_3d"),
            )
            .with_child(
                label(
                    "agent.workspace.preview.help",
                    "agent_workspace.preview_controls",
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::absolute(UiRect::new(
                    12.0,
                    rect.height - 40.0,
                    rect.width - 24.0,
                    36.0,
                ))),
            );
    }
    if tab == EvidenceTab::Artifacts {
        return root.with_child(evidence_gallery(palette, panel, language));
    }
    let mut list = UiNode::scroll_view("agent.workspace.evidence.scroll", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 12.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    let mut count = 0;
    for message in panel
        .runtime
        .messages
        .iter()
        .rev()
        .filter(|m| m.role == MessageRole::Tool)
        .take(24)
    {
        let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
            continue;
        };
        let relevant = match tab {
            EvidenceTab::Changes => result.diff.is_some() || result.changed,
            EvidenceTab::World => false,
            EvidenceTab::Artifacts => {
                result.data.get("artifact").is_some()
                    || result
                        .data
                        .pointer("/receipt/artifacts")
                        .and_then(|v| v.as_array())
                        .is_some_and(|artifacts| !artifacts.is_empty())
                    || result
                        .references
                        .iter()
                        .any(|r| r.contains("generated") || r.contains("agent_artifacts"))
            }
            EvidenceTab::Verification => result.verification.is_some(),
            EvidenceTab::Preview => false,
        };
        if !relevant {
            continue;
        }
        count += 1;
        let id = format!("agent.workspace.evidence.{}", message.id);
        let mut entry = UiNode::new(&id, UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 6.0,
                padding: UiSpacing::same(10.0),
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                radius: 8.0,
                ..UiStyle::transparent()
            })
            .with_child(
                UiNode::new(format!("{id}.summary"), UiNodeKind::Label)
                    .with_text_value(result.summary)
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .with_text_style(UiTextStyle::body(tokens.text))
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        if let Some(revision) = result.revision {
            entry = entry.with_child(
                UiNode::new(format!("{id}.revision"), UiNodeKind::Label)
                    .with_text_value(format!(
                        "{} {revision}",
                        t("agent_workspace.revision", language)
                    ))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            );
        }
        if result
            .data
            .pointer("/persistence/saved")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            entry = entry.with_child(label(
                &format!("{id}.saved"),
                "agent_workspace.autosave",
                &tokens,
            ));
        }
        let detail = match tab {
            EvidenceTab::Changes => result.diff.as_ref(),
            EvidenceTab::World => None,
            EvidenceTab::Artifacts => result
                .data
                .get("artifact")
                .or_else(|| result.data.pointer("/receipt/artifacts")),
            EvidenceTab::Verification => result.verification.as_ref(),
            EvidenceTab::Preview => None,
        };
        if let Some(detail) = detail {
            entry = entry.with_child(button(
                &format!("{id}.expand"),
                "agent_workspace.details",
                &format!("agent.tool.toggle:{}", message.id),
                &tokens,
            ));
            if panel.expanded_tools.contains(&message.id.to_string()) {
                let text = serde_json::to_string_pretty(detail)
                    .unwrap_or_default()
                    .chars()
                    .take(4096)
                    .collect::<String>();
                entry = entry.with_child(
                    UiNode::new(format!("{id}.detail"), UiNodeKind::Label)
                        .with_text_value(text)
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .selectable_text()
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
                );
            }
        }
        for (index, reference) in result.references.iter().take(12).enumerate() {
            entry = entry.with_child(
                UiNode::new(format!("{id}.reference.{index}"), UiNodeKind::Label)
                    .with_text_value(reference.clone())
                    .selectable_text()
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
        if let Some(verification) = result.verification {
            let status = verification
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let key = match status {
                "passed" | "Passed" => "agent_workspace.verified",
                "failed" | "Failed" => "agent_workspace.validation_failed",
                "preview" => "agent_workspace.preview",
                _ => "agent_workspace.unverified",
            };
            entry = entry.with_child(label(&format!("{id}.verification"), key, &tokens));
        }
        list = list.with_child(entry);
    }
    if count == 0 {
        list = list.with_child(
            label(
                "agent.workspace.evidence.empty",
                "agent_workspace.no_evidence",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    root.with_child(list)
}

fn evidence_image_node(
    tokens: &UiTokens,
    artifact: &EvidenceArtifact,
    id: &str,
    height: f32,
) -> UiNode {
    let mut image = UiNode::new(id, UiNodeKind::Image)
        .with_layout(UiLayout {
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, height],
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            radius: 6.0,
            ..UiStyle::transparent()
        });
    raf_ui::components::set_thumbnail_image(
        &mut image,
        evidence_image_key(&artifact.uri),
        raf_ui::UiImageFit::Cover,
        6.0,
    );
    image
}

fn evidence_card(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    artifact: &EvidenceArtifact,
    _index: usize,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let selected_index = panel
        .evidence_selected
        .iter()
        .position(|uri| uri == &artifact.uri);
    let selected = selected_index.is_some();
    let card_id = format!(
        "agent.workspace.evidence.card.{}",
        evidence_image_key(&artifact.uri).replace('.', "_")
    );
    let mut card = UiNode::new(&card_id, UiNodeKind::Button)
        .with_class("agent-evidence-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(7.0),
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, 188.0],
            grow: 1.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: if selected {
                tokens.surface_raised
            } else {
                tokens.surface_alt
            },
            border: if selected {
                tokens.accent_hot
            } else {
                tokens.border
            },
            border_width: if selected { 2.0 } else { 1.0 },
            radius: 8.0,
            ..UiStyle::transparent()
        })
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(artifact.kind.label_key())
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("agent.evidence.select:{}", artifact.uri),
        ))
        .with_child(evidence_image_node(
            &tokens,
            artifact,
            &format!("{card_id}.image"),
            104.0,
        ));
    let title = UiNode::new(format!("{card_id}.label"), UiNodeKind::Label)
        .with_text_value(artifact.label.clone())
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout {
            width_mode: UiSizeMode::Fill,
            ..UiLayout::fit_content()
        });
    card = card.with_child(title);
    if let Some(position) = selected_index {
        card = card.with_child(
            UiNode::new(format!("{card_id}.selected"), UiNodeKind::Label)
                .with_text_value(format!(
                    "{} {}",
                    position + 1,
                    t("agent_workspace.evidence_selected", panel.language)
                ))
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fit_content()),
        );
    }
    if panel.evidence_pinned.as_deref() == Some(artifact.uri.as_str()) {
        card = card.with_child(
            UiNode::new(format!("{card_id}.pinned"), UiNodeKind::Label)
                .with_text_key("agent_workspace.evidence_pinned")
                .with_text_style(UiTextStyle::button(tokens.positive))
                .with_layout(UiLayout::fit_content()),
        );
    }
    card = card.with_child(
        UiNode::new(format!("{card_id}.kind"), UiNodeKind::Label)
            .with_text_value(format!(
                "{} · {}",
                t(artifact.kind.label_key(), panel.language),
                artifact
                    .compare_lines
                    .first()
                    .cloned()
                    .unwrap_or_default()
            ))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout {
                width_mode: UiSizeMode::Fill,
                ..UiLayout::fit_content()
            }),
    );
    card.layout.basis[0] = 0.0;
    card
}

fn evidence_metadata(
    palette: StudioUiPalette,
    language: Language,
    artifact: &EvidenceArtifact,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut block = UiNode::new(
        format!("agent.workspace.evidence.meta.{}", evidence_image_key(&artifact.uri)),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 2.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    block = block.with_child(
        UiNode::new(
            format!("agent.workspace.evidence.meta.label.{}", evidence_image_key(&artifact.uri)),
            UiNodeKind::Label,
        )
        .with_text_value(artifact.label.clone())
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    );
    block = block.with_child(
        UiNode::new(
            format!("agent.workspace.evidence.meta.kind.{}", evidence_image_key(&artifact.uri)),
            UiNodeKind::Label,
        )
        .with_text_value(t(artifact.kind.label_key(), language))
        .with_text_style(UiTextStyle::body(tokens.accent_hot))
        .with_layout(UiLayout::fit_content()),
    );
    for (index, line) in artifact.compare_lines.iter().take(3).enumerate() {
        block = block.with_child(
            UiNode::new(
                format!(
                    "agent.workspace.evidence.meta.line.{}.{}",
                    evidence_image_key(&artifact.uri),
                    index
                ),
                UiNodeKind::Label,
            )
            .with_text_value(line.clone())
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    block
}

fn evidence_detail_panel(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    artifact: &EvidenceArtifact,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let key = evidence_image_key(&artifact.uri);
    let pinned = panel.evidence_pinned.as_deref() == Some(artifact.uri.as_str());
    let pin_label = if pinned {
        "agent_workspace.evidence_unpin"
    } else {
        "agent_workspace.evidence_pin"
    };
    UiNode::new(
        format!("agent.workspace.evidence.detail.{key}"),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        padding: UiSpacing::same(8.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_style(UiStyle {
        fill: tokens.surface_alt,
        border: tokens.border,
        border_width: 1.0,
        radius: 8.0,
        ..UiStyle::transparent()
    })
    .with_child(
        UiNode::new(
            format!("agent.workspace.evidence.detail.title.{key}"),
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_detail")
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_layout(UiLayout::fit_content()),
    )
    .with_child(evidence_image_node(
        &tokens,
        artifact,
        &format!("agent.workspace.evidence.detail.image.{key}"),
        300.0,
    ))
    .with_child(evidence_metadata(palette, panel.language, artifact))
    .with_child(
        UiNode::new(
            format!("agent.workspace.evidence.detail.summary.{key}"),
            UiNodeKind::Label,
        )
        .with_text_value(artifact.summary.clone())
        .with_text_style(UiTextStyle::body(tokens.text))
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    )
    .with_child(button(
        &format!("agent.workspace.evidence.detail.pin.{key}"),
        pin_label,
        &format!("agent.evidence.pin:{}", artifact.uri),
        &tokens,
    ))
}

fn evidence_compare_panel(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    selected: &[&EvidenceArtifact],
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new(
        "agent.workspace.evidence.compare",
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        padding: UiSpacing::same(8.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_style(UiStyle {
        fill: tokens.surface_alt,
        border: tokens.accent_hot,
        border_width: 1.0,
        radius: 8.0,
        ..UiStyle::transparent()
    })
    .with_child(
        UiNode::new(
            "agent.workspace.evidence.compare.title",
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_compare")
        .with_text_style(UiTextStyle::button(tokens.accent_hot))
        .with_layout(UiLayout::fit_content()),
    );
    let mut row = UiNode::new(
        "agent.workspace.evidence.compare.row",
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 8.0,
        align_items: UiAlign::Start,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    for artifact in selected.iter().take(2) {
        let key = evidence_image_key(&artifact.uri);
        let mut column = UiNode::new(
            format!("agent.workspace.evidence.compare.column.{key}"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            grow: 1.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(evidence_image_node(
            &tokens,
            artifact,
            &format!("agent.workspace.evidence.compare.image.{key}"),
            150.0,
        ))
        .with_child(evidence_metadata(palette, panel.language, artifact));
        column = column.with_child(
            UiNode::new(
                format!("agent.workspace.evidence.compare.summary.{key}"),
                UiNodeKind::Label,
            )
            .with_text_value(artifact.summary.clone())
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
        row = row.with_child(column);
    }
    root.with_child(row).with_child(
        UiNode::new(
            "agent.workspace.evidence.compare.hint",
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_compare_hint")
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
    )
}

pub(super) fn evidence_gallery(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    language: Language,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let artifacts = collect_evidence_artifacts(panel);
    let selected = panel
        .evidence_selected
        .iter()
        .filter_map(|uri| artifacts.iter().find(|artifact| &artifact.uri == uri))
        .collect::<Vec<_>>();
    let mut root = UiNode::new(
        "agent.workspace.evidence.gallery",
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 8.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });
    let mut header = UiNode::new(
        "agent.workspace.evidence.gallery.header",
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            "agent.workspace.evidence.gallery.title",
            UiNodeKind::Label,
        )
        .with_text_key("agent_workspace.evidence_gallery")
        .with_text_style(UiTextStyle::panel_title(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            width_mode: UiSizeMode::Fill,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new(
            "agent.workspace.evidence.gallery.count",
            UiNodeKind::Label,
        )
        .with_text_value(format!("{}", artifacts.len()))
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_layout(UiLayout::fit_content()),
    );
    if !panel.evidence_selected.is_empty() {
        header = header.with_child(button(
            "agent.workspace.evidence.clear",
            "agent_workspace.evidence_clear",
            "agent.evidence.clear",
            &tokens,
        ));
    }
    header = header.with_child(button(
        "agent.workspace.evidence.compare.toggle",
        "agent_workspace.evidence_compare",
        "agent.evidence.compare",
        &tokens,
    ));
    root = root.with_child(header);
    if artifacts.is_empty() {
        return root.with_child(
            label(
                "agent.workspace.evidence.empty",
                "agent_workspace.no_evidence",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    if selected.len() == 2 && panel.evidence_compare {
        let pair: Vec<&EvidenceArtifact> = selected.iter().copied().take(2).collect();
        root = root.with_child(evidence_compare_panel(palette, panel, &pair));
    } else if let Some(first) = selected.first() {
        root = root.with_child(evidence_detail_panel(palette, panel, first));
    } else {
        root = root.with_child(
            label(
                "agent.workspace.evidence.select.hint",
                "agent_workspace.evidence_select_hint",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    let mut scroll = UiNode::scroll_view(
        "agent.workspace.evidence.gallery.scroll",
        UiScrollAxis::Vertical,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 8.0,
        grow: 1.0,
        ..UiLayout::fill(UiFlow::Column)
    });
    for (row, chunk) in artifacts.chunks(2).enumerate() {
        let mut line = UiNode::new(
            format!("agent.workspace.evidence.gallery.row.{row}"),
            UiNodeKind::Toolbar,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 8.0,
            align_items: UiAlign::Start,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
        for (column, artifact) in chunk.iter().enumerate() {
            line = line.with_child(evidence_card(
                palette,
                panel,
                artifact,
                row * 2 + column,
            ));
        }
        if chunk.len() == 1 {
            line = line.with_child(
                UiNode::new(
                    format!("agent.workspace.evidence.gallery.spacer.{row}"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
            );
        }
        scroll = scroll.with_child(line);
    }
    root.with_child(scroll)
}
