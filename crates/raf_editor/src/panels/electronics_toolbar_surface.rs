//! Retained RafUI toolbar for the native Electronics workspace.
//!
//! The band used to be one flat list of sixteen equally weighted buttons, so it
//! answered four unrelated questions at once: which document is open, what the
//! current tool does to it, how the document is presented, and how far back the
//! session can go. It is now composed as three groups that never share a visual
//! treatment:
//!
//! 1. `document` (left corner): a segmented schematic/PCB selector, the tools of
//!    the active surface, and the two selection edits. Everything in this group
//!    changes the document.
//! 2. `view` (right corner): grid, snap and labels, plus fit and the two zoom
//!    steps. Nothing in this group changes the document.
//! 3. `history` (far right): undo and redo. They never author new geometry and
//!    they duplicate the application Edit menu and Ctrl+Z / Ctrl+Y.
//!
//! Only the tool in the user's hand wears the saturated accent fill, because
//! exactly one tool can be engaged at a time. A latched view option and the
//! active document segment use the pale `selection` wash instead, so "the tool
//! I hold" and "the option I turned on" cannot be read as the same state even
//! though both use the accent hue.
//!
//! # Density is a measured ladder, never a scroll rail
//!
//! `append_scrollbar` in the compositor returns early unless the *vertical* max
//! offset is positive, so a horizontal scroll owner clips its overflow with no
//! thumb and no reachable control. The band therefore never scrolls: every
//! density is sized to fit by construction and the root clips.
//!
//! - `Labelled` (canvas at least `ELECTRONICS_TOOLBAR_LABELLED_WIDTH`): one row
//!   where every control whose glyph is not a universal convention prints its
//!   localized label. Measured worst case is the Spanish PCB row.
//! - `Icon` (canvas at least `ELECTRONICS_TOOLBAR_STACKED_WIDTH`): one row of
//!   glyph only.
//! - `Stacked` (narrower canvas): the document row and the view/history row
//!   stack, and the reserved band grows to
//!   `ELECTRONICS_TOOLBAR_STACKED_HEIGHT`.
//!
//! Nothing here keeps state: the host passes the current tool, the view toggles
//! and the availability of every command as values, and this surface only
//! presents them.

mod actions;
mod menus;
mod pcb;
mod schematic;
mod styles;
#[cfg(test)]
mod tests;

use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiIconId, UiSurface};
use raf_ui::{
    UiAlign, UiFlow, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing,
};

use crate::editor_layout::{
    electronics_toolbar_height, ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
    ELECTRONICS_TOOLBAR_STACKED_WIDTH,
};
use crate::electronics_controller::ElectronicsTool;
use crate::panels::electronics_surface::ELECTRONICS_CONTROL_HEIGHT;
use raf_electronics::CadSurfaceKind;

use self::actions::{
    control_button, document_controls, history_controls, tool_is_engaged, tool_island,
    view_controls,
};
use self::menus::{build_desktop_ribbon, document_segment};
use self::styles::style_sheet;

/// Control height of a toolbar button, in logical points. It is the shared
/// Electronics control height, so the toolbar, the dock actions and the context
/// menu rows resolve to one track.
const TOOLBAR_BUTTON_HEIGHT: f32 = ELECTRONICS_CONTROL_HEIGHT;
/// Width of a glyph-only toolbar button, in logical points.
///
/// 24 points is the documented floor for a compact pointer target and it is
/// measured, not chosen: every span arithmetic in the tests depends on it, so a
/// wider glyph-only control would push the PCB row past its breakpoint.
const TOOLBAR_ICON_BUTTON_WIDTH: f32 = 24.0;
/// Smallest width a labelled control may claim, in logical points. The resolved
/// label almost always measures wider; this only protects a one-letter label.
const TOOLBAR_BUTTON_MIN_WIDTH: f32 = 24.0;
/// Gap between compact controls and around a group hairline, in logical points.
/// The narrower rhythm preserves the mode labels and all commands at the 590pt
/// one-row breakpoint.
const TOOLBAR_GAP: f32 = 2.0;
/// Vertical gap between the two rows of the stacked density, in logical points.
const TOOLBAR_ROW_GAP: f32 = 4.0;
/// Horizontal padding of the band, in logical points.
const TOOLBAR_PADDING_X: f32 = 6.0;
/// Height of a group hairline, in logical points.
const SEPARATOR_HEIGHT: f32 = 18.0;
/// Inset between the document segment and its two options, in logical points.
const SEGMENT_INSET: f32 = 2.0;
/// Gap between the two document segment options, in logical points.
const SEGMENT_GAP: f32 = 2.0;
/// Smallest width of the flexible right group, in logical points.
///
/// The group absorbs the free space of the band so `view` and `history` sit in
/// the right corner. It still needs a floor: a zero-width group is a collapsed
/// layout box, and a collapsed group pushes its children out of the band instead
/// of letting them stay visible.
const RIGHT_GROUP_MIN_WIDTH: f32 = 8.0;

/// Class of the one control allowed the saturated accent fill.
const CLASS_TOOL_ACTIVE: &str = "electronics-toolbar-tool-active";
/// Class of an engaged option and of the engaged document segment.
const CLASS_OPTION_ON: &str = "electronics-toolbar-option-on";
/// Class of a command whose prerequisite is missing.
const CLASS_UNAVAILABLE: &str = "electronics-toolbar-unavailable";
/// Class of the destructive command while it is available.
const CLASS_DANGER: &str = "electronics-toolbar-danger";
/// Shared resting class of every control of the band.
const CLASS_BUTTON: &str = "electronics-toolbar-button";
/// Neutral islands that make the active surface's tool families scannable.
const CLASS_TOOL_ISLAND_SELECT: &str = "electronics-toolbar-island-select";
const CLASS_TOOL_ISLAND_CONNECT: &str = "electronics-toolbar-island-connect";
const CLASS_TOOL_ISLAND_PLACE: &str = "electronics-toolbar-island-place";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolIsland {
    Selection,
    Connection,
    Placement,
    Operations,
}

/// How much of the band a control may occupy at one canvas width.
///
/// This is a presentation decision derived from the canvas width the host
/// already passes, so the reserved band height and the document cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolbarDensity {
    /// One row, localized labels on the controls that need them.
    Labelled,
    /// One row, glyph only.
    Icon,
    /// Two rows, glyph only.
    Stacked,
}

impl ToolbarDensity {
    fn for_width(width: f32) -> Self {
        if width >= ELECTRONICS_TOOLBAR_LABELLED_WIDTH {
            Self::Labelled
        } else if width >= ELECTRONICS_TOOLBAR_STACKED_WIDTH {
            Self::Icon
        } else {
            Self::Stacked
        }
    }

    /// True when a control with a localized label may print it.
    fn prints_labels(self) -> bool {
        matches!(self, Self::Labelled)
    }

    fn rows(self) -> u32 {
        match self {
            Self::Stacked => 2,
            Self::Labelled | Self::Icon => 1,
        }
    }

    /// True when the band is a single row.
    fn is_single_row(self) -> bool {
        self.rows() == 1
    }

    /// Vertical padding that makes the rows fill the reserved band exactly.
    fn padding_y(self, canvas_width: f32) -> f32 {
        let rows = self.rows() as f32;
        let content = rows * TOOLBAR_BUTTON_HEIGHT + (rows - 1.0) * TOOLBAR_ROW_GAP;
        ((electronics_toolbar_height(canvas_width) - content) * 0.5).max(0.0)
    }
}

/// What one control does. It decides the role, the label policy and the engaged
/// treatment, so those three answers cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    /// Exclusive pointer tool. Exactly one is engaged, and it is the only
    /// control of the band allowed the saturated accent fill.
    Tool,
    /// Exclusive document selector rendered as one segment.
    Segment,
    /// Latched presentation option that never touches the document.
    Option,
    /// One-shot command. It either applies or explains why it cannot.
    Command,
}

/// One control of the band.
///
/// The table is the whole information architecture: a control is declared in one
/// place and its command, glyph, label and role all come from the same row.
struct ToolbarControl {
    command: &'static str,
    icon: UiIconId,
    /// Localized label printed by the labelled density. `None` means the glyph
    /// is a universal convention, so a label would only add width.
    label: Option<&'static str>,
    kind: ControlKind,
    /// Prerequisite reported by the host. `None` for a control that is always
    /// available.
    available: Option<bool>,
    /// True when the command discards work and keeps the danger treatment.
    danger: bool,
}

impl ToolbarControl {
    fn new(
        command: &'static str,
        icon: UiIconId,
        label: Option<&'static str>,
        kind: ControlKind,
    ) -> Self {
        Self {
            command,
            icon,
            label,
            kind,
            available: None,
            danger: false,
        }
    }

    fn tool(command: &'static str, icon: UiIconId, label: Option<&'static str>) -> Self {
        Self::new(command, icon, label, ControlKind::Tool)
    }

    fn segment(command: &'static str, icon: UiIconId, label: Option<&'static str>) -> Self {
        Self::new(command, icon, label, ControlKind::Segment)
    }

    fn option(command: &'static str, icon: UiIconId, label: Option<&'static str>) -> Self {
        Self::new(command, icon, label, ControlKind::Option)
    }

    fn command(
        command: &'static str,
        icon: UiIconId,
        label: Option<&'static str>,
        available: bool,
    ) -> Self {
        Self {
            available: Some(available),
            ..Self::new(command, icon, label, ControlKind::Command)
        }
    }

    fn destructive(mut self) -> Self {
        self.danger = true;
        self
    }

    fn id(&self) -> String {
        format!("electronics.toolbar.{}", self.command)
    }

    /// True when the label of this control is the only thing that says what it
    /// does, so the labelled density must print it.
    fn needs_label(&self) -> bool {
        self.label.is_some()
    }
}

/// Host-reported state of the Electronics workspace.
///
/// It is a value on purpose: the band is rebuilt from it every revision and the
/// surface keeps nothing of its own.
struct ToolbarState {
    surface: CadSurfaceKind,
    tool: ElectronicsTool,
    grid_visible: bool,
    labels_visible: bool,
    snap_enabled: bool,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    can_rotate: bool,
}

impl ToolbarState {
    /// Every prerequisite satisfied. Used by the geometry tests as the readable
    /// default of a freshly opened document.
    #[cfg(test)]
    fn available(surface: CadSurfaceKind, tool: ElectronicsTool) -> Self {
        Self {
            surface,
            tool,
            grid_visible: true,
            labels_visible: true,
            snap_enabled: true,
            can_undo: true,
            can_redo: true,
            has_selection: true,
            can_rotate: true,
        }
    }

    /// Nothing is available: no selection, no history and every view option off.
    #[cfg(test)]
    fn nothing_available(surface: CadSurfaceKind, tool: ElectronicsTool) -> Self {
        Self {
            grid_visible: false,
            labels_visible: false,
            snap_enabled: false,
            can_undo: false,
            can_redo: false,
            has_selection: false,
            can_rotate: false,
            ..Self::available(surface, tool)
        }
    }
}

/// Builds the retained Electronics toolbar band for one canvas width.
///
/// `width` is the canvas width, which is the same number the layout used to
/// reserve the band, so the document and the reserved rect cannot disagree.
pub fn build_electronics_toolbar_surface(
    palette: StudioUiPalette,
    surface: CadSurfaceKind,
    tool: ElectronicsTool,
    grid_visible: bool,
    labels_visible: bool,
    snap_enabled: bool,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    can_rotate: bool,
    width: f32,
) -> UiSurface {
    if width >= 1100.0 {
        let state = ToolbarState {
            surface,
            tool,
            grid_visible,
            labels_visible,
            snap_enabled,
            can_undo,
            can_redo,
            has_selection,
            can_rotate,
        };
        return build_desktop_ribbon(palette, &state, width);
    }
    build_compact_electronics_toolbar_surface(
        palette,
        surface,
        tool,
        grid_visible,
        labels_visible,
        snap_enabled,
        can_undo,
        can_redo,
        has_selection,
        can_rotate,
        width,
    )
}

fn build_compact_electronics_toolbar_surface(
    palette: StudioUiPalette,
    surface: CadSurfaceKind,
    tool: ElectronicsTool,
    grid_visible: bool,
    labels_visible: bool,
    snap_enabled: bool,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    can_rotate: bool,
    width: f32,
) -> UiSurface {
    let state = ToolbarState {
        surface,
        tool,
        grid_visible,
        labels_visible,
        snap_enabled,
        can_undo,
        can_redo,
        has_selection,
        can_rotate,
    };
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let density = ToolbarDensity::for_width(width);
    let document = document_cluster(palette, &state, density, TOOLBAR_BUTTON_HEIGHT);
    let view_and_history = view_cluster(palette, &state, density, TOOLBAR_BUTTON_HEIGHT);

    let mut root = UiNode::new("electronics.toolbar", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar")
        .with_layout(UiLayout {
            flow: if density.is_single_row() {
                UiFlow::Row
            } else {
                UiFlow::Column
            },
            align_items: if density.is_single_row() {
                UiAlign::Center
            } else {
                UiAlign::Stretch
            },
            gap: if density.is_single_row() {
                TOOLBAR_GAP
            } else {
                TOOLBAR_ROW_GAP
            },
            padding: UiSpacing::xy(TOOLBAR_PADDING_X, density.padding_y(width)),
            // The band clips instead of scrolling: this compositor draws a
            // scrollbar only for a vertical owner, so a horizontal scroll view
            // would clip controls with no reachable thumb.
            overflow: UiOverflow::Clip,
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, electronics_toolbar_height(width)],
            ..UiLayout::default()
        });

    root = if density.is_single_row() {
        root.with_child(document)
            .with_child(separator("electronics.toolbar.document-separator"))
            .with_child(view_and_history)
    } else {
        root.with_child(document).with_child(view_and_history)
    };

    let mut document_surface = UiSurface::new("electronics.toolbar", palette, root);
    document_surface.style_sheet = style_sheet(&tokens);
    document_surface
}

/// The document group: the surface segment, the tools and the selection edits.
fn document_cluster(
    palette: StudioUiPalette,
    state: &ToolbarState,
    density: ToolbarDensity,
    height: f32,
) -> UiNode {
    let mut group = group_row("electronics.toolbar.cluster.document", height);
    group = group
        .with_child(document_segment(palette, state, density, height))
        .with_child(separator("electronics.toolbar.surface-separator"));
    let mut previous_island = None;
    for control in document_controls(state) {
        let island = tool_island(control.command);
        if density.prints_labels()
            && previous_island.is_some()
            && island.is_some()
            && previous_island != island
        {
            group = group.with_child(separator(&format!(
                "electronics.toolbar.island-separator.{}",
                match island.expect("a tool control has an island") {
                    ToolIsland::Selection => "selection",
                    ToolIsland::Connection => "connection",
                    ToolIsland::Placement => "placement",
                    ToolIsland::Operations => "operations",
                }
            )));
        }
        let engaged = match control.kind {
            ControlKind::Tool => tool_is_engaged(control.command, state.tool),
            _ => false,
        };
        group = group.with_child(control_button(palette, &control, density, height, engaged));
        previous_island = island.or(previous_island);
    }
    group
}

/// The view and history groups, anchored to the right corner of the band.
///
/// It is one flexible row so a single layout contract serves both the one-row
/// and the stacked densities: inside a row it grows horizontally and right-aligns
/// its children, inside the stacked column it simply fills the reserved track.
fn view_cluster(
    palette: StudioUiPalette,
    state: &ToolbarState,
    density: ToolbarDensity,
    height: f32,
) -> UiNode {
    let mut view = group_row("electronics.toolbar.cluster.view", height);
    for (control, latched) in view_controls(state) {
        view = view.with_child(control_button(palette, &control, density, height, latched));
    }
    let mut history = group_row("electronics.toolbar.cluster.history", height);
    for control in history_controls(state) {
        history = history.with_child(control_button(palette, &control, density, height, false));
    }

    UiNode::new("electronics.toolbar.right", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar-group")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::End,
            gap: TOOLBAR_GAP,
            min_size: [RIGHT_GROUP_MIN_WIDTH, height],
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, height],
            width_mode: UiSizeMode::Fill,
            // Only a one-row band has horizontal free space to absorb.
            grow: f32::from(density.is_single_row()),
            ..UiLayout::default()
        })
        .with_child(view)
        .with_child(separator("electronics.toolbar.view-separator"))
        .with_child(history)
}

/// One group row: intrinsic width, shared control track, no paint of its own.
fn group_row(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Toolbar)
        .with_class("electronics-toolbar-group")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: TOOLBAR_GAP,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, height],
            width_mode: UiSizeMode::FitContent,
            ..UiLayout::default()
        })
}

fn separator(id: &str) -> UiNode {
    crate::ui_atoms::UiSeparator::vertical(id, SEPARATOR_HEIGHT)
        .with_class("electronics-toolbar-separator")
}
