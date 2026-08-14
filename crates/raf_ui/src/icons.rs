//! Semantic icon contracts for retained RafUI surfaces.
//!
//! Surface builders refer to an icon by stable meaning rather than by a PNG
//! path. ApiGraphicBasic owns resolving that meaning into vector or bitmap
//! resources at the requested physical density.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiIconId {
    Select,
    Move,
    Rotate,
    Scale,
    Focus,
    Undo,
    Redo,
    Grid,
    View2d,
    View3d,
    Shaded,
    Wireframe,
    Folder,
    Scene,
    Entity,
    Cube,
    Sphere,
    Plane,
    Cylinder,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    More,
    Search,
    Filter,
    Add,
    Close,
    Play,
    Stop,
    Console,
    Assets,
    Project,
    Node,
    Agent,
    Schematic,
    Pcb,
    Settings,
    Menu,
    Warning,
    Error,
    Success,
}

impl UiIconId {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Move => "move",
            Self::Rotate => "rotate",
            Self::Scale => "scale",
            Self::Focus => "focus",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Grid => "grid",
            Self::View2d => "view-2d",
            Self::View3d => "view-3d",
            Self::Shaded => "shaded",
            Self::Wireframe => "wireframe",
            Self::Folder => "folder",
            Self::Scene => "scene",
            Self::Entity => "entity",
            Self::Cube => "cube",
            Self::Sphere => "sphere",
            Self::Plane => "plane",
            Self::Cylinder => "cylinder",
            Self::Eye => "eye",
            Self::EyeOff => "eye-off",
            Self::Lock => "lock",
            Self::Unlock => "unlock",
            Self::ChevronLeft => "chevron-left",
            Self::ChevronRight => "chevron-right",
            Self::ChevronDown => "chevron-down",
            Self::More => "more",
            Self::Search => "search",
            Self::Filter => "filter",
            Self::Add => "add",
            Self::Close => "close",
            Self::Play => "play",
            Self::Stop => "stop",
            Self::Console => "console",
            Self::Assets => "assets",
            Self::Project => "project",
            Self::Node => "node",
            Self::Agent => "agent",
            Self::Schematic => "schematic",
            Self::Pcb => "pcb",
            Self::Settings => "settings",
            Self::Menu => "menu",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Success => "success",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiIconSize {
    Small,
    Toolbar,
    Panel,
    Custom(u16),
}

impl UiIconSize {
    pub const fn logical_pixels(self) -> u16 {
        match self {
            Self::Small => 14,
            Self::Toolbar => 18,
            Self::Panel => 20,
            Self::Custom(size) => size,
        }
    }
}

impl Default for UiIconSize {
    fn default() -> Self {
        Self::Toolbar
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiIconState {
    Normal,
    Hovered,
    Active,
    Disabled,
}

impl Default for UiIconState {
    fn default() -> Self {
        Self::Normal
    }
}

/// A renderer-neutral icon request embedded in a retained node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UiIcon {
    pub id: UiIconId,
    #[serde(default)]
    pub size: UiIconSize,
    #[serde(default)]
    pub state: UiIconState,
    #[serde(default)]
    pub tint: Option<[u8; 4]>,
}

impl UiIcon {
    pub const fn new(id: UiIconId) -> Self {
        Self {
            id,
            size: UiIconSize::Toolbar,
            state: UiIconState::Normal,
            tint: None,
        }
    }

    pub const fn with_size(mut self, size: UiIconSize) -> Self {
        self.size = size;
        self
    }

    pub const fn with_tint(mut self, tint: [u8; 4]) -> Self {
        self.tint = Some(tint);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_identity_is_stable_and_does_not_depend_on_a_file_path() {
        let icon = UiIcon::new(UiIconId::Undo).with_size(UiIconSize::Small);

        assert_eq!(icon.id.key(), "undo");
        assert_eq!(icon.size.logical_pixels(), 14);
    }
}
