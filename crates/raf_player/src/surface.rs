//! Runtime presentation only; execution state is owned by raf_runtime.
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use raf_runtime::RuntimePhase;
use raf_ui::{
    UiAccessibilityRole, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiLayout, UiNode,
    UiNodeKind, UiRect, UiSpacing, UiStyle, UiTextStyle,
};
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeToolbarState {
    pub phase: Option<RuntimePhase>,
    pub can_launch: bool,
    pub instances: usize,
}
pub fn runtime_controls(palette: StudioUiPalette, state: &RuntimeToolbarState) -> UiNode {
    let tokens = palette.tokens();
    let paused = state.phase == Some(RuntimePhase::Paused);
    let running = state.phase == Some(RuntimePhase::Running);
    let active = matches!(
        state.phase,
        Some(
            RuntimePhase::Preparing
                | RuntimePhase::Running
                | RuntimePhase::Paused
                | RuntimePhase::Stopping
                | RuntimePhase::Failed
        )
    );
    let mut root = UiNode::new("runtime.controls", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 2.0,
            align_items: raf_ui::UiAlign::Center,
            ..UiLayout::fixed(
                130.0
                    + if state.instances > 0 { 80.0 } else { 0.0 }
                    + if state.instances > 1 { 34.0 } else { 0.0 },
                28.0,
            )
        })
        .with_style(UiStyle::transparent());
    for (id, icon, key, command, enabled) in [
        (
            "play",
            UiIconId::Play,
            "runtime.play",
            if paused {
                "runtime.resume"
            } else {
                "runtime.play"
            },
            paused || state.can_launch,
        ),
        (
            "pause",
            UiIconId::Pause,
            "runtime.pause",
            "runtime.pause",
            running,
        ),
        (
            "step",
            UiIconId::StepForward,
            "runtime.step",
            "runtime.step",
            paused,
        ),
        (
            "stop",
            UiIconId::Stop,
            "runtime.stop",
            "runtime.stop",
            active,
        ),
    ] {
        root = root.with_child(
            UiNode::new(format!("runtime.control.{id}"), UiNodeKind::Button)
                .with_layout(UiLayout::fixed(30.0, 26.0))
                .with_icon(UiIcon::new(icon).with_tint(if id == "play" {
                    tokens.accent
                } else {
                    tokens.text
                }))
                .with_tooltip_key(key)
                .with_accessibility_label_key(key)
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .disabled(!enabled)
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }
    if state.instances > 0 {
        root = root.with_child(
            UiNode::new("runtime.control.phase", UiNodeKind::Label)
                .with_text_key(phase_key(state.phase))
                .with_layout(UiLayout::fixed(78.0, 24.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    if state.instances > 1 {
        root = root.with_child(
            UiNode::new("runtime.control.instance", UiNodeKind::Button)
                .with_text_value(state.instances.to_string())
                .with_tooltip_key("runtime.next")
                .with_accessibility_label_key("runtime.next")
                .focusable()
                .with_layout(UiLayout::fixed(32.0, 26.0))
                .with_event(UiEventBinding::command(UiEventKind::Click, "runtime.next")),
        );
    }
    root
}
pub fn phase_key(phase: Option<RuntimePhase>) -> &'static str {
    match phase {
        None => "runtime.idle",
        Some(RuntimePhase::Preparing) => "runtime.preparing",
        Some(RuntimePhase::Running) => "runtime.running",
        Some(RuntimePhase::Paused) => "runtime.paused",
        Some(RuntimePhase::Stopping) => "runtime.stopping",
        Some(RuntimePhase::Stopped) => "runtime.stopped",
        Some(RuntimePhase::Failed) => "runtime.failed",
    }
}
pub fn player_surface(
    palette: StudioUiPalette,
    phase: Option<RuntimePhase>,
    size: [u32; 2],
    missing_camera: bool,
    error: Option<&str>,
    diagnostics: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let controls = runtime_controls(
        palette,
        &RuntimeToolbarState {
            phase,
            can_launch: false,
            instances: 1,
        },
    );
    let toolbar = UiNode::new("runtime.player.toolbar", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 10.0,
            padding: UiSpacing::xy(8.0, 0.0),
            align_items: raf_ui::UiAlign::Center,
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, size[0] as f32, 34.0))
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            ..UiStyle::transparent()
        })
        .with_child(
            UiNode::new("runtime.player.name", UiNodeKind::Label)
                .with_text_key("runtime.local")
                .with_layout(UiLayout::fixed(130.0, 24.0))
                .with_text_style(UiTextStyle::body(tokens.text)),
        )
        .with_child(controls);
    let mut root = UiNode::new("runtime.player.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle::transparent())
        .with_child(toolbar);
    if missing_camera {
        root = root.with_child(
            UiNode::new("runtime.player.no-camera", UiNodeKind::Label)
                .with_text_key("runtime.no_camera")
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::absolute(UiRect::new(
                    32.0,
                    size[1] as f32 * 0.5,
                    size[0] as f32 - 64.0,
                    70.0,
                )))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    if diagnostics {
        if let Some(error) = error {
            root = root.with_child(
                UiNode::new("runtime.player.error", UiNodeKind::Label)
                    .with_text_value(error)
                    .selectable_text()
                    .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                    .with_layout(UiLayout::absolute(UiRect::new(
                        16.0,
                        48.0,
                        size[0] as f32 - 32.0,
                        80.0,
                    )))
                    .with_text_style(UiTextStyle::body(tokens.accent)),
            );
        }
    }
    UiSurface::new("runtime.player", palette, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_follow_confirmed_state_and_locales_exist() {
        for phase in [
            None,
            Some(RuntimePhase::Preparing),
            Some(RuntimePhase::Running),
            Some(RuntimePhase::Paused),
            Some(RuntimePhase::Stopped),
        ] {
            let state = RuntimeToolbarState {
                phase,
                can_launch: phase.is_none(),
                instances: usize::from(phase.is_some()),
            };
            let root = runtime_controls(StudioUiPalette::IndustrialDark, &state);
            assert_eq!(
                root.children
                    .iter()
                    .filter(|n| n.kind == UiNodeKind::Button)
                    .count(),
                4
            );
            let disabled = |id: &str| {
                root.children
                    .iter()
                    .find(|node| node.id == format!("runtime.control.{id}"))
                    .unwrap()
                    .disabled
            };
            assert_eq!(disabled("pause"), phase != Some(RuntimePhase::Running));
            assert_eq!(disabled("step"), phase != Some(RuntimePhase::Paused));
            assert_eq!(
                disabled("play"),
                !state.can_launch && phase != Some(RuntimePhase::Paused)
            );
            assert_eq!(
                disabled("stop"),
                matches!(phase, None | Some(RuntimePhase::Stopped))
            );
            for language in [
                raf_core::config::Language::English,
                raf_core::config::Language::Spanish,
            ] {
                assert_ne!(
                    raf_core::i18n::t(phase_key(phase), language),
                    phase_key(phase)
                );
            }
        }
    }
}
