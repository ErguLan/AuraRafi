//! Platform-neutral window interaction contracts for RafUI.
//!
//! RafUI describes intent here; the native host executes the operation against
//! the real OS window. No widget toolkit owns these commands.

use serde::{Deserialize, Serialize};

use crate::events::UiCursorIcon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiWindowCommand {
    BeginDrag,
    BeginResize(UiResizeEdge),
    Minimize,
    ToggleMaximize,
    Close,
    ShowSystemMenu,
}

impl UiResizeEdge {
    pub const fn cursor(self) -> UiCursorIcon {
        match self {
            Self::North | Self::South => UiCursorIcon::ResizeVertical,
            Self::East | Self::West => UiCursorIcon::ResizeHorizontal,
            Self::NorthEast | Self::SouthWest => UiCursorIcon::ResizeNorthEastSouthWest,
            Self::NorthWest | Self::SouthEast => UiCursorIcon::ResizeNorthWestSouthEast,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiWindowHitTest {
    pub command: UiWindowCommand,
    pub cursor: UiCursorIcon,
}

impl UiWindowHitTest {
    pub const fn drag() -> Self {
        Self {
            command: UiWindowCommand::BeginDrag,
            cursor: UiCursorIcon::Default,
        }
    }

    pub const fn resize(edge: UiResizeEdge) -> Self {
        Self {
            command: UiWindowCommand::BeginResize(edge),
            cursor: edge.cursor(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_edges_map_to_native_cursor_intents() {
        assert_eq!(
            UiWindowHitTest::resize(UiResizeEdge::East).cursor,
            UiCursorIcon::ResizeHorizontal
        );
        assert_eq!(
            UiWindowHitTest::resize(UiResizeEdge::SouthWest).cursor,
            UiCursorIcon::ResizeNorthEastSouthWest
        );
    }

    #[test]
    fn drag_is_a_window_intent_not_a_widget_toolkit_command() {
        assert_eq!(UiWindowHitTest::drag().command, UiWindowCommand::BeginDrag);
    }
}
