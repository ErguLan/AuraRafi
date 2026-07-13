use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiPointerButton {
    Primary,
    Secondary,
    Middle,
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
