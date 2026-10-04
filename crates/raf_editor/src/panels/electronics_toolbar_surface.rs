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

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiEventBinding, UiEventKind, UiIcon, UiIconId,
    UiIconSize, UiStylePatch, UiStyleRuleState, UiStyleSheet, UiSurface, UiTextOverflow,
};
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode,
    UiSpacing, UiTextRole, UiTextStyle, UiTokens,
};

use crate::editor_layout::{
    electronics_toolbar_height, ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
    ELECTRONICS_TOOLBAR_STACKED_WIDTH,
};
use crate::electronics_controller::ElectronicsTool;
use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_class_rule, electronics_focus_rule,
    electronics_hover_rule, electronics_state_rule, with_alpha, DANGER_HOVER_ALPHA,
    DANGER_PRESS_ALPHA, ELECTRONICS_BORDER_WIDTH, ELECTRONICS_CONTROL_HEIGHT,
    ELECTRONICS_CORNER_RADIUS,
};
use raf_electronics::CadSurfaceKind;

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
/// Gap between two controls of the same group and around the hairline that
/// closes the group, in logical points. The separator carries the group
/// boundary, so the band keeps one rhythm instead of two.
const TOOLBAR_GAP: f32 = 3.0;
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
/// Alpha of the hover and focus wash of an engaged option, in 0..255. It is
/// derived from the accent token so an option keeps the accent hue without ever
/// using the accent as body text, which would fail contrast in the light theme.
const OPTION_HOVER_ALPHA: u8 = 92;
/// Alpha of the pressed wash of an engaged option, in 0..255.
const OPTION_ACTIVE_ALPHA: u8 = 120;
/// Opacity of a command whose prerequisite is missing, in 0..1.
const UNAVAILABLE_OPACITY: f32 = 0.4;
/// Opacity of that command while hovered or focused, in 0..1. It stays dimmer
/// than every available control so it can never read as ready.
const UNAVAILABLE_ATTENTION_OPACITY: f32 = 0.58;

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

/// Tools and selection edits of the active surface, in reading order.
///
/// `Pan` is the one tool that never prints a label. It is the only tool with a
/// permanent gesture equivalent (Space+drag, middle drag and right drag, all
/// stated in its own tooltip and in the canvas tool hint) and its four-way glyph
/// is a universal convention. Paying for its label is what pushed the Spanish
/// PCB row past the canvas of the default 1600-point window, which would have
/// cost the whole band its labels at the default size.
fn document_controls(state: &ToolbarState) -> Vec<ToolbarControl> {
    let mut controls = vec![
        ToolbarControl::tool(
            "electronics.select",
            UiIconId::Select,
            Some("electronics.toolbar.select"),
        ),
        ToolbarControl::tool("electronics.pan", UiIconId::Move, None),
    ];
    match state.surface {
        CadSurfaceKind::Schematic => {
            controls.push(ToolbarControl::tool(
                "electronics.wire",
                UiIconId::Wire,
                Some("electronics.toolbar.wire"),
            ));
            controls.push(ToolbarControl::tool(
                "electronics.place",
                UiIconId::Add,
                Some("electronics.toolbar.place"),
            ));
        }
        CadSurfaceKind::Pcb => {
            controls.push(ToolbarControl::tool(
                "electronics.route",
                UiIconId::Route,
                Some("electronics.toolbar.route"),
            ));
            controls.push(ToolbarControl::tool(
                "electronics.board-outline",
                UiIconId::BoardOutline,
                None,
            ));
            controls.push(ToolbarControl::tool(
                "electronics.place",
                UiIconId::Add,
                Some("electronics.toolbar.place"),
            ));
            // Synchronizing footprints is a one-way projection of the schematic
            // onto the board, not a view action, so it belongs with the tools
            // even though its glyph reads like a refresh. Entering the PCB
            // surface already projects it, which is why it stays a glyph: it is
            // the least frequent command of the band.
            controls.push(ToolbarControl::command(
                "electronics.pcb.sync",
                UiIconId::Refresh,
                None,
                true,
            ));
        }
    }
    controls.push(ToolbarControl::command(
        "electronics.rotate",
        UiIconId::Rotate,
        None,
        state.can_rotate,
    ));
    controls.push(
        ToolbarControl::command("edit.delete", UiIconId::Trash, None, state.has_selection)
            .destructive(),
    );
    controls
}

/// Controls that only change how the document is presented or navigated,
/// each with the latched value the host reported.
fn view_controls(state: &ToolbarState) -> Vec<(ToolbarControl, bool)> {
    vec![
        (
            ToolbarControl::option(
                "electronics.grid.toggle",
                UiIconId::Grid,
                Some("electronics.toolbar.grid"),
            ),
            state.grid_visible,
        ),
        (
            // The family has no magnet glyph. `Lock` reads as "locked to the
            // grid", which is what snapping does, and no other Electronics
            // command uses it. The glyph is the weakest link of this band and
            // the family needs a magnet before it can be replaced.
            ToolbarControl::option(
                "electronics.snap.toggle",
                UiIconId::Lock,
                Some("electronics.toolbar.snap"),
            ),
            state.snap_enabled,
        ),
        (
            ToolbarControl::option(
                "electronics.labels.toggle",
                UiIconId::Eye,
                Some("electronics.toolbar.labels"),
            ),
            state.labels_visible,
        ),
        // `Focus` is a crosshair, not a frame, so fit is the glyph of this band
        // that describes a camera action least literally. It keeps the `F`
        // shortcut, and the family needs a real frame glyph to replace it.
        (
            ToolbarControl::command("electronics.fit", UiIconId::Focus, None, true),
            false,
        ),
        (
            ToolbarControl::command("electronics.zoom-out", UiIconId::ZoomOut, None, true),
            false,
        ),
        (
            ToolbarControl::command("electronics.zoom-in", UiIconId::ZoomIn, None, true),
            false,
        ),
    ]
}

/// Controls that step through the session history.
fn history_controls(state: &ToolbarState) -> Vec<ToolbarControl> {
    vec![
        ToolbarControl::command("edit.undo", UiIconId::Undo, None, state.can_undo),
        ToolbarControl::command("edit.redo", UiIconId::Redo, None, state.can_redo),
    ]
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
    let tokens = palette.tokens();
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
    group = group.with_child(document_segment(palette, state, density, height));
    for control in document_controls(state) {
        let engaged = match control.kind {
            ControlKind::Tool => tool_is_engaged(control.command, state.tool),
            _ => false,
        };
        group = group.with_child(control_button(palette, &control, density, height, engaged));
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

/// The schematic/PCB selector.
///
/// It is the only bordered container of the band, so "which document am I
/// editing" cannot be mistaken for a tool, and its engaged option uses the pale
/// selection wash because the solid accent is reserved for the tool in hand.
fn document_segment(
    palette: StudioUiPalette,
    state: &ToolbarState,
    density: ToolbarDensity,
    height: f32,
) -> UiNode {
    let option_height = height - 2.0 * (SEGMENT_INSET + ELECTRONICS_BORDER_WIDTH);
    let options = [
        (
            ToolbarControl::segment(
                "electronics.mode.schematic",
                UiIconId::Schematic,
                Some("electronics.toolbar.schematic"),
            ),
            CadSurfaceKind::Schematic,
        ),
        (
            ToolbarControl::segment(
                "electronics.mode.pcb",
                UiIconId::Pcb,
                Some("electronics.toolbar.pcb"),
            ),
            CadSurfaceKind::Pcb,
        ),
    ];
    let mut segment = UiNode::new("electronics.toolbar.segment", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar-segment")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: SEGMENT_GAP,
            padding: UiSpacing::same(SEGMENT_INSET),
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, height],
            width_mode: UiSizeMode::FitContent,
            ..UiLayout::default()
        });
    for (control, kind) in &options {
        segment = segment.with_child(control_button(
            palette,
            control,
            density,
            option_height,
            state.surface == *kind,
        ));
    }
    segment
}

/// Builds one control of the band.
///
/// A labelled control never combines a glyph and text: the compositor reserves
/// the glyph track outside the measured text track, so a control carrying both
/// would print its label past its own box and crowd the next control. The
/// labelled density therefore prints the label alone and the glyph densities
/// print the glyph alone.
fn control_button(
    palette: StudioUiPalette,
    control: &ToolbarControl,
    density: ToolbarDensity,
    height: f32,
    engaged: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let unavailable = control.available.is_some_and(|available| !available);
    let labelled = density.prints_labels() && control.needs_label();
    let action = tooltip_key(control.command);
    let tooltip = if unavailable {
        unavailable_reason_key(control.command)
    } else {
        action
    };
    // The engaged tool is the only control whose glyph sits on the accent fill,
    // so it is the only one that needs the contrasting page color.
    let on_accent = engaged && control.kind == ControlKind::Tool;
    let glyph_color = if on_accent {
        tokens.background
    } else if unavailable || !engaged {
        tokens.text_muted
    } else {
        tokens.text
    };
    let label_color = if unavailable {
        tokens.text_muted
    } else if on_accent {
        tokens.background
    } else {
        tokens.text
    };

    let mut node = UiNode::new(control.id(), UiNodeKind::Button)
        .with_class(CLASS_BUTTON)
        .with_layout(if labelled {
            UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                padding: UiSpacing::xy(8.0, 2.0),
                // The horizontal minimum must stay at zero for a content-sized
                // row child: any non-zero `min_size`/`basis` makes the parent
                // advance this node by that constant while the node still paints
                // at its measured text width, so the next sibling lands on top
                // of it. The atlas-driven intrinsic is what both agree on.
                min_size: [0.0, height],
                height_mode: UiSizeMode::Fixed,
                basis: [0.0, height],
                width_mode: UiSizeMode::FitContent,
                ..UiLayout::default()
            }
            .with_text_safe_area(true)
        } else {
            UiLayout::fixed(TOOLBAR_ICON_BUTTON_WIDTH, height)
        })
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(action)
        .with_accessibility_role(match control.kind {
            ControlKind::Option => UiAccessibilityRole::Checkbox,
            ControlKind::Tool | ControlKind::Segment | ControlKind::Command => {
                UiAccessibilityRole::Button
            }
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, control.command));

    if labelled {
        node = node
            .with_text_key(control.label.expect("a labelled control has a label"))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Button,
                size_px: 11.0,
                line_height_px: 15.0,
                // Medium keeps the labelled controls readable without letting
                // the whole band carry the same weight as its one accent fill.
                weight: UiFontWeight::Medium,
                color: label_color,
                inherit_color: false,
            });
    } else {
        // The glyph carries the state when there is no label: muted at rest, the
        // contrasting page color on the engaged tool and the body color on an
        // engaged option, so the state is never color-only.
        node = node.with_icon(
            UiIcon::new(control.icon)
                .with_size(UiIconSize::Small)
                .with_tint(glyph_color),
        );
    }

    if engaged {
        node = node.with_class(match control.kind {
            ControlKind::Tool => CLASS_TOOL_ACTIVE,
            ControlKind::Segment | ControlKind::Option | ControlKind::Command => CLASS_OPTION_ON,
        });
    }
    if unavailable {
        // A disabled node is skipped by hit testing, so it could neither show
        // the reason nor be reached by keyboard. The control therefore stays
        // interactive, states its prerequisite in its tooltip and accessible
        // description, and every unavailable command is a no-op in the
        // controller.
        node = node
            .with_class(CLASS_UNAVAILABLE)
            .with_accessibility_description_key(unavailable_reason_key(control.command));
    } else if control.danger {
        node = node.with_class(CLASS_DANGER);
    }

    match control.kind {
        ControlKind::Option => {
            node = node.with_accessibility_checked(engaged);
        }
        ControlKind::Tool | ControlKind::Segment => {
            node = node.with_accessibility_selected(engaged);
        }
        ControlKind::Command => {}
    }
    node
}

/// True when `command` is the tool currently in the user's hand.
fn tool_is_engaged(command: &str, tool: ElectronicsTool) -> bool {
    matches!(
        (command, tool),
        ("electronics.select", ElectronicsTool::Select)
            | ("electronics.pan", ElectronicsTool::Pan)
            | ("electronics.wire", ElectronicsTool::Wire)
            | ("electronics.route", ElectronicsTool::Route)
            | ("electronics.place", ElectronicsTool::Place)
            | ("electronics.board-outline", ElectronicsTool::BoardOutline)
    )
}

fn separator(id: &str) -> UiNode {
    crate::ui_atoms::UiSeparator::vertical(id, SEPARATOR_HEIGHT)
        .with_class("electronics-toolbar-separator")
}

/// Localized description of what a control does.
fn tooltip_key(command: &str) -> &'static str {
    match command {
        "electronics.mode.schematic" => "electronics.tooltip.mode.schematic",
        "electronics.mode.pcb" => "electronics.tooltip.mode.pcb",
        "electronics.select" => "electronics.tooltip.select",
        "electronics.pan" => "electronics.tooltip.pan",
        "electronics.wire" => "electronics.tooltip.wire",
        "electronics.place" => "electronics.tooltip.place",
        "electronics.route" => "electronics.tooltip.route",
        "electronics.board-outline" => "electronics.tooltip.board_outline",
        "electronics.pcb.sync" => "electronics.tooltip.pcb_sync",
        "electronics.fit" => "electronics.tooltip.fit",
        "electronics.zoom-out" => "electronics.tooltip.zoom_out",
        "electronics.zoom-in" => "electronics.tooltip.zoom_in",
        "electronics.grid.toggle" => "electronics.tooltip.grid",
        "electronics.snap.toggle" => "electronics.tooltip.snap",
        "electronics.labels.toggle" => "electronics.tooltip.labels",
        "electronics.rotate" => "electronics.tooltip.rotate",
        "edit.delete" => "electronics.tooltip.delete",
        "edit.undo" => "electronics.tooltip.undo",
        "edit.redo" => "electronics.tooltip.redo",
        _ => "electronics.tooltip.command",
    }
}

/// Localized reason a control cannot run yet.
///
/// An unavailable control keeps the action description as its accessible name
/// and states the missing prerequisite here, so the tooltip is self-contained
/// and assistive technology announces both.
fn unavailable_reason_key(command: &str) -> &'static str {
    match command {
        "electronics.rotate" => "electronics.tooltip.rotate.unavailable",
        "edit.delete" => "electronics.tooltip.delete.unavailable",
        "edit.undo" => "electronics.tooltip.undo.unavailable",
        "edit.redo" => "electronics.tooltip.redo.unavailable",
        _ => "electronics.tooltip.command",
    }
}

fn style_sheet(tokens: &UiTokens) -> UiStyleSheet {
    let mut rules = vec![
        electronics_class_rule(
            "electronics-toolbar",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        electronics_state_rule(
            UiStyleRuleState::Always,
            "electronics-toolbar-separator",
            UiStylePatch {
                fill: Some(with_alpha(tokens.border, 120)),
                border: Some([0, 0, 0, 0]),
                text: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(0.0),
                opacity: Some(0.65),
                ..UiStylePatch::default()
            },
        ),
        // The document segment is the only bordered container of the band, which
        // is what separates "which document" from "which tool" without needing
        // another separator or another color.
        electronics_class_rule(
            "electronics-toolbar-segment",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        // Every control shares one resting treatment: flat, quiet and
        // borderless, so group boundaries are carried by the hairlines and the
        // band corners instead of sixteen competing outlines.
        electronics_state_rule(
            UiStyleRuleState::Always,
            CLASS_BUTTON,
            UiStylePatch {
                fill: Some([0, 0, 0, 0]),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text_muted),
                border_width: Some(0.0),
                radius: Some(ELECTRONICS_CORNER_RADIUS),
                ..UiStylePatch::default()
            },
        ),
    ];
    rules.push(electronics_hover_rule(CLASS_BUTTON, *tokens));
    rules.push(electronics_focus_rule(CLASS_BUTTON, *tokens));
    rules.push(electronics_active_rule(CLASS_BUTTON, *tokens));

    // The tool in hand: the only saturated accent fill of the band. Hover keeps
    // the fill so a pointer crossing an engaged tool does not read as a state
    // change, and focus brightens it because the resting border already carries
    // the accent edge the shared focus ring would use.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        CLASS_TOOL_ACTIVE,
        UiStylePatch {
            fill: Some(tokens.accent),
            border: Some(tokens.accent_hot),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            radius: Some(ELECTRONICS_CORNER_RADIUS),
            text: Some(tokens.background),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        CLASS_TOOL_ACTIVE,
        UiStylePatch {
            fill: Some(tokens.accent),
            border: Some(tokens.focus),
            text: Some(tokens.background),
            ..UiStylePatch::default()
        },
    ));
    for state in [UiStyleRuleState::Focused, UiStyleRuleState::Active] {
        rules.push(electronics_state_rule(
            state,
            CLASS_TOOL_ACTIVE,
            UiStylePatch {
                fill: Some(tokens.accent_hot),
                border: Some(tokens.focus),
                ..UiStylePatch::default()
            },
        ));
    }

    // An engaged option, and the engaged document segment, use the selection
    // token: the same hue as the active tool but a pale wash with a neutral edge,
    // so the two states are told apart by geometry and not by hue alone. The
    // neutral resting border is also what keeps the shared focus ring visible on
    // top of it.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        CLASS_OPTION_ON,
        UiStylePatch {
            fill: Some(tokens.selection),
            border: Some(tokens.border),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            radius: Some(ELECTRONICS_CORNER_RADIUS),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    for (state, alpha) in [
        (UiStyleRuleState::Hovered, OPTION_HOVER_ALPHA),
        (UiStyleRuleState::Focused, OPTION_HOVER_ALPHA),
        (UiStyleRuleState::Active, OPTION_ACTIVE_ALPHA),
    ] {
        rules.push(electronics_state_rule(
            state,
            CLASS_OPTION_ON,
            UiStylePatch {
                fill: Some(with_alpha(tokens.accent, alpha)),
                border: Some(tokens.accent_hot),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        ));
    }
    rules.push(electronics_focus_rule(CLASS_OPTION_ON, *tokens));

    // A command whose prerequisite is missing stays visible and dimmed, keeps a
    // hover and a focus affordance so its reason can be read, and never reaches
    // the contrast of an available control.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            fill: Some([0, 0, 0, 0]),
            border: Some([0, 0, 0, 0]),
            border_width: Some(0.0),
            text: Some(tokens.text_muted),
            opacity: Some(UNAVAILABLE_OPACITY),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            opacity: Some(UNAVAILABLE_ATTENTION_OPACITY),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Focused,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            border: Some(tokens.focus),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            opacity: Some(UNAVAILABLE_ATTENTION_OPACITY),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Active,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            border: Some([0, 0, 0, 0]),
            border_width: Some(0.0),
            opacity: Some(UNAVAILABLE_ATTENTION_OPACITY),
            ..UiStylePatch::default()
        },
    ));

    // The destructive action keeps the danger token while it is available and the
    // shared warm focus ring for keyboard focus.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        CLASS_DANGER,
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_HOVER_ALPHA)),
            border: Some(tokens.danger),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Active,
        CLASS_DANGER,
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_PRESS_ALPHA)),
            border: Some(tokens.danger),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule(CLASS_DANGER, *tokens));
    UiStyleSheet { rules }
}

#[cfg(test)]
mod tests {
    use raf_render::api_graphic_basic::ui_surface::UiSurfaceFrame;
    use raf_ui::{UiRect, UiStyleSelector};

    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;

    /// Canvas width that resolves the labelled density.
    const LABELLED_WIDTH: f32 = 1600.0;
    /// Canvas width that resolves the single glyph-only row.
    const ICON_WIDTH: f32 = 700.0;
    /// Narrowest canvas the Electronics layout allows, and therefore the worst
    /// case of the stacked density.
    const NARROW_WIDTH: f32 = 360.0;

    /// Advance of each labeled control in the Spanish catalog, in logical points,
    /// already including the 8+8 points of padding the compositor forces on every
    /// labelled control. Measured from the bundled Ubuntu Medium face at 11px
    /// against `crates/raf_core/locales/es.json`, which is the longest catalog of
    /// the two.
    const MEASURED_ES_LABELS: [(&str, f32); 9] = [
        ("electronics.toolbar.schematic", 81.5),
        ("electronics.toolbar.pcb", 37.1),
        ("electronics.toolbar.select", 74.5),
        ("electronics.toolbar.wire", 45.0),
        ("electronics.toolbar.place", 54.9),
        ("electronics.toolbar.route", 48.3),
        ("electronics.toolbar.grid", 70.1),
        ("electronics.toolbar.snap", 48.9),
        ("electronics.toolbar.labels", 64.8),
    ];

    fn build(kind: CadSurfaceKind, width: f32) -> UiSurface {
        build_with(
            ToolbarState::available(kind, ElectronicsTool::Select),
            width,
        )
    }

    fn build_unavailable(kind: CadSurfaceKind, width: f32) -> UiSurface {
        build_with(
            ToolbarState::nothing_available(kind, ElectronicsTool::Select),
            width,
        )
    }

    fn build_with(state: ToolbarState, width: f32) -> UiSurface {
        build_electronics_toolbar_surface(
            StudioUiPalette::IndustrialDark,
            state.surface,
            state.tool,
            state.grid_visible,
            state.labels_visible,
            state.snap_enabled,
            state.can_undo,
            state.can_redo,
            state.has_selection,
            state.can_rotate,
            width,
        )
    }

    fn frame(surface: &UiSurface, width: f32) -> UiSurfaceFrame {
        surface.build_frame(
            width.max(1.0) as u32,
            electronics_toolbar_height(width).max(1.0) as u32,
            [0, 0, 0, 255],
        )
    }

    fn rect_of(frame: &UiSurfaceFrame, id: &str) -> UiRect {
        frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == id)
            .unwrap_or_else(|| panic!("{id} is missing from the frame"))
            .rect
    }

    fn rule_for<'a>(
        surface: &'a UiSurface,
        class: &str,
        state: UiStyleRuleState,
    ) -> &'a UiStylePatch {
        surface
            .style_sheet
            .rules
            .iter()
            .find(|rule| {
                rule.selector == UiStyleSelector::Class(class.to_string()) && rule.state == state
            })
            .map(|rule| &rule.patch)
            .unwrap_or_else(|| panic!("{class} has no {state:?} rule"))
    }

    fn class_of(surface: &UiSurface, id: &str, class: &str) -> bool {
        surface
            .root
            .find(id)
            .unwrap_or_else(|| panic!("{id} is missing"))
            .classes
            .iter()
            .any(|owned| owned == class)
    }

    fn collect_glyphs(node: &UiNode, seen: &mut Vec<(String, UiIconId)>) {
        if let Some(icon) = node.icon {
            let command = node
                .id
                .strip_prefix("electronics.toolbar.")
                .unwrap_or(node.id.as_str())
                .to_string();
            seen.push((command, icon.id));
        }
        for child in &node.children {
            collect_glyphs(child, seen);
        }
    }

    /// Width of one control at the labelled density, from the measured table.
    fn labelled_width(label: &str) -> f32 {
        MEASURED_ES_LABELS
            .iter()
            .find(|(key, _)| *key == label)
            .map(|(_, width)| *width)
            .unwrap_or(TOOLBAR_ICON_BUTTON_WIDTH)
    }

    /// Width of one control at one density, from the measured table.
    fn control_width(labelled: bool, label: Option<&str>) -> f32 {
        match (labelled, label) {
            (true, Some(label)) => labelled_width(label),
            _ => TOOLBAR_ICON_BUTTON_WIDTH,
        }
    }

    /// Span of the document group at one density: the surface segment, the tools
    /// of the active surface and the two selection edits.
    fn document_span(kind: CadSurfaceKind, labelled: bool) -> f32 {
        let segment = if labelled {
            labelled_width("electronics.toolbar.schematic")
                + labelled_width("electronics.toolbar.pcb")
                + SEGMENT_GAP
                + 2.0 * SEGMENT_INSET
        } else {
            2.0 * TOOLBAR_ICON_BUTTON_WIDTH + SEGMENT_GAP + 2.0 * SEGMENT_INSET
        };
        let mut widths = vec![
            segment,
            control_width(labelled, Some("electronics.toolbar.select")),
            // Pan stays a glyph: see `document_controls`.
            control_width(labelled, None),
        ];
        match kind {
            CadSurfaceKind::Schematic => {
                widths.push(control_width(labelled, Some("electronics.toolbar.wire")));
                widths.push(control_width(labelled, Some("electronics.toolbar.place")));
            }
            CadSurfaceKind::Pcb => {
                widths.push(control_width(labelled, Some("electronics.toolbar.route")));
                widths.push(control_width(labelled, Some("electronics.toolbar.place")));
                widths.push(control_width(labelled, None));
                widths.push(control_width(labelled, None));
            }
        }
        // Rotate and Delete are glyph-only in every density.
        widths.push(control_width(labelled, None));
        widths.push(control_width(labelled, None));
        widths.iter().sum::<f32>() + TOOLBAR_GAP * (widths.len() - 1) as f32
    }

    /// Span of the view group at one density: the three latched options plus the
    /// fit and zoom commands.
    fn view_span(labelled: bool) -> f32 {
        let widths = [
            control_width(labelled, Some("electronics.toolbar.grid")),
            control_width(labelled, Some("electronics.toolbar.snap")),
            control_width(labelled, Some("electronics.toolbar.labels")),
            control_width(labelled, None),
            control_width(labelled, None),
            control_width(labelled, None),
        ];
        widths.iter().sum::<f32>() + TOOLBAR_GAP * (widths.len() - 1) as f32
    }

    /// Span of the history group at one density: undo and redo, both glyph-only.
    fn history_span() -> f32 {
        2.0 * TOOLBAR_ICON_BUTTON_WIDTH + TOOLBAR_GAP
    }

    /// Authored width of one hairline, read from the document instead of being
    /// re-declared here.
    fn separator_width(surface: &UiSurface, id: &str) -> f32 {
        surface
            .root
            .find(id)
            .unwrap_or_else(|| panic!("{id} is missing"))
            .layout
            .basis[0]
    }

    /// Band a single-row density needs: both groups, both hairlines, the four
    /// gaps between the three row children and the band padding.
    fn single_row_span(surface: &UiSurface, kind: CadSurfaceKind, labelled: bool) -> f32 {
        document_span(kind, labelled)
            + view_span(labelled)
            + history_span()
            + separator_width(surface, "electronics.toolbar.document-separator")
            + separator_width(surface, "electronics.toolbar.view-separator")
            + 4.0 * TOOLBAR_GAP
            + 2.0 * TOOLBAR_PADDING_X
    }

    /// Which row of the stacked density a span describes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum StackedRow {
        /// The document group, alone on the first row.
        Document,
        /// The view and history groups, sharing the second row.
        View,
    }

    /// Band a stacked row needs: the row content, its own hairline and gaps and
    /// the band padding.
    fn stacked_row_span(
        surface: &UiSurface,
        kind: CadSurfaceKind,
        row: StackedRow,
        labelled: bool,
    ) -> f32 {
        let (content, hairline, gaps) = match row {
            // The document row is a single group.
            StackedRow::Document => (document_span(kind, labelled), 0.0, 0.0),
            // The view row holds the view group, the hairline and the history
            // group, so it pays for the hairline and two extra gaps.
            StackedRow::View => (
                view_span(labelled) + history_span(),
                separator_width(surface, "electronics.toolbar.view-separator"),
                2.0 * TOOLBAR_GAP,
            ),
        };
        content + hairline + gaps + 2.0 * TOOLBAR_PADDING_X
    }

    #[test]
    fn every_control_declares_a_role_a_label_and_a_tooltip() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
                let surface = build(kind, width);
                assert_declared(&surface, kind, width);
            }
        }
    }

    fn assert_declared(surface: &UiSurface, kind: CadSurfaceKind, width: f32) {
        fn walk(node: &UiNode, kind: CadSurfaceKind, width: f32) {
            if node.kind == UiNodeKind::Button {
                assert!(
                    node.tooltip_key.is_some(),
                    "{} needs a tooltip at every density",
                    node.id
                );
                assert!(
                    node.accessibility_label_key.is_some(),
                    "{} needs an accessible name",
                    node.id
                );
                assert_ne!(
                    node.accessibility_role,
                    UiAccessibilityRole::Generic,
                    "{} needs a role",
                    node.id
                );
                assert!(
                    node.text_key.is_some() || node.icon.is_some(),
                    "{} must say what it does with a label or a glyph, {kind:?} at {width}",
                    node.id
                );
                assert!(
                    !(node.text_key.is_some() && node.icon.is_some()),
                    "{} must not combine a glyph and a label",
                    node.id
                );
            }
            for child in &node.children {
                walk(child, kind, width);
            }
        }
        walk(&surface.root, kind, width);
    }

    #[test]
    fn the_labelled_density_prints_the_controls_whose_glyph_does_not_speak() {
        let surface = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
        for command in [
            "electronics.mode.schematic",
            "electronics.mode.pcb",
            "electronics.select",
            "electronics.route",
            "electronics.place",
            "electronics.grid.toggle",
            "electronics.snap.toggle",
            "electronics.labels.toggle",
        ] {
            let node = surface
                .root
                .find(&format!("electronics.toolbar.{command}"))
                .unwrap_or_else(|| panic!("{command} is missing"));
            assert!(node.text_key.is_some(), "{command} must print its label");
            assert!(node.icon.is_none(), "{command} must not carry a glyph");
            assert_eq!(
                node.layout.width_mode,
                UiSizeMode::FitContent,
                "{command} must measure its localized label"
            );
        }
        // A glyph-only control is deliberate, not an omission: each of these has a
        // universal glyph or a gesture equivalent that its tooltip states.
        for command in [
            "electronics.pan",
            "electronics.board-outline",
            "electronics.pcb.sync",
            "electronics.rotate",
            "edit.delete",
            "edit.undo",
            "edit.redo",
        ] {
            let node = surface
                .root
                .find(&format!("electronics.toolbar.{command}"))
                .unwrap_or_else(|| panic!("{command} is missing"));
            assert!(node.text_key.is_none(), "{command} stays a glyph");
            assert!(node.icon.is_some(), "{command} must keep its glyph");
            assert_eq!(node.layout.basis[0], TOOLBAR_ICON_BUTTON_WIDTH);
        }
    }

    #[test]
    fn the_glyph_densities_keep_a_glyph_and_a_tooltip_for_every_control() {
        for width in [ICON_WIDTH, NARROW_WIDTH] {
            for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
                let surface = build(kind, width);
                fn walk(node: &UiNode) {
                    if node.kind == UiNodeKind::Button {
                        assert!(node.text_key.is_none(), "{} hid its glyph", node.id);
                        assert!(node.icon.is_some(), "{} lost its glyph", node.id);
                        assert!(node.tooltip_key.is_some());
                        assert_eq!(node.layout.width_mode, UiSizeMode::Fixed);
                    }
                    for child in &node.children {
                        walk(child);
                    }
                }
                walk(&surface.root);
            }
        }
    }

    #[test]
    fn the_band_never_scrolls_and_clips_instead() {
        // A horizontal scroll owner has no reachable thumb in this compositor, so
        // a scrollable band would hide controls instead of moving them.
        for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
            for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
                let surface = build(kind, width);
                assert!(surface.root.control.scroll_axis().is_none());
                assert_eq!(surface.root.layout.overflow, UiOverflow::Clip);
                assert!(!surface.root.interactive, "the band floor is not a target");
            }
        }
    }

    #[test]
    fn the_stacked_density_puts_the_view_row_below_the_document_row() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let surface = build(kind, NARROW_WIDTH);
            assert_eq!(surface.root.layout.flow, UiFlow::Column);
            assert_eq!(
                surface.root.layout.basis[1],
                crate::editor_layout::ELECTRONICS_TOOLBAR_STACKED_HEIGHT
            );
            assert_eq!(surface.root.children.len(), 2);
            let resolved = frame(&surface, NARROW_WIDTH);
            let document = rect_of(&resolved, "electronics.toolbar.cluster.document");
            let view = rect_of(&resolved, "electronics.toolbar.right");
            assert!(
                document.bottom() <= view.y + f32::EPSILON,
                "{kind:?}: the document row ends at {} and the view row starts at {}",
                document.bottom(),
                view.y
            );
            assert_eq!(document.height, TOOLBAR_BUTTON_HEIGHT);
            assert_eq!(view.height, TOOLBAR_BUTTON_HEIGHT);
        }
    }

    #[test]
    fn the_single_row_densities_keep_both_groups_on_one_track() {
        for width in [ICON_WIDTH, LABELLED_WIDTH] {
            for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
                let surface = build(kind, width);
                assert_eq!(surface.root.layout.flow, UiFlow::Row);
                assert_eq!(surface.root.children.len(), 3);
                assert_eq!(
                    surface.root.layout.basis[1],
                    crate::editor_layout::ELECTRONICS_TOOLBAR_HEIGHT
                );
                let resolved = frame(&surface, width);
                let document = rect_of(&resolved, "electronics.toolbar.cluster.document");
                let view = rect_of(&resolved, "electronics.toolbar.cluster.view");
                let history = rect_of(&resolved, "electronics.toolbar.cluster.history");
                assert!(
                    document.right() <= view.x + f32::EPSILON,
                    "{kind:?} at {width}: the groups overlap"
                );
                assert!(view.right() <= history.x + f32::EPSILON);
                assert!(
                    (document.y - view.y).abs() < 0.01,
                    "{kind:?} at {width}: the groups left the shared track"
                );
                assert!((document.y - history.y).abs() < 0.01);
            }
        }
    }

    #[test]
    fn the_labelled_row_fits_its_breakpoint_with_the_measured_span() {
        let mut worst = 0.0_f32;
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let span = single_row_span(&build(kind, LABELLED_WIDTH), kind, true);
            assert!(
                span <= ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
                "{kind:?} labelled row needs {span} points, the breakpoint is {ELECTRONICS_TOOLBAR_LABELLED_WIDTH}"
            );
            worst = worst.max(span);
        }
        // A breakpoint far above the worst measured span would push labelled
        // controls into the glyph-only row for no reason.
        assert!(
            ELECTRONICS_TOOLBAR_LABELLED_WIDTH - worst <= 60.0,
            "the worst labelled row needs {worst} points, the breakpoint leaves {} unused",
            ELECTRONICS_TOOLBAR_LABELLED_WIDTH - worst
        );
    }

    #[test]
    fn the_glyph_row_fits_its_breakpoint_with_the_measured_span() {
        let mut worst = 0.0_f32;
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let span = single_row_span(&build(kind, ICON_WIDTH), kind, false);
            assert!(
                span <= ELECTRONICS_TOOLBAR_STACKED_WIDTH,
                "{kind:?} glyph row needs {span} points, the breakpoint is {ELECTRONICS_TOOLBAR_STACKED_WIDTH}"
            );
            worst = worst.max(span);
        }
        assert!(
            ELECTRONICS_TOOLBAR_STACKED_WIDTH - worst <= 40.0,
            "the worst glyph row needs {worst} points, the breakpoint leaves {} unused",
            ELECTRONICS_TOOLBAR_STACKED_WIDTH - worst
        );
    }

    #[test]
    fn every_stacked_row_fits_the_narrowest_canvas_the_editor_allows() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let surface = build(kind, NARROW_WIDTH);
            for row in [StackedRow::Document, StackedRow::View] {
                let span = stacked_row_span(&surface, kind, row, false);
                assert!(
                    span <= NARROW_WIDTH,
                    "{kind:?} {row:?} row needs {span} of a {NARROW_WIDTH} point canvas"
                );
            }
        }
    }

    #[test]
    fn the_reserved_band_height_matches_the_density() {
        for (width, density) in [
            (NARROW_WIDTH, ToolbarDensity::Stacked),
            (ICON_WIDTH, ToolbarDensity::Icon),
            (LABELLED_WIDTH, ToolbarDensity::Labelled),
        ] {
            let surface = build(CadSurfaceKind::Schematic, width);
            assert_eq!(ToolbarDensity::for_width(width), density, "at {width}");
            let band = electronics_toolbar_height(width);
            assert_eq!(surface.root.layout.basis[1], band, "at {width}");
            let rows = density.rows() as f32;
            let content = rows * TOOLBAR_BUTTON_HEIGHT + (rows - 1.0) * TOOLBAR_ROW_GAP;
            assert_eq!(
                density.padding_y(width) * 2.0 + content,
                band,
                "the rows and their padding must consume the whole band at {width}"
            );
            assert_eq!(TOOLBAR_BUTTON_HEIGHT, ELECTRONICS_CONTROL_HEIGHT);
            assert!(density.padding_y(width) >= 4.0, "at {width}");
        }
    }

    #[test]
    fn the_density_breakpoints_are_ordered() {
        assert!(ELECTRONICS_TOOLBAR_STACKED_WIDTH < ELECTRONICS_TOOLBAR_LABELLED_WIDTH);
        assert_eq!(
            ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_STACKED_WIDTH - 1.0),
            ToolbarDensity::Stacked
        );
        assert_eq!(
            ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_STACKED_WIDTH),
            ToolbarDensity::Icon
        );
        assert_eq!(
            ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_LABELLED_WIDTH - 1.0),
            ToolbarDensity::Icon
        );
        assert_eq!(
            ToolbarDensity::for_width(ELECTRONICS_TOOLBAR_LABELLED_WIDTH),
            ToolbarDensity::Labelled
        );
    }

    #[test]
    fn the_engaged_tool_and_an_engaged_option_do_not_share_a_treatment() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let surface = build_electronics_toolbar_surface(
                palette,
                CadSurfaceKind::Schematic,
                ElectronicsTool::Wire,
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                LABELLED_WIDTH,
            );
            let tokens = palette.tokens();
            let tool = rule_for(&surface, CLASS_TOOL_ACTIVE, UiStyleRuleState::Always);
            let option = rule_for(&surface, CLASS_OPTION_ON, UiStyleRuleState::Always);
            assert_eq!(tool.fill, Some(tokens.accent));
            // The page color is the only token that contrasts with the accent in
            // both themes, so the tool glyph stays readable on its own fill.
            assert_eq!(tool.text, Some(tokens.background));
            assert_eq!(option.fill, Some(tokens.selection));
            assert_ne!(option.fill, tool.fill);
            assert_ne!(option.text, tool.text);
            // The engaged option keeps a neutral resting edge so the shared focus
            // ring stays visible on top of it.
            assert_eq!(option.border, Some(tokens.border));
            assert_ne!(option.border, tool.border);

            assert!(class_of(
                &surface,
                "electronics.toolbar.electronics.wire",
                CLASS_TOOL_ACTIVE
            ));
            assert!(!class_of(
                &surface,
                "electronics.toolbar.electronics.grid.toggle",
                CLASS_TOOL_ACTIVE
            ));
            assert!(class_of(
                &surface,
                "electronics.toolbar.electronics.grid.toggle",
                CLASS_OPTION_ON
            ));
            assert!(class_of(
                &surface,
                "electronics.toolbar.electronics.mode.schematic",
                CLASS_OPTION_ON
            ));
        }
    }

    #[test]
    fn only_the_tool_in_hand_wears_the_saturated_accent() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
                let surface =
                    build_with(ToolbarState::available(kind, ElectronicsTool::Place), width);
                let mut engaged = Vec::new();
                fn walk(node: &UiNode, engaged: &mut Vec<String>) {
                    if node.classes.iter().any(|class| class == CLASS_TOOL_ACTIVE) {
                        engaged.push(node.id.clone());
                    }
                    for child in &node.children {
                        walk(child, engaged);
                    }
                }
                walk(&surface.root, &mut engaged);
                assert_eq!(
                    engaged,
                    vec!["electronics.toolbar.electronics.place".to_string()],
                    "{kind:?} at {width} must engage exactly one tool"
                );
            }
        }
    }

    #[test]
    fn the_document_segment_is_the_only_bordered_container() {
        let surface = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
        let tokens = surface.palette.tokens();
        let segment = rule_for(
            &surface,
            "electronics-toolbar-segment",
            UiStyleRuleState::Always,
        );
        assert_eq!(segment.border, Some(tokens.border));
        assert_eq!(segment.border_width, Some(ELECTRONICS_BORDER_WIDTH));
        let button = rule_for(&surface, CLASS_BUTTON, UiStyleRuleState::Always);
        assert_eq!(button.border, Some([0, 0, 0, 0]));
        assert_eq!(button.border_width, Some(0.0));
        // The segment holds both options side by side without overlapping.
        // The gap is intentional padding, so the invariant is ordering plus a
        // non-negative gap, not a hard "touching" edge.
        let resolved = frame(&surface, LABELLED_WIDTH);
        let schematic = rect_of(&resolved, "electronics.toolbar.electronics.mode.schematic");
        let pcb = rect_of(&resolved, "electronics.toolbar.electronics.mode.pcb");
        let shell = rect_of(&resolved, "electronics.toolbar.segment");
        assert!(
          schematic.right() <= pcb.x + f32::EPSILON,
          "segment options overlap at {LABELLED_WIDTH}px: schematic={schematic:?} pcb={pcb:?} shell={shell:?}"
      );
        assert!(shell.x <= schematic.x + f32::EPSILON);
        assert!(pcb.right() <= shell.right() + f32::EPSILON);
    }

    #[test]
    fn an_unavailable_command_states_its_prerequisite_instead_of_disappearing() {
        for width in [NARROW_WIDTH, LABELLED_WIDTH] {
            let surface = build_unavailable(CadSurfaceKind::Schematic, width);
            for (command, reason) in [
                (
                    "electronics.rotate",
                    "electronics.tooltip.rotate.unavailable",
                ),
                ("edit.delete", "electronics.tooltip.delete.unavailable"),
                ("edit.undo", "electronics.tooltip.undo.unavailable"),
                ("edit.redo", "electronics.tooltip.redo.unavailable"),
            ] {
                let id = format!("electronics.toolbar.{command}");
                let node = surface
                    .root
                    .find(&id)
                    .unwrap_or_else(|| panic!("{id} is missing"));
                assert!(
                    !node.disabled,
                    "{id} must stay reachable so its reason can be read"
                );
                assert!(node.focusable, "{id} must stay in the focus order");
                assert_eq!(
                    node.tooltip_key.as_deref(),
                    Some(reason),
                    "{id} must explain why it cannot run"
                );
                assert_eq!(node.accessibility_description_key.as_deref(), Some(reason));
                assert_eq!(
                    node.accessibility_label_key.as_deref(),
                    Some(tooltip_key(command)),
                    "{id} must keep naming its action"
                );
                assert!(node.classes.iter().any(|class| class == CLASS_UNAVAILABLE));
            }
            // An available destructive action keeps the danger edge; an
            // unavailable one must not, or it would read as armed.
            assert!(!class_of(
                &surface,
                "electronics.toolbar.edit.delete",
                CLASS_DANGER
            ));
            let available = build(CadSurfaceKind::Schematic, width);
            assert!(class_of(
                &available,
                "electronics.toolbar.edit.delete",
                CLASS_DANGER
            ));
        }
    }

    #[test]
    fn every_band_class_exposes_hover_focus_and_pressed_states() {
        let surface = build(CadSurfaceKind::Pcb, LABELLED_WIDTH);
        for class in [
            CLASS_BUTTON,
            CLASS_TOOL_ACTIVE,
            CLASS_OPTION_ON,
            CLASS_UNAVAILABLE,
            CLASS_DANGER,
        ] {
            let states: Vec<UiStyleRuleState> = surface
                .style_sheet
                .rules
                .iter()
                .filter(|rule| rule.selector == UiStyleSelector::Class(class.to_string()))
                .map(|rule| rule.state)
                .collect();
            for state in [
                UiStyleRuleState::Hovered,
                UiStyleRuleState::Focused,
                UiStyleRuleState::Active,
            ] {
                assert!(
                    states.contains(&state),
                    "{class} needs a {state:?} rule, found {states:?}"
                );
            }
        }
    }

    #[test]
    fn no_two_band_commands_share_one_glyph() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            for width in [NARROW_WIDTH, ICON_WIDTH, LABELLED_WIDTH] {
                let surface = build(kind, width);
                let mut seen = Vec::new();
                collect_glyphs(&surface.root, &mut seen);
                for (index, (command, icon)) in seen.iter().enumerate() {
                    assert!(
                        !seen[index + 1..].iter().any(|(_, other)| other == icon),
                        "{kind:?} at {width}: {command} shares the {icon:?} glyph with a later command"
                    );
                }
            }
        }
    }

    #[test]
    fn the_band_passes_the_retained_layout_gate_in_both_width_directions() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            for width in [
                NARROW_WIDTH,
                ELECTRONICS_TOOLBAR_STACKED_WIDTH - 1.0,
                ELECTRONICS_TOOLBAR_STACKED_WIDTH,
                ELECTRONICS_TOOLBAR_LABELLED_WIDTH - 1.0,
                ELECTRONICS_TOOLBAR_LABELLED_WIDTH,
                1280.0,
                LABELLED_WIDTH,
            ] {
                let surface = build(kind, width);
                let diagnostics = assert_electronics_layout_gate(
                    &surface,
                    width as u32,
                    electronics_toolbar_height(width) as u32,
                );
                assert!(
                    diagnostics.focusable_regions >= 10,
                    "{kind:?} at {width} resolved {} focusable regions",
                    diagnostics.focusable_regions
                );
            }
        }
    }

    #[test]
    fn the_unavailable_band_also_passes_the_retained_layout_gate() {
        for kind in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            for width in [NARROW_WIDTH, LABELLED_WIDTH] {
                let surface = build_unavailable(kind, width);
                assert_electronics_layout_gate(
                    &surface,
                    width as u32,
                    electronics_toolbar_height(width) as u32,
                );
            }
        }
    }
}
