//! One-line runtime failure banner for the Game workbench.
//!
//! The application owns the notice; this surface only renders it and emits the
//! retry/dismiss commands consumed by the workbench input boundary.

use raf_core::config::Language;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiSizeMode, UiSpacing, UiSurface, UiTextStyle,
};
use raf_runtime::wire::RuntimeDiagnosticSeverity;
use raf_ui::{UiAccessibilityRole, UiStyle, UiTextOverflow};

use crate::native_game_runtime::RuntimeNotice;
use crate::native_runtime_log::localized_runtime_text;

pub const RUNTIME_NOTICE_HEIGHT: f32 = 36.0;

pub fn build_runtime_notice_surface(
    palette: StudioUiPalette,
    notice: &RuntimeNotice,
    language: Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let diagnostic = notice.diagnostic();
    let (severity_key, severity_class, color) = match diagnostic.severity {
        RuntimeDiagnosticSeverity::Info => (
            "console.severity.info",
            "runtime-notice-info",
            tokens.accent_hot,
        ),
        RuntimeDiagnosticSeverity::Warning => (
            "console.severity.warning",
            "runtime-notice-warning",
            tokens.warning,
        ),
        RuntimeDiagnosticSeverity::Error => (
            "console.severity.error",
            "runtime-notice-error",
            tokens.danger,
        ),
    };
    let message = localized_runtime_text(&diagnostic.message, language);
    let detail = diagnostic
        .detail
        .as_deref()
        .map(|detail| localized_runtime_text(detail, language));

    let mut root = UiNode::new("runtime.notice", UiNodeKind::Panel)
        .with_class(format!("runtime-notice {severity_class}"))
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 4.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, RUNTIME_NOTICE_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(
            UiStyle::transparent()
                .with_fill(tokens.surface_alt)
                .with_border(color, 1.0)
                .with_radius(6.0),
        )
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key("console.runtime")
        .with_child(
            UiNode::new("runtime.notice.severity", UiNodeKind::Label)
                .with_class(format!("runtime-notice-severity {severity_class}"))
                .with_text_key(severity_key)
                .with_layout(UiLayout::fixed(56.0, 22.0).with_text_safe_area(true))
                .with_text_style(UiTextStyle::button(color)),
        )
        .with_child(
            UiNode::new("runtime.notice.message", UiNodeKind::Label)
                .with_text_value(message)
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [60.0, 22.0],
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fixed(0.0, 22.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .with_text_safe_area(false)
                })
                .with_text_overflow(UiTextOverflow::Clip)
                .with_text_style(UiTextStyle {
                    size_px: 12.0,
                    line_height_px: 18.0,
                    ..UiTextStyle::body(tokens.text)
                }),
        );
    if let Some(detail) = detail {
        root = root.with_child(
            UiNode::new("runtime.notice.detail", UiNodeKind::Label)
                .with_text_value(detail)
                .with_layout(UiLayout {
                    max_size: [300.0, 22.0],
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fixed(0.0, 22.0).with_text_safe_area(false)
                })
                .with_text_overflow(UiTextOverflow::Clip)
                .with_text_style(UiTextStyle {
                    role: raf_ui::UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: raf_ui::UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                }),
        );
    }
    root = root
        .with_child(action_button(
            "runtime.notice.retry",
            "runtime.loading.retry",
            "runtime.notice.retry",
            tokens.surface_raised,
            color,
            tokens.text,
        ))
        .with_child(action_button(
            "runtime.notice.cancel",
            "runtime.loading.cancel",
            "runtime.notice.dismiss",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ));

    UiSurface::new("runtime.notice", palette, root)
}

fn action_button(
    id: &str,
    text_key: &str,
    command: &str,
    fill: [u8; 4],
    border: [u8; 4],
    text: [u8; 4],
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("runtime-notice-action")
        .with_text_key(text_key)
        .with_layout(UiLayout::fixed(80.0, 24.0).with_text_safe_area(true))
        .with_text_style(UiTextStyle::button(text))
        .with_style(
            UiStyle::transparent()
                .with_fill(fill)
                .with_border(border, 1.0)
                .with_radius(4.0),
        )
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_runtime::wire::RuntimeDiagnostic;

    fn find<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|candidate| find(candidate, id))
    }

    #[test]
    fn banner_exposes_error_retry_and_cancel() {
        let notice = RuntimeNotice::from_diagnostic(
            RuntimeDiagnostic::error(
                "console.runtime_timeout",
                "editor",
                "console.runtime_timeout",
            )
            .detail("after 20s"),
        );
        let surface = build_runtime_notice_surface(
            StudioUiPalette::IndustrialDark,
            &notice,
            Language::English,
        );

        let severity = find(&surface.root, "runtime.notice.severity").expect("severity badge");
        assert_eq!(severity.text_key.as_deref(), Some("console.severity.error"));
        let retry = find(&surface.root, "runtime.notice.retry").expect("retry action");
        assert!(retry.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "runtime.notice.retry"
        )));
        let cancel = find(&surface.root, "runtime.notice.cancel").expect("cancel action");
        assert_eq!(cancel.text_key.as_deref(), Some("runtime.loading.cancel"));
    }
}
