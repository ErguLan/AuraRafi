//! Retained resize handles for the editor workbench.
//!
//! The layout math stays in `EditorFrameLayout`; these small surfaces only
//! provide a wide, discoverable pointer contract around the visual borders.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSplitterKind {
    LeftPanel,
    RightPanel,
    BottomDock,
}

impl EditorSplitterKind {
    fn id(self) -> &'static str {
        match self {
            Self::LeftPanel => "left",
            Self::RightPanel => "right",
            Self::BottomDock => "bottom",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::LeftPanel | Self::RightPanel => "editor-splitter-vertical",
            Self::BottomDock => "editor-splitter-horizontal",
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::LeftPanel => "editor.resize_hierarchy",
            Self::RightPanel => "editor.resize_inspector",
            Self::BottomDock => "editor.resize_downbar",
        }
    }
}

pub fn build_editor_splitter_surface(
    palette: StudioUiPalette,
    kind: EditorSplitterKind,
) -> UiSurface {
    let tokens = palette.tokens();
    let id = kind.id();
    let target = format!("layout.{id}");
    let root = UiNode::new(format!("editor.splitter.{id}"), UiNodeKind::Button)
        .with_class(kind.class())
        // The layout rectangle is supplied by the workbench. Keeping the
        // node filled makes the hitbox larger than the one-pixel divider.
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle {
            fill: [0, 0, 0, 0],
            border: [0, 0, 0, 0],
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_tooltip_key(kind.tooltip())
        .with_accessibility_label_key(kind.tooltip())
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!("layout.resize.start.{target}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            format!("layout.resize.move.{target}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            format!("layout.resize.end.{target}"),
        ));

    let mut surface = UiSurface::new(format!("editor.splitter.{id}"), palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class(kind.class().to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class(kind.class().to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    opacity: Some(0.8),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class(kind.class().to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    opacity: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
        ],
    };
    surface
}
