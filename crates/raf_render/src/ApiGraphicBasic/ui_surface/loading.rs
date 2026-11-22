//! Shared startup and runtime loading document. Geometry and branding have one owner.
use super::{DirectUiSurfaceHost, StudioUiPalette, UiSurface};
use raf_core::{config::Language, i18n::t};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiContentAlign, UiContentLayout, UiEventBinding, UiEventKind,
    UiFlow, UiFontWeight, UiImage, UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextOverflow, UiTextRole, UiTextStyle, UiVisual,
};

pub fn insert_loading_brand(host: &mut DirectUiSurfaceHost) {
    if let Ok(decoded) = image::load_from_memory(include_bytes!("../../../../../editor/icon.png")) {
        let decoded = decoded.to_rgba8();
        let _ = host.images_mut().insert_rgba(
            "loading.brand-mark",
            [decoded.width(), decoded.height()],
            decoded.into_raw(),
        );
    }
}
pub fn build_loading_surface(
    palette: StudioUiPalette,
    progress: f32,
    language: Language,
    loading_message: &str,
    status: &str,
    progress_label: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let progress = progress.clamp(0.0, 1.0);
    let status_width = if language == Language::Spanish {
        250.0
    } else {
        178.0
    };
    let track = UiNode::new("loading.progress.track", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(380.0, 6.0))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 0.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("loading.progress.fill", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(380.0 * progress, 6.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 3.0,
                    opacity: 1.0,
                }),
        );
    let card = UiNode::new("loading.card", UiNodeKind::Panel)
        .with_class("loading-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(36.0, 20.0),
            ..UiLayout::fixed(620.0, 500.0)
        })
        .with_child(
            UiNode::image(
                "loading.brand-mark",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new("loading.brand-mark"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(200.0, 200.0)),
        )
        .with_child(
            UiNode::new("loading.brand", UiNodeKind::Label)
                .with_text_key("RAFI")
                .with_layout(UiLayout::fixed(150.0, 80.0))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Brand,
                    size_px: 72.0,
                    line_height_px: 80.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(
            UiNode::new("loading.message.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::Center,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(548.0, 48.0)
                })
                .with_child(
                    UiNode::new("loading.message", UiNodeKind::Label)
                        .with_text_value(loading_message)
                        .with_layout(UiLayout {
                            width_mode: UiSizeMode::FitContent,
                            height_mode: UiSizeMode::FitContent,
                            max_size: [540.0, 48.0],
                            ..UiLayout::default()
                        })
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 19.0,
                            line_height_px: 0.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text,
                            inherit_color: false,
                        }),
                ),
        )
        .with_child(
            UiNode::new("loading.signature", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(36.0, 2.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 1.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("loading.status.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::Center,
                    ..UiLayout::fixed(0.0, 24.0)
                })
                .with_child(
                    UiNode::new("loading.status", UiNodeKind::Label)
                        .with_text_value(status)
                        .with_layout(UiLayout::fixed(status_width, 24.0))
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 18.0,
                            line_height_px: 24.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text_muted,
                            inherit_color: false,
                        }),
                ),
        )
        .with_child(
            UiNode::new("loading.progress.row", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 12.0,
                    ..UiLayout::fixed(440.0, 20.0)
                })
                .with_child(track)
                .with_child(
                    UiNode::new("loading.progress.label", UiNodeKind::Label)
                        .with_text_value(progress_label)
                        .with_layout(UiLayout::fixed(48.0, 20.0))
                        .with_text_style(UiTextStyle::body(tokens.accent)),
                ),
        )
        .with_child(
            UiNode::new("loading.engine", UiNodeKind::Label)
                .with_text_value(t("app.engine", language))
                .with_layout(UiLayout::fixed(56.0, 20.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("loading.version", UiNodeKind::Label)
                .with_text_value(format!("v{}", env!("CARGO_PKG_VERSION")))
                .with_layout(UiLayout::fixed(56.0, 20.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    let root = UiNode::new("loading.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(card);
    let mut surface = UiSurface::new("loading", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![UiStyleRule::new(
            UiStyleSelector::Class("loading-card".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(10.0),
                ..UiStylePatch::default()
            },
        )],
    };
    surface
}

/// Presentation facts for the runtime Play loading card.
///
/// The editor startup card keeps [`build_loading_surface`]; this view is only
/// used by the player and adds stage history, elapsed time, slow and failed
/// states without changing the startup contract.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeLoadingView<'a> {
    pub palette: StudioUiPalette,
    pub progress: f32,
    pub language: Language,
    /// Current asset path, or the cancel hint when no asset is being compiled.
    pub detail: &'a str,
    /// Localized current stage.
    pub status: &'a str,
    /// `completed/total` for script compilation.
    pub count: &'a str,
    pub elapsed_secs: u64,
    /// Recent stage labels, oldest first. At most three are rendered.
    pub history: &'a [String],
    pub slow: bool,
    pub failed: bool,
    pub error: Option<&'a str>,
    pub can_retry: bool,
}

/// Builds the runtime loading card with the default retry availability.
pub fn build_runtime_loading_surface(
    palette: StudioUiPalette,
    progress: f32,
    language: Language,
    detail: &str,
    status: &str,
    count: &str,
    elapsed_secs: u64,
    history: &[String],
    slow: bool,
    failed: bool,
    error: Option<&str>,
) -> UiSurface {
    build_runtime_loading_surface_with_options(RuntimeLoadingView {
        palette,
        progress,
        language,
        detail,
        status,
        count,
        elapsed_secs,
        history,
        slow,
        failed,
        error,
        can_retry: true,
    })
}

pub fn build_runtime_loading_surface_with_options(view: RuntimeLoadingView<'_>) -> UiSurface {
    let RuntimeLoadingView {
        palette,
        progress,
        language,
        detail,
        status,
        count,
        elapsed_secs,
        history,
        slow,
        failed,
        error,
        can_retry,
    } = view;
    let tokens = palette.tokens();
    let progress = progress.clamp(0.0, 1.0);
    let failed_message = t("runtime.loading.failed", language);
    let headline = if failed {
        failed_message.as_str()
    } else if status.is_empty() {
        detail
    } else {
        status
    };
    let fallback_error = t("runtime.loading.failed", language);
    let error_text = error.unwrap_or(fallback_error.as_str());

    let track = UiNode::new("loading.progress.track", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(380.0, 6.0))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 0.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("loading.progress.fill", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(380.0 * progress, 6.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 3.0,
                    opacity: 1.0,
                }),
        );

    let mut details = UiNode::new("loading.details", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::xy(10.0, 5.0),
            ..UiLayout::fixed(548.0, 92.0)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            border_width: 1.0,
            radius: 6.0,
            ..UiStyle::transparent()
        })
        .with_child(
            UiNode::new("loading.details.recent", UiNodeKind::Label)
                .with_text_key("runtime.loading.recent")
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill))
                .with_text_style(UiTextStyle::label(tokens.text_muted)),
        );
    for (index, stage) in history
        .iter()
        .skip(history.len().saturating_sub(3))
        .enumerate()
    {
        details = details.with_child(
            UiNode::new(format!("loading.history.{index}"), UiNodeKind::Label)
                .with_text_value(stage)
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::label(tokens.text_muted)),
        );
    }
    details = details.with_child(
        UiNode::new("loading.details.elapsed", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                ..UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("loading.details.elapsed.label", UiNodeKind::Label)
                    .with_text_key("runtime.loading.elapsed")
                    .with_layout(UiLayout::fixed(110.0, 16.0))
                    .with_text_style(UiTextStyle::label(tokens.text_muted)),
            )
            .with_child(
                UiNode::new("loading.details.elapsed.value", UiNodeKind::Label)
                    .with_text_value(format!("{elapsed_secs} s"))
                    .with_layout(
                        UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::FitContent),
                    )
                    .with_text_style(UiTextStyle::label(tokens.text)),
            ),
    );

    let mut card = UiNode::new("loading.card", UiNodeKind::Panel)
        .with_class("loading-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(36.0, 18.0),
            ..UiLayout::fixed(620.0, 500.0)
        })
        .with_child(
            UiNode::image(
                "loading.brand-mark",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new("loading.brand-mark"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(112.0, 112.0)),
        )
        .with_child(
            UiNode::new("loading.brand", UiNodeKind::Label)
                .with_text_key("RAFI")
                .with_layout(UiLayout::fixed(150.0, 50.0))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Brand,
                    size_px: 52.0,
                    line_height_px: 50.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(
            UiNode::new("loading.message.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::Center,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(548.0, 44.0)
                })
                .with_child(
                    UiNode::new("loading.message", UiNodeKind::Label)
                        .with_text_value(headline)
                        .with_layout(UiLayout {
                            width_mode: UiSizeMode::FitContent,
                            height_mode: UiSizeMode::FitContent,
                            max_size: [540.0, 44.0],
                            ..UiLayout::default()
                        })
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 19.0,
                            line_height_px: 22.0,
                            weight: UiFontWeight::Regular,
                            color: if failed { tokens.danger } else { tokens.text },
                            inherit_color: false,
                        }),
                ),
        )
        .with_child(
            UiNode::new("loading.signature", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(36.0, 2.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 1.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("loading.detail", UiNodeKind::Label)
                .with_text_value(detail)
                .with_layout(UiLayout::fixed(548.0, 18.0))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );

    if failed {
        card = card
            .with_child(
                UiNode::new("loading.error", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        padding: UiSpacing::xy(10.0, 5.0),
                        ..UiLayout::fixed(548.0, 58.0)
                    })
                    .with_style(UiStyle {
                        fill: tokens.surface_raised,
                        border: tokens.danger,
                        border_width: 1.0,
                        radius: 6.0,
                        ..UiStyle::transparent()
                    })
                    .with_child(
                        UiNode::new("loading.error.text", UiNodeKind::Label)
                            .with_text_value(error_text)
                            .selectable_text()
                            .with_layout(
                                UiLayout::fixed(0.0, 46.0).with_width_mode(UiSizeMode::Fill),
                            )
                            .with_text_overflow(UiTextOverflow::Wrap)
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::Body,
                                size_px: 12.0,
                                line_height_px: 15.0,
                                weight: UiFontWeight::Regular,
                                color: tokens.danger,
                                inherit_color: false,
                            }),
                    ),
            )
            .with_child({
                let mut actions = UiNode::new("loading.actions", UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        justify_content: UiJustify::End,
                        align_items: UiAlign::Center,
                        gap: 8.0,
                        ..UiLayout::fixed(548.0, 32.0)
                    });
                if can_retry {
                    actions = actions.with_child(runtime_loading_button(
                        "loading.action.retry",
                        "runtime.loading.retry",
                        "runtime.retry",
                        palette,
                        true,
                        112.0,
                        28.0,
                    ));
                }
                actions.with_child(runtime_loading_button(
                    "loading.action.cancel",
                    "runtime.loading.cancel",
                    "runtime.cancel",
                    palette,
                    false,
                    112.0,
                    28.0,
                ))
            });
    } else {
        card = card.with_child(
            UiNode::new("loading.progress.row", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 12.0,
                    ..UiLayout::fixed(440.0, 20.0)
                })
                .with_child(track)
                .with_child(
                    UiNode::new("loading.progress.label", UiNodeKind::Label)
                        .with_text_value(count)
                        .with_layout(UiLayout::fixed(48.0, 20.0))
                        .with_text_style(UiTextStyle::body(tokens.accent)),
                ),
        );
    }

    card = card.with_child(details);

    if slow && !failed {
        card = card.with_child(
            UiNode::new("loading.slow", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::End,
                    gap: 12.0,
                    padding: UiSpacing::xy(8.0, 0.0),
                    ..UiLayout::fixed(548.0, 34.0)
                })
                .with_style(UiStyle {
                    fill: tokens.surface_raised,
                    border: tokens.warning,
                    border_width: 1.0,
                    radius: 6.0,
                    ..UiStyle::transparent()
                })
                .with_child(
                    UiNode::new("loading.slow.message", UiNodeKind::Label)
                        .with_text_key("runtime.loading.slow")
                        .with_layout(
                            UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill),
                        )
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 12.0,
                            line_height_px: 15.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.warning,
                            inherit_color: false,
                        }),
                )
                .with_child(runtime_loading_button(
                    "loading.slow.cancel",
                    "runtime.loading.cancel",
                    "runtime.cancel",
                    palette,
                    false,
                    96.0,
                    26.0,
                )),
        );
    }

    card = card.with_child(
        UiNode::new("loading.footer", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                justify_content: UiJustify::Center,
                align_items: UiAlign::Center,
                gap: 8.0,
                ..UiLayout::fixed(548.0, 20.0)
            })
            .with_child(
                UiNode::new("loading.engine", UiNodeKind::Label)
                    .with_text_value(t("app.engine", language))
                    .with_layout(
                        UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::FitContent),
                    )
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            )
            .with_child(
                UiNode::new("loading.version", UiNodeKind::Label)
                    .with_text_value(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .with_layout(
                        UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::FitContent),
                    )
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            ),
    );

    let root = UiNode::new("loading.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(card);

    let state_rule = |selector: &str, state: UiStyleRuleState, patch: UiStylePatch| {
        UiStyleRule::new(UiStyleSelector::Class(selector.to_string()), patch).when(state)
    };
    let mut rules = vec![UiStyleRule::new(
        UiStyleSelector::Class("loading-card".to_string()),
        UiStylePatch {
            fill: Some(tokens.surface),
            border: Some(tokens.border),
            border_width: Some(1.0),
            radius: Some(10.0),
            ..UiStylePatch::default()
        },
    )];
    rules.push(state_rule(
        "runtime-loading-retry",
        UiStyleRuleState::Hovered,
        UiStylePatch {
            fill: Some(tokens.accent_hot),
            border: Some(tokens.accent_hot),
            ..UiStylePatch::default()
        },
    ));
    rules.push(state_rule(
        "runtime-loading-retry",
        UiStyleRuleState::Active,
        UiStylePatch {
            fill: Some(tokens.accent_hot),
            border: Some(tokens.focus),
            ..UiStylePatch::default()
        },
    ));
    rules.push(state_rule(
        "runtime-loading-cancel",
        UiStyleRuleState::Hovered,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            ..UiStylePatch::default()
        },
    ));
    rules.push(state_rule(
        "runtime-loading-cancel",
        UiStyleRuleState::Active,
        UiStylePatch {
            fill: Some(tokens.selection),
            ..UiStylePatch::default()
        },
    ));
    rules.push(state_rule(
        "runtime-loading-button",
        UiStyleRuleState::Focused,
        UiStylePatch {
            border: Some(tokens.focus),
            border_width: Some(1.0),
            ..UiStylePatch::default()
        },
    ));

    let mut surface = UiSurface::new("runtime.loading", palette, root);
    surface.style_sheet = UiStyleSheet { rules };
    surface
}

fn runtime_loading_button(
    id: &str,
    label_key: &str,
    command: &str,
    palette: StudioUiPalette,
    accent: bool,
    width: f32,
    height: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let (class, fill, border, ink) = if accent {
        (
            "runtime-loading-retry",
            tokens.accent,
            tokens.accent,
            tokens.chrome().icon_on_accent,
        )
    } else {
        ("runtime-loading-cancel", [0; 4], tokens.border, tokens.text)
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class("runtime-loading-button")
        .with_class(class)
        .with_text_key(label_key)
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_layout(UiLayout::fixed(width, height))
        .with_style(UiStyle {
            fill,
            border,
            text: ink,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .with_text_style(UiTextStyle::button(ink))
        .with_visual(UiVisual {
            content: UiContentLayout {
                align: UiContentAlign::Center,
                gap: 0.0,
                ..UiContentLayout::default()
            },
            ..UiVisual::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Vec<String> {
        vec!["Compiling scripts".into(), "Loading game interface".into()]
    }

    fn view<'a>(
        failed: bool,
        slow: bool,
        can_retry: bool,
        history: &'a [String],
    ) -> RuntimeLoadingView<'a> {
        RuntimeLoadingView {
            palette: StudioUiPalette::IndustrialDark,
            progress: 0.5,
            language: Language::English,
            detail: "assets/scripts/player.raf",
            status: "Compiling scripts",
            count: "2/4",
            elapsed_secs: 7,
            history,
            slow,
            failed,
            error: Some("Script compile error"),
            can_retry,
        }
    }

    #[test]
    fn runtime_loading_reports_stage_count_path_and_history() {
        let history = history();
        let surface = build_runtime_loading_surface_with_options(view(false, false, true, &history));
        assert!(surface.root.find("loading.progress.row").is_some());
        assert!(surface.root.find("loading.detail").is_some());
        assert!(surface.root.find("loading.details.elapsed.value").is_some());
        assert!(surface.root.find("loading.history.1").is_some());
        assert!(surface.root.find("loading.message").is_some());
    }

    #[test]
    fn slow_preparation_exposes_cancel() {
        let surface =
            build_runtime_loading_surface_with_options(view(false, true, true, &[]));
        assert!(surface.root.find("loading.slow.message").is_some());
        let cancel = surface.root.find("loading.slow.cancel").unwrap();
        assert!(cancel.event_handlers.iter().any(|event| matches!(
            &event.action,
            raf_ui::UiAction::Command { name } if name == "runtime.cancel"
        )));
    }

    #[test]
    fn default_runtime_builder_is_retry_capable() {
        let surface = build_runtime_loading_surface(
            StudioUiPalette::IndustrialDark,
            0.25,
            Language::English,
            "assets/scripts/player.raf",
            "Compiling scripts",
            "1/4",
            3,
            &[],
            false,
            true,
            Some("Script compile error"),
        );
        assert!(surface.root.find("loading.action.retry").is_some());
        assert!(surface.root.find("loading.action.cancel").is_some());
    }

    #[test]
    fn failed_preparation_replaces_progress_with_retry_and_cancel() {
        let failed =
            build_runtime_loading_surface_with_options(view(true, false, false, &[]));
        assert!(failed.root.find("loading.progress.row").is_none());
        assert!(failed.root.find("loading.action.retry").is_none());
        let cancel = failed.root.find("loading.action.cancel").unwrap();
        assert!(cancel
            .event_handlers
            .iter()
            .any(|event| matches!(&event.action, raf_ui::UiAction::Command { name } if name == "runtime.cancel")));

        let retryable =
            build_runtime_loading_surface_with_options(view(true, false, true, &[]));
        let retry = retryable.root.find("loading.action.retry").unwrap();
        assert!(!retry.disabled);
        assert!(retry.event_handlers.iter().any(|event| matches!(
            &event.action,
            raf_ui::UiAction::Command { name } if name == "runtime.retry"
        )));
    }
}
