//! Native Electronics analysis surface, shared surface primitives, and the
//! destructive-action confirmation overlay.
//!
//! Three responsibilities live here, because the Electronics family has no
//! other module a surface builder may depend on without growing a cross-panel
//! utility file:
//!
//! 1. the retained DRC / Simulation dock body and its typed line model;
//! 2. the shared style and control recipes every other `electronics_*_surface`
//!    reuses (body text, class rules, focus ring, pressed ring, icon button);
//! 3. the delete confirmation overlay, which is a global overlay and therefore
//!    not owned by any panel.
//!
//! Everything stays declarative: motion samples arrive as values from the
//! workbench host, and the surface never keeps transient state of its own.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiScrollAxis, UiSizeMode,
    UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiSurface, UiSurfaceMaterial, UiTextOverflow, UiTextStyle, UiTokens,
};
use raf_ui::{UiAlign, UiFontWeight, UiJustify, UiTextRole};
use uuid::Uuid;

/// Retained dock tab id of the DRC analysis panel.
///
/// Mirrors the host-side constant so the surface can resolve the panel from the
/// tab the dock actually retained.
pub(crate) const ELECTRONICS_ANALYSIS_DRC_TAB: &str = "drc";
/// Retained dock tab id of the DC simulation analysis panel.
pub(crate) const ELECTRONICS_ANALYSIS_SIMULATION_TAB: &str = "simulation";

/// Command that confirms the armed destructive action.
pub const ELECTRONICS_DELETE_CONFIRM_COMMAND: &str = "electronics.delete.confirm";
/// Command that dismisses the armed destructive action.
pub const ELECTRONICS_DELETE_CANCEL_COMMAND: &str = "electronics.delete.cancel";
/// Prefix of the positional per-row command an analysis line may emit.
///
/// Resolution is by component identity today, so this surface never emits it.
/// The prefix stays because `ElectronicsAction::FocusAnalysisIssue` and the
/// controller arm that consume it are host-owned, and a host that still reports
/// a positional finding keeps a working command instead of a silent one.
pub const ELECTRONICS_ANALYSIS_FOCUS_PREFIX: &str = "electronics.analysis.focus_line";
/// Prefix of the command emitted when the backend names the component behind a
/// finding, which is the stable form of `ELECTRONICS_ANALYSIS_FOCUS_PREFIX`.
pub const ELECTRONICS_ANALYSIS_FOCUS_COMPONENT_PREFIX: &str =
    "electronics.analysis.focus_component";

/// Presented width of the delete confirmation modal, in logical points.
pub const ELECTRONICS_DELETE_MODAL_WIDTH: f32 = 360.0;
/// Presented height of the delete confirmation modal, in logical points.
///
/// The stack needs `14 + 20 + 8 + 30 + 8 + 15 + 8 + 30 + 14 = 147` points with
/// the longest localized body on two lines; the extra slack keeps a longer
/// translation from painting outside the panel background.
pub const ELECTRONICS_DELETE_MODAL_HEIGHT: f32 = 156.0;
/// Inner padding of the delete confirmation modal, in logical points.
const DELETE_MODAL_PADDING_X: f32 = 16.0;
const DELETE_MODAL_PADDING_Y: f32 = 14.0;
/// Vertical gap between the modal blocks, in logical points.
const DELETE_MODAL_GAP: f32 = 8.0;
/// Height of the modal title row, in logical points.
const DELETE_MODAL_HEADER_HEIGHT: f32 = 20.0;
/// Height of the delete confirmation footer row, in logical points.
const DELETE_MODAL_FOOTER_HEIGHT: f32 = 30.0;

/// Visual weight of one analysis dock row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsAnalysisTone {
    Normal,
    Running,
    Passed,
    Issues,
    Failed,
}

/// Which analysis dock panel a body belongs to.
///
/// The identity is resolved from the retained dock tab id, never from the
/// rendered title: the previous `title.contains("simulation")` check silently
/// rendered the Simulation tab as a DRC panel in any language whose title does
/// not contain that English word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElectronicsAnalysisPanel {
    Drc,
    Simulation,
}

impl ElectronicsAnalysisPanel {
    /// Resolves a retained dock tab id. Anything else is not an analysis panel.
    pub(crate) fn from_tab(tab: &str) -> Option<Self> {
        match tab {
            ELECTRONICS_ANALYSIS_DRC_TAB => Some(Self::Drc),
            ELECTRONICS_ANALYSIS_SIMULATION_TAB => Some(Self::Simulation),
            _ => None,
        }
    }

    /// Localized panel title key.
    pub(crate) fn title_key(self) -> &'static str {
        match self {
            Self::Drc => "electronics.analysis.drc_title",
            Self::Simulation => "electronics.analysis.simulation_title",
        }
    }

    pub(crate) fn is_simulation(self) -> bool {
        matches!(self, Self::Simulation)
    }
}

/// Semantic destination of one analysis line.
///
/// The host owns the domain target, so the surface only presents a row the
/// controller can resolve. `None` keeps the row informational.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectronicsAnalysisTarget {
    /// A component named by the finding.
    ///
    /// Identity is the only stable destination: a presented row offset moves
    /// whenever a status or stale line is added above it, and the controller
    /// resolves the report list, not the dock list.
    Component { source_id: Uuid },
}

/// One row of the analysis dock body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectronicsAnalysisLine {
    pub text: String,
    pub tone: ElectronicsAnalysisTone,
    /// Destination resolved by the host when the row is activated.
    pub target: Option<ElectronicsAnalysisTarget>,
}

impl ElectronicsAnalysisLine {
    pub fn normal(text: impl Into<String>) -> Self {
        Self::with_tone(text, ElectronicsAnalysisTone::Normal)
    }

    pub fn with_tone(text: impl Into<String>, tone: ElectronicsAnalysisTone) -> Self {
        Self {
            text: text.into(),
            tone,
            target: None,
        }
    }

    /// Row that resolves to a domain location when activated.
    pub fn with_target(
        text: impl Into<String>,
        tone: ElectronicsAnalysisTone,
        target: ElectronicsAnalysisTarget,
    ) -> Self {
        Self {
            text: text.into(),
            tone,
            target: Some(target),
        }
    }
}

/// Host-owned motion samples for the analysis body.
///
/// The workbench keeps the tweens and the reduced-motion policy; the surface
/// only reads the current values so the document stays declarative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectronicsAnalysisMotion {
    /// 1.0 once the dock panel swap settled, 0.0 at the start of the swap.
    pub panel_settle: f32,
    /// 0..1 sweep of the indeterminate progress track while an analysis runs.
    /// The host must pass 0.0 when no analysis is running.
    pub running_phase: f32,
}

impl Default for ElectronicsAnalysisMotion {
    fn default() -> Self {
        Self {
            panel_settle: 1.0,
            running_phase: 0.0,
        }
    }
}

impl ElectronicsAnalysisMotion {
    pub fn new(panel_settle: f32, running_phase: f32) -> Self {
        Self {
            panel_settle: if panel_settle.is_finite() {
                panel_settle.clamp(0.0, 1.0)
            } else {
                1.0
            },
            running_phase: if running_phase.is_finite() {
                running_phase.clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }

    /// Opacity of the body during a dock panel swap. A reduced-motion host
    /// resolves its tween to 1.0, so this is fully opaque for those users.
    fn body_opacity(self) -> f32 {
        0.72 + 0.28 * self.panel_settle.clamp(0.0, 1.0)
    }

    /// Indeterminate progress fill. It never reaches a full bar, so a running
    /// analysis cannot be mistaken for a finished one.
    fn progress_fill(self) -> f32 {
        0.18 + 0.62 * self.running_phase.clamp(0.0, 1.0)
    }
}

/// Builds the retained analysis dock body for one Electronics panel.
///
/// `tab_id` is the retained dock tab id, so the panel identity, its title, its
/// run command and its row command prefix all come from one source.
pub fn build_electronics_analysis_surface(
    palette: StudioUiPalette,
    tab_id: &str,
    lines: &[ElectronicsAnalysisLine],
    motion: ElectronicsAnalysisMotion,
) -> UiSurface {
    let tokens = palette.tokens();
    let Some(panel) = ElectronicsAnalysisPanel::from_tab(tab_id) else {
        return unavailable_surface(palette);
    };
    let is_running = lines
        .iter()
        .any(|line| line.tone == ElectronicsAnalysisTone::Running);
    let has_result = lines.iter().any(|line| {
        matches!(
            line.tone,
            ElectronicsAnalysisTone::Passed | ElectronicsAnalysisTone::Issues
        )
    });

    let header_icon = if panel.is_simulation() {
        UiIconId::Play
    } else {
        UiIconId::Warning
    };
    let mut root = UiNode::new("electronics.analysis", UiNodeKind::Panel)
        .with_class("electronics-analysis")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(PANEL_PADDING_X, PANEL_PADDING_Y),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("electronics.analysis.header", UiNodeKind::Toolbar)
                .with_class("electronics-analysis-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 7.0,
                    padding: UiSpacing::xy(8.0, 6.0),
                    ..UiLayout::fixed(0.0, ELECTRONICS_ITEM_HEIGHT)
                        .with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("electronics.analysis.header.icon", UiNodeKind::Label)
                        .with_icon(UiIcon::new(header_icon).with_size(UiIconSize::Small))
                        // A row child with no authored track resolves to zero
                        // width and its glyph is never painted.
                        .with_layout(UiLayout::fixed(
                            ELECTRONICS_ROW_GLYPH_TRACK,
                            ELECTRONICS_ITEM_HEIGHT - 12.0,
                        )),
                )
                .with_child(
                    UiNode::new("electronics.analysis.header.title", UiNodeKind::Label)
                        .with_text_key(panel.title_key())
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 12.0,
                            line_height_px: 16.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        })
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
                ),
        );

    // The host already slides the whole body while the dock panel swaps, so the
    // body only fades in. Two competing offsets would double the travel.
    let mut body = UiNode::scroll_view("electronics.analysis.lines", UiScrollAxis::Vertical)
        .with_class("electronics-analysis-lines")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(2.0, 2.0),
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle {
            opacity: motion.body_opacity(),
            ..UiStyle::transparent()
        });

    if is_running {
        body = body.with_child(running_progress(motion));
    }
    for (index, line) in lines.iter().enumerate() {
        body = body.with_child(analysis_row(palette, panel, line, index));
    }

    root = root.with_child(body);
    root = root.with_child(action_row(palette, panel, is_running, has_result));

    let mut surface = UiSurface::new("electronics.analysis", palette, root);
    surface.style_sheet = analysis_style_sheet(palette);
    surface
}

/// Body shown when the active dock tab is not an Electronics analysis panel.
///
/// The dock also holds other retained tabs, so the body states that it has no
/// analysis to present instead of rendering a panel that was not requested.
fn unavailable_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("electronics.analysis", UiNodeKind::Panel)
        .with_class("electronics-analysis")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 8.0,
            padding: UiSpacing::xy(18.0, 18.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("electronics.analysis.unavailable", UiNodeKind::Label)
                .with_text_key("electronics.analysis.unavailable")
                .with_text_style(electronics_body_style(tokens.text_muted)),
        );
    let mut surface = UiSurface::new("electronics.analysis", palette, root);
    surface.style_sheet = analysis_style_sheet(palette);
    surface
}

/// Indeterminate progress track for a background analysis.
///
/// The track is always painted and the fill only changes width, so a paused
/// host clock (reduced motion) still reads as "running" instead of vanishing.
fn running_progress(motion: ElectronicsAnalysisMotion) -> UiNode {
    let fill_width = (100.0 * motion.progress_fill()).clamp(8.0, 80.0);
    UiNode::new("electronics.analysis.progress", UiNodeKind::Panel)
        .with_class("electronics-analysis-progress")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(2.0, 2.0),
            ..UiLayout::fixed(0.0, 11.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key("electronics.analysis.running")
        .with_child(
            UiNode::new("electronics.analysis.progress.track", UiNodeKind::Panel)
                .with_class("electronics-analysis-progress-track")
                .with_layout(UiLayout::row().fill_width().fixed_height(4.0))
                .with_child(
                    UiNode::new("electronics.analysis.progress.fill", UiNodeKind::Panel)
                        .with_class("electronics-analysis-progress-fill")
                        .with_layout(UiLayout::row().fixed_width(fill_width).fixed_height(4.0)),
                ),
        )
}

/// One analysis row.
///
/// Severity is carried by an icon and by the localized text, never by color
/// alone, so the dock stays readable with color-vision deficiency. Only a row
/// the host resolved to a component is a real button: clickable, focusable and
/// reachable with Enter/Space. A severity alone never makes a row activatable,
/// because the dock offsets its report rows with status and stale lines and a
/// positional command would select the wrong finding.
fn analysis_row(
    palette: StudioUiPalette,
    panel: ElectronicsAnalysisPanel,
    line: &ElectronicsAnalysisLine,
    index: usize,
) -> UiNode {
    let tokens = palette.tokens();
    let color = analysis_tone_color(tokens, line.tone);
    let icon = analysis_tone_icon(line.tone);
    let id = format!("electronics.analysis.line.{index}");
    let command = analysis_row_command(line);
    let text = UiNode::new(format!("{id}.text"), UiNodeKind::Label)
        .with_class("electronics-analysis-line-text")
        .with_text_value(line.text.clone())
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(electronics_body_style(color))
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill));

    let mut row = match command {
        Some(command) => UiNode::new(id.clone(), UiNodeKind::Button)
            .with_class("electronics-analysis-line")
            .with_class("electronics-analysis-line-action")
            .with_layout(analysis_row_layout())
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key(analysis_row_label_key(panel))
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        None => UiNode::new(id.clone(), UiNodeKind::Panel)
            .with_class("electronics-analysis-line")
            .with_layout(analysis_row_layout()),
    };
    if let Some(icon) = icon {
        row = row.with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_class("electronics-analysis-line-icon")
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(color),
                )
                .with_layout(UiLayout::fixed(
                    ELECTRONICS_ROW_GLYPH_TRACK,
                    ELECTRONICS_ROW_CONTENT_HEIGHT,
                )),
        );
    }
    row.with_child(text)
}

fn analysis_row_layout() -> UiLayout {
    UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Start,
        gap: 6.0,
        padding: UiSpacing::xy(6.0, ELECTRONICS_ROW_PADDING_Y),
        width_mode: UiSizeMode::Fill,
        height_mode: UiSizeMode::Fixed,
        basis: [0.0, ELECTRONICS_ROW_HEIGHT],
        ..UiLayout::default()
    }
}

/// Command an activatable row emits, if the host gave it a destination.
fn analysis_row_command(line: &ElectronicsAnalysisLine) -> Option<String> {
    // Identity beats position: the controller resolves the component the
    // finding names, which stays correct even if the dock gains a row.
    match line.target.as_ref() {
        Some(ElectronicsAnalysisTarget::Component { source_id }) => Some(format!(
            "{ELECTRONICS_ANALYSIS_FOCUS_COMPONENT_PREFIX}:{source_id}"
        )),
        None => None,
    }
}

/// Localized description of what activating a row does.
fn analysis_row_label_key(panel: ElectronicsAnalysisPanel) -> &'static str {
    if panel.is_simulation() {
        "electronics.analysis.focus_line_hint"
    } else {
        "electronics.analysis.focus_issue_hint"
    }
}

/// Primary run/cancel action of the analysis dock.
fn action_row(
    palette: StudioUiPalette,
    panel: ElectronicsAnalysisPanel,
    is_running: bool,
    has_result: bool,
) -> UiNode {
    let (command, label_key) = if is_running {
        ("electronics.analysis.cancel", "electronics.analysis.cancel")
    } else if panel.is_simulation() {
        (
            "electronics.analysis.simulation",
            if has_result {
                "electronics.analysis.rerun_simulation"
            } else {
                "electronics.analysis.simulate"
            },
        )
    } else {
        (
            "electronics.analysis.drc",
            if has_result {
                "electronics.analysis.rerun_drc"
            } else {
                "electronics.analysis.run_drc"
            },
        )
    };
    UiNode::new("electronics.analysis.actions", UiNodeKind::Toolbar)
        .with_class("electronics-analysis-actions")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(analysis_action(
            palette,
            command,
            label_key,
            tooltip_key_for_analysis_action(command),
            command,
            is_running,
        ))
}

fn analysis_action(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    tooltip_key: &str,
    command: &str,
    danger: bool,
) -> UiNode {
    let tokens = palette.tokens();
    // Cancel is the only action that discards work, so it wears the destructive
    // treatment while Run/Re-run keeps the quiet one.
    let class = if danger {
        "electronics-analysis-action electronics-analysis-action-danger"
    } else {
        "electronics-analysis-action"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(
            UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT)
                .with_width_mode(UiSizeMode::Fill)
                .with_text_safe_area(true),
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn tooltip_key_for_analysis_action(command: &str) -> &'static str {
    match command {
        "electronics.analysis.cancel" => "electronics.tooltip.analysis.cancel",
        "electronics.analysis.simulation" => "electronics.tooltip.analysis.simulation",
        _ => "electronics.tooltip.analysis.drc",
    }
}

/// Severity of a row, expressed with semantic tokens.
fn analysis_tone_color(tokens: UiTokens, tone: ElectronicsAnalysisTone) -> [u8; 4] {
    match tone {
        ElectronicsAnalysisTone::Passed => tokens.positive,
        ElectronicsAnalysisTone::Issues => tokens.warning,
        ElectronicsAnalysisTone::Running => tokens.info,
        ElectronicsAnalysisTone::Failed => tokens.danger,
        ElectronicsAnalysisTone::Normal => tokens.text_muted,
    }
}

/// Non-color channel for the same severity.
fn analysis_tone_icon(tone: ElectronicsAnalysisTone) -> Option<UiIconId> {
    match tone {
        ElectronicsAnalysisTone::Running => Some(UiIconId::StepForward),
        ElectronicsAnalysisTone::Passed => Some(UiIconId::Success),
        ElectronicsAnalysisTone::Issues => Some(UiIconId::Warning),
        ElectronicsAnalysisTone::Failed => Some(UiIconId::Error),
        ElectronicsAnalysisTone::Normal => None,
    }
}

/// Builds the dimmed scrim that owns clicks outside the delete confirmation.
///
/// It is a separate surface so the host can place it in window coordinates and
/// keep the modal itself unclipped.
pub fn build_electronics_delete_backdrop_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("electronics.delete.backdrop", UiNodeKind::Overlay)
        .with_class("electronics-delete-backdrop")
        .with_material(UiSurfaceMaterial::BackdropScrim)
        .with_layout(UiLayout::fill(UiFlow::None).with_z_index(60))
        .with_style(UiStyle {
            fill: tokens.background,
            border: [0, 0, 0, 0],
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("electronics.delete.confirm_title")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            ELECTRONICS_DELETE_CANCEL_COMMAND,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            ELECTRONICS_DELETE_CANCEL_COMMAND,
        ));
    UiSurface::new("electronics.delete.backdrop", palette, root)
}

/// Builds the destructive-action confirmation modal.
///
/// The host presents it centered in the window while the controller reports a
/// pending deletion, and routes [`ELECTRONICS_DELETE_CONFIRM_COMMAND`] /
/// [`ELECTRONICS_DELETE_CANCEL_COMMAND`] to `confirm_delete` /
/// `cancel_pending_action`. Escape already reaches the controller from the
/// canvas input layer; the explicit binding keeps the modal dismissable when
/// keyboard focus never reached it.
///
/// `entrance` is a host motion sample where 1.0 means closed and 0.0 means
/// open, the same convention as the Electronics overlay entrance. A
/// reduced-motion host resolves its tween to 1.0, which renders the modal fully
/// opaque.
pub fn build_electronics_delete_modal_surface(
    palette: StudioUiPalette,
    rect: UiRect,
    item_count: usize,
    entrance: f32,
) -> UiSurface {
    let tokens = palette.tokens();
    let entrance = if entrance.is_finite() {
        entrance.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let root = UiNode::new("electronics.delete.modal", UiNodeKind::FloatingPanel)
        .with_material(UiSurfaceMaterial::ModalSurface)
        // An absolutely placed overlay keeps `UiFlow::None`, which hands every
        // child the whole panel rect. The stack therefore declares its own
        // column, gap and padding instead of relying on the absolute rect to
        // place four blocks that would otherwise all paint over each other.
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: DELETE_MODAL_GAP,
            padding: UiSpacing::xy(DELETE_MODAL_PADDING_X, DELETE_MODAL_PADDING_Y),
            ..UiLayout::absolute(rect).with_z_index(61)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: ELECTRONICS_BORDER_WIDTH,
            radius: DELETE_MODAL_RADIUS,
            opacity: entrance,
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("electronics.delete.confirm_title")
        .with_accessibility_description_key("electronics.delete.confirm_body")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            ELECTRONICS_DELETE_CANCEL_COMMAND,
        ))
        .with_child(
            UiNode::new("electronics.delete.modal.title", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 7.0,
                    ..UiLayout::fixed(0.0, DELETE_MODAL_HEADER_HEIGHT)
                        .with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("electronics.delete.modal.icon", UiNodeKind::Label)
                        .with_icon(
                            UiIcon::new(UiIconId::Warning)
                                .with_size(UiIconSize::Small)
                                .with_tint(tokens.danger),
                        )
                        .with_layout(UiLayout::fixed(ELECTRONICS_ROW_GLYPH_TRACK, 18.0)),
                )
                .with_child(
                    UiNode::new("electronics.delete.modal.title.text", UiNodeKind::Label)
                        .with_text_key("electronics.delete.confirm_title")
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 13.0,
                            line_height_px: 18.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        })
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
                ),
        )
        .with_child(
            UiNode::new("electronics.delete.modal.body", UiNodeKind::Label)
                .with_text_key("electronics.delete.confirm_body")
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        // The count needs a noun: a bare number floating under the message reads
        // as a mistake, and the catalog has no plural interpolation, so the
        // localized label and the runtime number stay two explicit nodes.
        .with_child(
            UiNode::new("electronics.delete.modal.count", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT)
                        .with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("electronics.delete.modal.count.label", UiNodeKind::Label)
                        .with_text_key("electronics.delete.confirm_count")
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("electronics.delete.modal.count.value", UiNodeKind::Label)
                        .with_text_value(item_count.to_string())
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Body,
                            size_px: 11.0,
                            line_height_px: ELECTRONICS_BODY_LINE_HEIGHT,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        })
                        .with_layout(UiLayout::fit_content()),
                ),
        )
        .with_child(
            UiNode::new("electronics.delete.modal.footer", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::End,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, DELETE_MODAL_FOOTER_HEIGHT)
                        .with_width_mode(UiSizeMode::Fill)
                })
                .with_child(modal_button(
                    palette,
                    "electronics.delete.cancel",
                    "electronics.delete.confirm_cancel",
                    ELECTRONICS_DELETE_CANCEL_COMMAND,
                    false,
                ))
                .with_child(modal_button(
                    palette,
                    "electronics.delete.confirm",
                    "electronics.delete.confirm_accept",
                    ELECTRONICS_DELETE_CONFIRM_COMMAND,
                    true,
                )),
        );
    let mut surface = UiSurface::new("electronics.delete.modal", palette, root);
    surface.style_sheet = delete_modal_style_sheet(palette);
    surface
}

fn modal_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &'static str,
    command: &str,
    danger: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let class = if danger {
        "electronics-delete-button electronics-delete-button-danger"
    } else {
        "electronics-delete-button"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(
            UiLayout::fixed(MODAL_BUTTON_WIDTH, DELETE_MODAL_FOOTER_HEIGHT)
                .with_text_safe_area(true),
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command,
        ))
}

// ---------------------------------------------------------------------------
// Shared surface primitives
//
// The Electronics family needs one body text recipe, one set of metrics, one
// class rule, one focus ring and one icon button. They live here so navigator,
// inspector, toolbar, context menu and analysis body cannot drift apart.
// ---------------------------------------------------------------------------

/// Height of one interactive control: toolbar buttons, tabs, text fields, dock
/// actions and context-menu rows all resolve to this track.
pub(crate) const ELECTRONICS_CONTROL_HEIGHT: f32 = 30.0;
/// Height of one single-line list or property row: analysis lines, inspector
/// field rows, pin rows and category headers.
pub(crate) const ELECTRONICS_ROW_HEIGHT: f32 = 24.0;
/// Height of one two-line item: navigator rows, library cards and panel headers.
pub(crate) const ELECTRONICS_ITEM_HEIGHT: f32 = 38.0;
/// Line box of [`electronics_body_style`]. A label that declares a smaller track
/// than this clips its own glyphs, so row builders use it as their minimum.
pub(crate) const ELECTRONICS_BODY_LINE_HEIGHT: f32 = 15.0;
/// Width reserved for a 14px leading glyph inside a row or control.
pub(crate) const ELECTRONICS_ROW_GLYPH_TRACK: f32 = 14.0;
/// Smallest track a standalone label may claim before its text has no room.
pub(crate) const ELECTRONICS_ROW_MIN_TRACK: f32 = 24.0;
/// Vertical padding of a single-line row, in logical points. Half of it on each
/// side, so `row height - padding` is the track a leading glyph may occupy.
pub(crate) const ELECTRONICS_ROW_PADDING_Y: f32 = 4.0;
/// Content track of a single-line row: its height minus its vertical padding.
pub(crate) const ELECTRONICS_ROW_CONTENT_HEIGHT: f32 =
    ELECTRONICS_ROW_HEIGHT - ELECTRONICS_ROW_PADDING_Y * 2.0;
/// Horizontal padding of a panel body.
pub(crate) const PANEL_PADDING_X: f32 = 8.0;
/// Vertical padding of a panel body.
pub(crate) const PANEL_PADDING_Y: f32 = 8.0;
/// Corner radius of every Electronics panel, row and control.
pub(crate) const ELECTRONICS_CORNER_RADIUS: f32 = 4.0;
/// Border weight of every Electronics panel, row and control.
pub(crate) const ELECTRONICS_BORDER_WIDTH: f32 = 1.0;
/// Alpha of a destructive resting fill, in 0..255.
pub(crate) const DANGER_FILL_ALPHA: u8 = 64;
/// Alpha of a destructive hover fill, in 0..255.
pub(crate) const DANGER_HOVER_ALPHA: u8 = 110;
/// Alpha of a destructive pressed fill, in 0..255.
pub(crate) const DANGER_PRESS_ALPHA: u8 = 160;
/// Corner radius of the delete confirmation modal. A window-level panel reads as
/// one step more elevated than an in-panel control.
const DELETE_MODAL_RADIUS: f32 = 6.0;
/// Width of one modal button, in logical points.
const MODAL_BUTTON_WIDTH: f32 = 112.0;

/// Compact body text used by every Electronics surface.
pub(crate) fn electronics_body_style(color: [u8; 4]) -> UiTextStyle {
    UiTextStyle {
        role: UiTextRole::Body,
        size_px: 11.0,
        line_height_px: ELECTRONICS_BODY_LINE_HEIGHT,
        weight: UiFontWeight::Regular,
        color,
        inherit_color: false,
    }
}

/// Replaces only the alpha channel of a semantic token.
pub(crate) fn with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

/// Bordered class rule at the base layer.
pub(crate) fn electronics_class_rule(
    class: &str,
    fill: [u8; 4],
    border: [u8; 4],
    text: [u8; 4],
) -> UiStyleRule {
    electronics_state_rule(
        UiStyleRuleState::Always,
        class,
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            radius: Some(ELECTRONICS_CORNER_RADIUS),
            ..UiStylePatch::default()
        },
    )
}

/// Borderless class rule for list rows and tabs.
pub(crate) fn electronics_flat_class_rule(
    class: &str,
    fill: [u8; 4],
    text: [u8; 4],
) -> UiStyleRule {
    electronics_state_rule(
        UiStyleRuleState::Always,
        class,
        UiStylePatch {
            fill: Some(fill),
            border: Some([0, 0, 0, 0]),
            text: Some(text),
            border_width: Some(0.0),
            radius: Some(ELECTRONICS_CORNER_RADIUS),
            ..UiStylePatch::default()
        },
    )
}

/// Quiet hover lift shared by every interactive Electronics class.
pub(crate) fn electronics_hover_rule(class: &str, tokens: UiTokens) -> UiStyleRule {
    electronics_state_rule(
        UiStyleRuleState::Hovered,
        class,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            border: Some([0, 0, 0, 0]),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    )
}

/// Keyboard focus ring. `focus` is the semantic token and the border keeps the
/// 1px weight the rest of the editor uses, so a focused control never grows.
pub(crate) fn electronics_focus_rule(class: &str, tokens: UiTokens) -> UiStyleRule {
    electronics_state_rule(
        UiStyleRuleState::Focused,
        class,
        UiStylePatch {
            border: Some(tokens.focus),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            ..UiStylePatch::default()
        },
    )
}

/// Pointer-pressed ring. A warm edge plus a raised fill, not a solid accent
/// fill, so a press reads as continuity and not as a primary action.
pub(crate) fn electronics_active_rule(class: &str, tokens: UiTokens) -> UiStyleRule {
    electronics_state_rule(
        UiStyleRuleState::Active,
        class,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            border: Some(tokens.accent_hot),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    )
}

/// Quiet disabled treatment shared by action icons.
pub(crate) fn electronics_disabled_rule(class: &str, tokens: UiTokens) -> UiStyleRule {
    electronics_state_rule(
        UiStyleRuleState::Disabled,
        class,
        UiStylePatch {
            fill: Some([0, 0, 0, 0]),
            border: Some([0, 0, 0, 0]),
            text: Some(tokens.text_muted),
            opacity: Some(0.35),
            ..UiStylePatch::default()
        },
    )
}

pub(crate) fn electronics_state_rule(
    state: UiStyleRuleState,
    class: &str,
    patch: UiStylePatch,
) -> UiStyleRule {
    UiStyleRule::new(UiStyleSelector::Class(class.to_string()), patch).when(state)
}

/// Icon-only command button shared by navigator and toolbar.
///
/// One recipe keeps the icon size, the localized label, the role and the click
/// binding identical across the Electronics chrome.
#[derive(Debug, Clone)]
pub(crate) struct ElectronicsIconButton {
    id: String,
    icon: UiIconId,
    command: String,
    tooltip_key: &'static str,
    class: &'static str,
    width: f32,
    height: f32,
    /// Optional icon tint. Tools stay on the text token until they are active.
    icon_tint: Option<[u8; 4]>,
}

impl ElectronicsIconButton {
    pub fn new(
        id: impl Into<String>,
        icon: UiIconId,
        command: impl Into<String>,
        tooltip_key: &'static str,
        class: &'static str,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            command: command.into(),
            tooltip_key,
            class,
            width,
            height,
            icon_tint: None,
        }
    }

    pub fn with_tint(mut self, tint: [u8; 4]) -> Self {
        self.icon_tint = Some(tint);
        self
    }

    pub fn build(self) -> UiNode {
        let mut icon = UiIcon::new(self.icon).with_size(UiIconSize::Small);
        if let Some(tint) = self.icon_tint {
            icon = icon.with_tint(tint);
        }
        UiNode::new(self.id, UiNodeKind::Button)
            .with_class(self.class)
            .with_layout(UiLayout::fixed(self.width, self.height))
            .with_icon(icon)
            .with_tooltip_key(self.tooltip_key)
            .with_accessibility_label_key(self.tooltip_key)
            .with_accessibility_role(UiAccessibilityRole::Button)
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, self.command))
    }
}

fn analysis_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let mut rules = vec![
        electronics_class_rule(
            "electronics-analysis",
            tokens.background,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-analysis-header",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-analysis-lines",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-analysis-line",
            tokens.surface,
            tokens.border,
            tokens.text_muted,
        ),
        electronics_class_rule(
            "electronics-analysis-progress",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text_muted,
        ),
        electronics_class_rule(
            "electronics-analysis-progress-track",
            with_alpha(tokens.border, 160),
            [0, 0, 0, 0],
            [0, 0, 0, 0],
        ),
        electronics_class_rule(
            "electronics-analysis-progress-fill",
            tokens.info,
            [0, 0, 0, 0],
            [0, 0, 0, 0],
        ),
        electronics_class_rule(
            "electronics-analysis-actions",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-analysis-action",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
    ];
    // Only rows the host can resolve are buttons, so the affordance and the
    // keyboard path stay truthful.
    for class in [
        "electronics-analysis-line-action",
        "electronics-analysis-action",
    ] {
        rules.push(electronics_hover_rule(class, tokens));
        rules.push(electronics_focus_rule(class, tokens));
        rules.push(electronics_active_rule(class, tokens));
    }
    // Cancelling a running analysis discards its result, so it wears the same
    // destructive ring the delete confirmation uses instead of the quiet action.
    rules.push(electronics_class_rule(
        "electronics-analysis-action-danger",
        with_alpha(tokens.danger, DANGER_FILL_ALPHA),
        tokens.danger,
        tokens.text,
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-analysis-action-danger",
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_HOVER_ALPHA)),
            border: Some(tokens.danger),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule(
        "electronics-analysis-action-danger",
        tokens,
    ));
    UiStyleSheet { rules }
}

fn delete_modal_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let mut rules = vec![electronics_class_rule(
        "electronics-delete-button",
        tokens.surface_alt,
        tokens.border,
        tokens.text,
    )];
    rules.push(electronics_hover_rule("electronics-delete-button", tokens));
    rules.push(electronics_focus_rule("electronics-delete-button", tokens));
    rules.push(electronics_active_rule("electronics-delete-button", tokens));
    rules.push(electronics_class_rule(
        "electronics-delete-button-danger",
        with_alpha(tokens.danger, DANGER_FILL_ALPHA),
        tokens.danger,
        tokens.text,
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-delete-button-danger",
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_HOVER_ALPHA)),
            border: Some(tokens.danger),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule(
        "electronics-delete-button-danger",
        tokens,
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Active,
        "electronics-delete-button-danger",
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_PRESS_ALPHA)),
            border: Some(tokens.danger),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    UiStyleSheet { rules }
}

/// The retained-frame diagnostics are a test-only dependency of this module.
#[cfg(test)]
use raf_render::api_graphic_basic::ui_surface::UiSurfaceDiagnostics;

/// Quality gate shared by every Electronics surface.
///
/// `UiSurfaceDiagnostics` is the project gate, and this is the first thing in
/// `raf_editor` that ever runs it. One counter needs an honest exception: a
/// `ScrollView` control is interactive, and RafUI's role vocabulary has no
/// region or group role to give it, so every scroll view is reported as an
/// untyped interactive node. The gate therefore accepts exactly the nodes that
/// are scroll views and fails on any other untyped interactive node.
#[cfg(test)]
pub(crate) fn assert_electronics_layout_gate(
    surface: &UiSurface,
    width: u32,
    height: u32,
) -> UiSurfaceDiagnostics {
    let diagnostics =
        UiSurfaceDiagnostics::from_frame(&surface.build_frame(width, height, [0, 0, 0, 255]));
    let expected_untyped = untyped_interactive_nodes(&surface.root);
    assert_eq!(diagnostics.duplicate_ids, 0, "retained ids must be unique");
    assert_eq!(diagnostics.zero_sized_boxes, 0, "no collapsed layout box");
    assert_eq!(
        diagnostics.invalid_clip_regions, 0,
        "a clip must always intersect a real box"
    );
    assert_eq!(
        diagnostics.missing_accessibility_labels, 0,
        "every interactive node needs an accessible name"
    );
    assert_eq!(
        diagnostics.missing_accessibility_roles, expected_untyped,
        "only a scroll view may stay untyped; every other interactive node declares a role"
    );
    diagnostics
}

/// Interactive nodes without a role, which the gate tolerates only for scroll
/// views. Counted from the document so adding one scroll view does not require
/// editing an expected number.
#[cfg(test)]
fn untyped_interactive_nodes(node: &UiNode) -> usize {
    let own = usize::from(
        node.interactive
            && !node.disabled
            && node.accessibility_role == UiAccessibilityRole::Generic,
    );
    own + node
        .children
        .iter()
        .map(untyped_interactive_nodes)
        .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ui::UiAction;

    fn surface_for(tab: &str, lines: &[ElectronicsAnalysisLine]) -> UiSurface {
        build_electronics_analysis_surface(
            StudioUiPalette::IndustrialDark,
            tab,
            lines,
            ElectronicsAnalysisMotion::default(),
        )
    }

    #[test]
    fn panel_identity_comes_from_the_tab_id_not_from_a_title() {
        assert_eq!(
            ElectronicsAnalysisPanel::from_tab(ELECTRONICS_ANALYSIS_SIMULATION_TAB),
            Some(ElectronicsAnalysisPanel::Simulation)
        );
        assert_eq!(
            ElectronicsAnalysisPanel::from_tab(ELECTRONICS_ANALYSIS_DRC_TAB),
            Some(ElectronicsAnalysisPanel::Drc)
        );
        assert_eq!(ElectronicsAnalysisPanel::from_tab("console"), None);
    }

    #[test]
    fn an_unknown_tab_states_that_there_is_no_analysis() {
        let surface = surface_for("console", &[]);
        assert!(surface
            .root
            .find("electronics.analysis.unavailable")
            .is_some());
    }

    #[test]
    fn a_simulation_tab_presents_the_simulation_body() {
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_SIMULATION_TAB,
            &[ElectronicsAnalysisLine::normal("Componentes: 1")],
        );
        let title = surface
            .root
            .find("electronics.analysis.header.title")
            .expect("panel title");
        assert_eq!(
            title.text_key.as_deref(),
            Some("electronics.analysis.simulation_title")
        );
        let action = surface
            .root
            .find("electronics.analysis.simulation")
            .expect("run simulation");
        assert!(action.event_handlers.iter().any(|event| {
            matches!(&event.action, UiAction::Command { name }
                if name == "electronics.analysis.simulation")
        }));
    }

    #[test]
    fn severity_is_carried_by_an_icon_and_not_only_by_color() {
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_DRC_TAB,
            &[
                ElectronicsAnalysisLine::with_tone(
                    "[ERROR] short_circuit: N001 shorts VCC to GND",
                    ElectronicsAnalysisTone::Failed,
                ),
                ElectronicsAnalysisLine::with_tone(
                    "[WARNING] clearance: R2 to C1",
                    ElectronicsAnalysisTone::Issues,
                ),
            ],
        );
        let error_icon = surface
            .root
            .find("electronics.analysis.line.0.icon")
            .expect("severity icon");
        assert_eq!(error_icon.icon.map(|icon| icon.id), Some(UiIconId::Error));
        let warning_icon = surface
            .root
            .find("electronics.analysis.line.1.icon")
            .expect("severity icon");
        assert_eq!(
            warning_icon.icon.map(|icon| icon.id),
            Some(UiIconId::Warning)
        );
    }

    #[test]
    fn only_a_host_resolved_finding_is_activatable() {
        let source_id = Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_DRC_TAB,
            &[
                ElectronicsAnalysisLine::normal("Componentes: 2"),
                // A severity without a destination is a status line, not a
                // navigation target: the dock offsets its report rows with
                // status and stale lines, so a positional command would resolve
                // a different finding than the one the user clicked.
                ElectronicsAnalysisLine::with_tone(
                    "Estado: se encontraron problemas | Errores: 1",
                    ElectronicsAnalysisTone::Issues,
                ),
                ElectronicsAnalysisLine::with_target(
                    "[ERROR] short_circuit: N001 shorts VCC to GND",
                    ElectronicsAnalysisTone::Failed,
                    ElectronicsAnalysisTarget::Component { source_id },
                ),
            ],
        );
        let metric = surface
            .root
            .find("electronics.analysis.line.0")
            .expect("metric row");
        assert!(metric.event_handlers.is_empty());
        assert!(!metric.focusable);
        let status = surface
            .root
            .find("electronics.analysis.line.1")
            .expect("status row");
        assert!(
            status.event_handlers.is_empty(),
            "a status line must not claim to be navigable"
        );
        assert!(!status.focusable);
        let finding = surface
            .root
            .find("electronics.analysis.line.2")
            .expect("finding row");
        assert!(finding.focusable);
        assert_eq!(finding.accessibility_role, UiAccessibilityRole::Button);
        assert!(finding.event_handlers.iter().any(|event| {
            matches!(&event.action, UiAction::Command { name }
                  if name == &format!("electronics.analysis.focus_component:{source_id}"))
        }));
    }

    #[test]
    fn a_running_analysis_replaces_run_with_cancel_and_shows_progress() {
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_DRC_TAB,
            &[ElectronicsAnalysisLine::with_tone(
                "Status: running",
                ElectronicsAnalysisTone::Running,
            )],
        );
        assert!(surface.root.find("electronics.analysis.cancel").is_some());
        assert!(surface
            .root
            .find("electronics.analysis.progress.fill")
            .is_some());
        assert!(surface.root.find("electronics.analysis.drc").is_none());
    }

    #[test]
    fn every_interactive_analysis_node_declares_a_role_and_a_label() {
        let source_id = Uuid::from_u128(0x9999_8888_7777_6666_5555_4444_3333_2222);
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_DRC_TAB,
            &[
                ElectronicsAnalysisLine::with_tone(
                    "[ERROR] short_circuit",
                    ElectronicsAnalysisTone::Failed,
                ),
                ElectronicsAnalysisLine::with_target(
                    "[ERROR] clearance: R2 to C1",
                    ElectronicsAnalysisTone::Issues,
                    ElectronicsAnalysisTarget::Component { source_id },
                ),
            ],
        );
        for id in ["electronics.analysis.drc", "electronics.analysis.line.1"] {
            let node = surface.root.find(id).expect("interactive node");
            assert_ne!(
                node.accessibility_role,
                UiAccessibilityRole::Generic,
                "{id} is interactive but exposes no role"
            );
            assert!(node.accessibility_label_key.is_some(), "{id} needs a label");
        }
        // A report line with no destination behind it is informational, not a
        // control: it must stay non-interactive and must not claim a role,
        // because activating it would have nowhere to go.
        let informational = surface
            .root
            .find("electronics.analysis.line.0")
            .expect("informational row");
        assert_eq!(informational.kind, UiNodeKind::Panel);
        assert!(!informational.focusable);
    }

    #[test]
    fn the_analysis_body_passes_the_retained_layout_gate() {
        let source_id = Uuid::from_u128(0x0f0f_0f0f_0f0f_0f0f_0f0f_0f0f_0f0f);
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_DRC_TAB,
            &[
                ElectronicsAnalysisLine::normal("Componentes: 12"),
                ElectronicsAnalysisLine::with_target(
                    "[ERROR] short_circuit: N001 shorts VCC to GND",
                    ElectronicsAnalysisTone::Failed,
                    ElectronicsAnalysisTarget::Component { source_id },
                ),
            ],
        );
        let diagnostics = assert_electronics_layout_gate(&surface, 420, 320);
        assert!(
            diagnostics.layout_boxes > 8 && diagnostics.focusable_regions >= 2,
            "the gate must have run over a real body, not an empty document"
        );
    }

    #[test]
    fn every_analysis_row_shares_the_family_row_height() {
        let surface = surface_for(
            ELECTRONICS_ANALYSIS_DRC_TAB,
            &[ElectronicsAnalysisLine::with_tone(
                "[ERROR] short_circuit",
                ElectronicsAnalysisTone::Failed,
            )],
        );
        let row = surface
            .root
            .find("electronics.analysis.line.0")
            .expect("analysis row");
        assert_eq!(row.layout.basis[1], ELECTRONICS_ROW_HEIGHT);
        assert_eq!(row.layout.height_mode, UiSizeMode::Fixed);
        let action = surface
            .root
            .find("electronics.analysis.drc")
            .expect("dock action");
        assert_eq!(action.layout.basis[1], ELECTRONICS_CONTROL_HEIGHT);
    }

    #[test]
    fn the_modal_stack_fits_its_declared_height() {
        let required = DELETE_MODAL_PADDING_Y * 2.0
            + DELETE_MODAL_HEADER_HEIGHT
            + ELECTRONICS_BODY_LINE_HEIGHT * 2.0
            + ELECTRONICS_BODY_LINE_HEIGHT
            + DELETE_MODAL_FOOTER_HEIGHT
            + DELETE_MODAL_GAP * 3.0;
        assert!(
            ELECTRONICS_DELETE_MODAL_HEIGHT >= required,
            "the modal stack needs {required} points but declares {}",
            ELECTRONICS_DELETE_MODAL_HEIGHT
        );
    }

    #[test]
    fn the_modal_count_states_what_is_being_counted() {
        let rect = UiRect::new(
            0.0,
            0.0,
            ELECTRONICS_DELETE_MODAL_WIDTH,
            ELECTRONICS_DELETE_MODAL_HEIGHT,
        );
        let surface =
            build_electronics_delete_modal_surface(StudioUiPalette::IndustrialDark, rect, 7, 1.0);
        let label = surface
            .root
            .find("electronics.delete.modal.count.label")
            .expect("count label");
        assert_eq!(
            label.text_key.as_deref(),
            Some("electronics.delete.confirm_count")
        );
        let value = surface
            .root
            .find("electronics.delete.modal.count.value")
            .expect("count value");
        assert_eq!(value.text_value.as_deref(), Some("7"));
    }

    #[test]
    fn the_modal_declares_a_column_so_its_blocks_do_not_overlap() {
        let rect = UiRect::new(
            0.0,
            0.0,
            ELECTRONICS_DELETE_MODAL_WIDTH,
            ELECTRONICS_DELETE_MODAL_HEIGHT,
        );
        let surface =
            build_electronics_delete_modal_surface(StudioUiPalette::IndustrialDark, rect, 1, 1.0);
        let modal = surface.root.find("electronics.delete.modal").expect("root");
        assert_eq!(
            modal.layout.flow,
            UiFlow::Column,
            "an absolute overlay hands every child the whole panel unless it stacks"
        );
        let frame = surface.build_frame(
            ELECTRONICS_DELETE_MODAL_WIDTH as u32,
            ELECTRONICS_DELETE_MODAL_HEIGHT as u32,
            [0, 0, 0, 255],
        );
        let mut blocks: Vec<(&str, f32, f32)> = Vec::new();
        for id in [
            "electronics.delete.modal.title",
            "electronics.delete.modal.body",
            "electronics.delete.modal.count",
            "electronics.delete.modal.footer",
        ] {
            let layout = frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == id)
                .unwrap_or_else(|| panic!("{id} is missing from the frame"));
            blocks.push((id, layout.rect.y, layout.rect.height));
        }
        for pair in blocks.windows(2) {
            let (previous, previous_y, previous_height) = pair[0];
            let (next, next_y, _) = pair[1];
            assert!(
                previous_y + previous_height <= next_y + f32::EPSILON,
                "{previous} ends at {} and {next} starts at {next_y}",
                previous_y + previous_height
            );
        }
    }

    #[test]
    fn the_delete_modal_passes_the_retained_layout_gate() {
        let rect = UiRect::new(
            0.0,
            0.0,
            ELECTRONICS_DELETE_MODAL_WIDTH,
            ELECTRONICS_DELETE_MODAL_HEIGHT,
        );
        let surface =
            build_electronics_delete_modal_surface(StudioUiPalette::IndustrialDark, rect, 3, 1.0);
        assert_electronics_layout_gate(
            &surface,
            ELECTRONICS_DELETE_MODAL_WIDTH as u32,
            ELECTRONICS_DELETE_MODAL_HEIGHT as u32,
        );
    }

    #[test]
    fn delete_modal_traps_focus_and_exposes_both_decisions() {
        let rect = UiRect::new(
            120.0,
            80.0,
            ELECTRONICS_DELETE_MODAL_WIDTH,
            ELECTRONICS_DELETE_MODAL_HEIGHT,
        );
        let surface =
            build_electronics_delete_modal_surface(StudioUiPalette::IndustrialDark, rect, 1, 0.0);
        let modal = surface
            .root
            .find("electronics.delete.modal")
            .expect("modal root");
        assert_eq!(modal.accessibility_role, UiAccessibilityRole::Dialog);
        assert!(modal.focusable);
        assert!(modal.event_handlers.iter().any(|event| {
            matches!(&event.event, UiEventKind::KeyPress(key) if key == "escape")
        }));
        for id in ["electronics.delete.confirm", "electronics.delete.cancel"] {
            let button = surface.root.find(id).expect("modal button");
            assert!(button.focusable);
            assert!(button.text_key.is_some(), "{id} needs a localized label");
        }
        assert_eq!(
            modal.style.opacity, 0.0,
            "the modal follows the host entrance sample"
        );
        let open =
            build_electronics_delete_modal_surface(StudioUiPalette::IndustrialDark, rect, 1, 1.0);
        assert_eq!(
            open.root
                .find("electronics.delete.modal")
                .map(|node| node.style.opacity),
            Some(1.0)
        );
    }

    #[test]
    fn shared_primitives_expose_focus_and_pressed_states() {
        let sheet = analysis_style_sheet(StudioUiPalette::IndustrialDark);
        let states: Vec<UiStyleRuleState> = sheet
            .rules
            .iter()
            .filter(|rule| {
                rule.selector == UiStyleSelector::Class("electronics-analysis-action".to_string())
            })
            .map(|rule| rule.state)
            .collect();
        assert!(states.contains(&UiStyleRuleState::Focused));
        assert!(states.contains(&UiStyleRuleState::Active));
    }
}
