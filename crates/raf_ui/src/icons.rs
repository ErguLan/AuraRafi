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
    Refresh,
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
    Wire,
    Route,
    BoardOutline,
    ZoomIn,
    ZoomOut,
    Trash,
    Script,
    File,
    ExternalLink,
    Pencil,
    Copy,
    Settings,
    Menu,
    Warning,
    Error,
    Success,
}

impl UiIconId {
    /// Every semantic icon in the family. Coverage tests and tooling iterate
    /// this instead of mirroring the enum by hand.
    pub const ALL: [Self; 57] = [
        Self::Select,
        Self::Move,
        Self::Rotate,
        Self::Scale,
        Self::Focus,
        Self::Undo,
        Self::Redo,
        Self::Refresh,
        Self::Grid,
        Self::View2d,
        Self::View3d,
        Self::Shaded,
        Self::Wireframe,
        Self::Folder,
        Self::Scene,
        Self::Entity,
        Self::Cube,
        Self::Sphere,
        Self::Plane,
        Self::Cylinder,
        Self::Eye,
        Self::EyeOff,
        Self::Lock,
        Self::Unlock,
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::ChevronDown,
        Self::More,
        Self::Search,
        Self::Filter,
        Self::Add,
        Self::Close,
        Self::Play,
        Self::Stop,
        Self::Console,
        Self::Assets,
        Self::Project,
        Self::Node,
        Self::Agent,
        Self::Schematic,
        Self::Pcb,
        Self::Wire,
        Self::Route,
        Self::BoardOutline,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::Trash,
        Self::Script,
        Self::File,
        Self::ExternalLink,
        Self::Pencil,
        Self::Copy,
        Self::Settings,
        Self::Menu,
        Self::Warning,
        Self::Error,
        Self::Success,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Move => "move",
            Self::Rotate => "rotate",
            Self::Scale => "scale",
            Self::Focus => "focus",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Refresh => "refresh",
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
            Self::Wire => "wire",
            Self::Route => "route",
            Self::BoardOutline => "board-outline",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
            Self::Trash => "trash",
            Self::Script => "script",
            Self::File => "file",
            Self::ExternalLink => "external-link",
            Self::Pencil => "pencil",
            Self::Copy => "copy",
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

    #[test]
    fn all_lists_every_icon_with_a_unique_key() {
        let mut keys: Vec<&'static str> = UiIconId::ALL.iter().map(|id| id.key()).collect();
        keys.sort_unstable();
        keys.dedup();

        assert_eq!(keys.len(), UiIconId::ALL.len(), "icon keys must be unique");
        assert_eq!(UiIconId::ALL.len(), 57);
    }
}
