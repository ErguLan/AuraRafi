use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiPointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Renderer-agnostic pointer affordance requested by a retained surface.
/// Native hosts translate this into their windowing toolkit's cursor enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiCursorIcon {
    Default,
    PointingHand,
    Text,
    ResizeHorizontal,
    ResizeVertical,
    ResizeNorthEastSouthWest,
    ResizeNorthWestSouthEast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiEventKind {
    Click,
    DoubleClick,
    PointerDown(UiPointerButton),
    PointerUp(UiPointerButton),
    PointerMove,
    DragStart,
    DragMove,
    DragEnd,
    HoverEnter,
    HoverLeave,
    ContextMenu,
    KeyPress(String),
    TextInput(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiAction {
    None,
    Command {
        name: String,
    },
    ToggleState {
        key: String,
    },
    SetToggle {
        key: String,
        value: bool,
    },
    SetRange {
        key: String,
        value: f32,
    },
    SetColorHsv {
        key: String,
        hue: f32,
        saturation: f32,
        value: f32,
    },
    SetSelect {
        key: String,
        value: String,
        index: usize,
    },
    SetSelectOpen {
        id: String,
        open: bool,
    },
    SetText {
        key: String,
        value: String,
    },
    SetClipboard {
        text: String,
    },
    ScrollTo {
        id: String,
        offset: [f32; 2],
    },
    FocusNode {
        id: String,
    },
    OpenMenu {
        id: String,
    },
    Custom {
        channel: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiEventBinding {
    pub event: UiEventKind,
    pub action: UiAction,
}

impl UiEventBinding {
    pub fn command(event: UiEventKind, name: impl Into<String>) -> Self {
        Self {
            event,
            action: UiAction::Command { name: name.into() },
        }
    }
}
