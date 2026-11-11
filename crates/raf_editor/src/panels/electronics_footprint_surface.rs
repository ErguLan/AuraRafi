//! Retained chooser for project-local physical PCB footprint assets.
//!
//! The surface only presents the supplied model. The Electronics editor owns
//! library reads, search results, pin/pad compatibility, and assignment.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiJustify,
    UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStylePatch,
    UiStyleRuleState, UiStyleSheet, UiTextInput, UiTextOverflow, UiTextRole, UiTextStyle,
};
use serde::{Deserialize, Serialize};

use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_focus_rule, electronics_hover_rule, electronics_state_rule, with_alpha,
    ELECTRONICS_BODY_LINE_HEIGHT, ELECTRONICS_ROW_HEIGHT,
};

const SEARCH_HEIGHT: f32 = 36.0;
const IMPORT_HEIGHT: f32 = 38.0;
const FOOTER_HEIGHT: f32 = 38.0;
const CANDIDATE_ROW_HEIGHT: f32 = 44.0;
const WIDE_LAYOUT_MIN_WIDTH: f32 = 720.0;
const PREVIEW_HEIGHT: f32 = 152.0;

/// Simple, serializable input model for a real project footprint library.
///
/// `candidates` must be the list the editor wants to show for `query` (in that
/// order). A selection command carries this displayed index back to the host.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FootprintChooserModel {
    #[serde(default)]
    pub import_pending: bool,
    #[serde(default)]
    pub preview_image_key: Option<String>,
    pub query: String,
    pub import_path: String,
    pub component_designator: String,
    pub component_pins: Vec<String>,
    pub current_footprint: String,
    pub import_status: String,
    pub import_error: bool,
    pub selected_index: Option<usize>,
    pub candidates: Vec<FootprintChooserCandidate>,
}

impl FootprintChooserModel {
    fn is_browsing(&self) -> bool {
        self.component_designator.trim().is_empty() && self.component_pins.is_empty()
    }
}

/// Backend-independent projection of one physical asset and its compatibility
/// with the component currently being edited.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FootprintChooserCandidate {
    /// Stable project-local asset id used by the assignment command.
    pub id: String,
    pub name: String,
    pub source: String,
    pub description: String,
    pub tags: Vec<String>,
    /// KiCad-local bounds and dimensions, in millimeters.
    pub bounds_min_mm: [f32; 2],
    pub bounds_size_mm: [f32; 2],
    pub pads: Vec<FootprintChooserPad>,
    /// Component pin label to physical pad number. Empty pad strings are
    /// preserved as unmatched pins so the UI cannot imply a mapping exists.
    pub pin_pad_mapping: Vec<(String, String)>,
    pub compatible: bool,
    pub compatibility_detail: String,
    pub fabrication_ready: bool,
    pub fabrication_detail: String,
}

/// Physical pad geometry projected into simple values for RafUI presentation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FootprintChooserPad {
    pub number: String,
    pub position_mm: [f32; 2],
    pub size_mm: [f32; 2],
    /// Common KiCad shape names such as `rect`, `circle`, or `oval`.
    pub shape: String,
    pub rotation_degrees: f32,
}

/// Builds an embeddable central-workspace surface node.
pub fn footprint_chooser_content(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    available_size: [f32; 2],
) -> UiNode {
    let browsing = model.is_browsing();
    let wide = available_size[0] >= WIDE_LAYOUT_MIN_WIDTH;
    let body = if wide {
        wide_body(palette, model, available_size)
    } else {
        compact_body(palette, model, available_size)
    };
    let mut chooser = UiNode::new("electronics.footprint.chooser", UiNodeKind::Panel)
        .with_class("electronics-footprint-chooser")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            padding: UiSpacing::xy(16.0, 14.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key(if browsing {
            "electronics.desktop.footprints"
        } else {
            "electronics.footprint.title"
        })
        .focusable()
        .with_event(close_on_escape())
        .with_child(header(palette, browsing));
    if !browsing {
        chooser = chooser.with_child(target_context(palette, model));
    }
    chooser = chooser
        .with_child(search_field(palette))
        .with_child(body)
        .with_child(import_field(palette, model.import_pending));
    if !model.import_status.trim().is_empty() {
        chooser = chooser.with_child(import_status(palette, model));
    }
    chooser.with_child(footer(palette, model))
}

/// Wraps the chooser node as a standalone retained surface.
pub fn build_electronics_footprint_surface(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    available_size: [f32; 2],
) -> UiSurface {
    let mut surface = UiSurface::new(
        "electronics.footprint",
        palette,
        footprint_chooser_content(palette, model, available_size),
    );
    surface.style_sheet = footprint_chooser_style_sheet(palette);
    surface
}

/// The catalogue lives in the navigator; its detail stays in the inspector.
/// Both use the same query, selection and project-scoped assignment commands.
pub fn footprint_library_content(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
) -> UiNode {
    let mut root = UiNode::new("electronics.footprint.library", UiNodeKind::Panel)
        .with_layout(UiLayout {
            gap: 6.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(search_field(palette));
    let mut list = candidate_list(palette, model, 0.0, 0.0);
    // Compatibility belongs beside the actual target and mapping in the inspector.
    // In the narrow catalogue, keep the asset name and provenance readable.
    for row in &mut list.children {
        row.children.retain(|node| !node.id.ends_with(".status"));
        if let Some(icon) = row.children.first_mut() {
            icon.icon = Some(UiIcon::new(UiIconId::Pcb).with_size(UiIconSize::Small));
        }
    }
    root = root.with_child(list);
    root
}

pub fn build_footprint_inspector_surface(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    width: f32,
) -> UiSurface {
    let mut imports = import_field(palette, model.import_pending);
    imports.layout = UiLayout {
        flow: UiFlow::Column,
        gap: 6.0,
        ..UiLayout::fixed(0.0, 76.0).fill_width()
    };
    imports
        .children
        .retain(|node| !node.id.ends_with("path-icon"));
    for child in &mut imports.children {
        child.layout = UiLayout::fixed(0.0, 34.0)
            .fill_width()
            .with_text_safe_area(true);
    }
    let mut detail = UiNode::scroll_view(
        "electronics.footprint.inspector.scroll",
        UiScrollAxis::Vertical,
    )
    .with_layout(UiLayout {
        gap: 10.0,
        padding: UiSpacing {
            right: 14.0,
            ..UiSpacing::same(0.0)
        },
        ..UiLayout::fill(UiFlow::Column)
    });
    if !model.is_browsing() {
        detail = detail.with_child(target_context(palette, model));
    }
    detail = detail
        .with_child(selected_detail(
            palette,
            model,
            [(width - 32.0).max(120.0), 320.0],
            false,
        ))
        .with_child(imports);
    if !model.import_status.trim().is_empty() {
        detail = detail.with_child(import_status(palette, model));
    }
    let root = UiNode::new("electronics.footprint.inspector", UiNodeKind::Panel)
        .with_class("electronics-footprint-chooser")
        .with_layout(UiLayout {
            gap: 8.0,
            padding: UiSpacing::same(10.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(header(palette, model.is_browsing()))
        .with_child(detail)
        .with_child(footer(palette, model));
    let mut surface = UiSurface::new("electronics.footprint.inspector", palette, root);
    surface.style_sheet = footprint_chooser_style_sheet(palette);
    surface
}

/// Style rules to merge when `footprint_chooser_content` is embedded in a host
/// surface with other panels and overlays.
pub fn footprint_chooser_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut rules = vec![
        electronics_class_rule(
            "electronics-footprint-chooser",
            tokens.background,
            with_alpha(tokens.border, 154),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-header",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-list",
            with_alpha(tokens.canvas, 238),
            with_alpha(tokens.border, 110),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-detail",
            tokens.surface,
            with_alpha(tokens.border, 110),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-input",
            tokens.surface_alt,
            with_alpha(tokens.border, 150),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-candidate",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-candidate-selected",
            with_alpha(tokens.accent, 24),
            with_alpha(tokens.accent, 116),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-action",
            tokens.surface_alt,
            with_alpha(tokens.border, 140),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-action-primary",
            with_alpha(tokens.accent, 216),
            tokens.accent,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-canvas",
            tokens.canvas,
            with_alpha(tokens.border, 118),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-bounds",
            with_alpha(tokens.surface_raised, 178),
            with_alpha(tokens.info, 170),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-pad",
            with_alpha(tokens.accent, 230),
            tokens.accent,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-pad-through-hole",
            with_alpha(tokens.warning, 232),
            tokens.warning,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-status-ready",
            with_alpha(tokens.positive, 32),
            with_alpha(tokens.positive, 118),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-footprint-status-review",
            with_alpha(tokens.warning, 32),
            with_alpha(tokens.warning, 120),
            tokens.text,
        ),
    ];
    for class in [
        "electronics-footprint-input",
        "electronics-footprint-candidate",
        "electronics-footprint-action",
        "electronics-footprint-action-primary",
    ] {
        rules.push(electronics_hover_rule(class, tokens));
        rules.push(electronics_focus_rule(class, tokens));
        rules.push(electronics_active_rule(class, tokens));
    }
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-footprint-candidate",
        UiStylePatch {
            fill: Some(with_alpha(tokens.surface_raised, 104)),
            ..Default::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Focused,
        "electronics-footprint-candidate",
        UiStylePatch {
            border: Some(tokens.focus),
            ..Default::default()
        },
    ));
    UiStyleSheet { rules }
}

fn header(palette: StudioUiPalette, browsing: bool) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    UiNode::new("electronics.footprint.header", UiNodeKind::Toolbar)
        .with_class("electronics-footprint-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 9.0,
            ..UiLayout::fixed(0.0, 42.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.header.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::Pcb)
                        .with_size(UiIconSize::Panel)
                        .with_tint(tokens.accent),
                )
                .with_layout(UiLayout::fixed(22.0, 22.0)),
        )
        .with_child(
            UiNode::new("electronics.footprint.header.text", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 0.0,
                    grow: 1.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("electronics.footprint.title", UiNodeKind::Label)
                        .with_text_key(if browsing {
                            "electronics.desktop.footprints"
                        } else {
                            "electronics.footprint.title"
                        })
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 15.0,
                            line_height_px: 19.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        })
                        .with_layout(UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(
                    UiNode::new("electronics.footprint.subtitle", UiNodeKind::Label)
                        .with_text_key(if browsing {
                            "electronics.footprint.preview"
                        } else {
                            "electronics.footprint.subtitle"
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(
                            UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT)
                                .with_width_mode(UiSizeMode::Fill),
                        ),
                ),
        )
        .with_child(icon_button(
            palette,
            "electronics.footprint.close",
            UiIconId::Close,
            "electronics.footprint.close",
            "electronics.footprint.close",
        ))
}

fn search_field(palette: StudioUiPalette) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    UiNode::new("electronics.footprint.search.row", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            ..UiLayout::fixed(0.0, SEARCH_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.search.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::Search)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
        .with_child(
            UiNode::text_input(
                "electronics.footprint.query.input",
                UiTextInput {
                    value_key: "electronics.footprint.query".to_string(),
                    placeholder_key: Some("electronics.footprint.search_placeholder".to_string()),
                    max_length: 192,
                    multiline: false,
                    password: false,
                    submit_command: Some("electronics.footprint.query".to_string()),
                },
            )
            .with_class("electronics-footprint-input")
            .with_layout(
                UiLayout::fixed(0.0, SEARCH_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill)
                    .grow(1.0)
                    .with_text_safe_area(true),
            )
            .with_accessibility_role(UiAccessibilityRole::Textbox)
            .with_accessibility_label_key("electronics.footprint.search")
            .with_accessibility_description_key("electronics.footprint.search_help")
            .with_tooltip_key("electronics.footprint.search_help")
            .focusable()
            .with_text_style(electronics_body_style(tokens.text))
            .with_event(close_on_escape()),
        )
}

fn target_context(palette: StudioUiPalette, model: &FootprintChooserModel) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let designator = model.component_designator.trim();
    let footprint = model.current_footprint.trim();
    let value = match (designator.is_empty(), footprint.is_empty()) {
        (false, false) => format!("{designator}  ·  {footprint}"),
        (false, true) => designator.to_string(),
        (true, false) => footprint.to_string(),
        (true, true) => "—".to_string(),
    };
    UiNode::new("electronics.footprint.target", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.target.label", UiNodeKind::Label)
                .with_text_key("electronics.footprint.target_label")
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::FitContent)),
        )
        .with_child(
            UiNode::new("electronics.footprint.target.value", UiNodeKind::Label)
                .with_text_value(value)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0).fill_width().grow(1.0)),
        )
}

fn wide_body(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    available_size: [f32; 2],
) -> UiNode {
    let list_width = 274.0_f32.min((available_size[0] * 0.38).max(224.0));
    UiNode::new("electronics.footprint.body", UiNodeKind::Panel)
        .with_class("electronics-footprint-body")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Stretch,
            gap: 12.0,
            grow: 1.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_child(candidate_list(palette, model, list_width, 0.0))
        .with_child(selected_detail(
            palette,
            model,
            [
                (available_size[0] - list_width - 48.0).max(180.0),
                PREVIEW_HEIGHT + 170.0,
            ],
            true,
        ))
}

fn compact_body(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    available_size: [f32; 2],
) -> UiNode {
    let detail_width = (available_size[0] - 36.0).max(180.0);
    UiNode::scroll_view("electronics.footprint.body", UiScrollAxis::Vertical)
        .with_class("electronics-footprint-body")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(candidate_list(palette, model, detail_width, 164.0))
        .with_child(selected_detail(
            palette,
            model,
            [detail_width, PREVIEW_HEIGHT + 170.0],
            false,
        ))
}

fn candidate_list(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    width: f32,
    height: f32,
) -> UiNode {
    let fixed_height = if height > 0.0 { height } else { 0.0 };
    let mut list = UiNode::scroll_view("electronics.footprint.candidates", UiScrollAxis::Vertical)
        .with_class("electronics-footprint-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::same(6.0),
            width_mode: if width > 0.0 {
                UiSizeMode::Fixed
            } else {
                UiSizeMode::Fill
            },
            height_mode: if fixed_height > 0.0 {
                UiSizeMode::Fixed
            } else {
                UiSizeMode::Fill
            },
            basis: [width, fixed_height],
            grow: if width > 0.0 { 0.0 } else { 1.0 },
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    if model.candidates.is_empty() {
        list = list.with_child(
            UiNode::new("electronics.footprint.no_results", UiNodeKind::Label)
                .with_text_key("electronics.footprint.no_results")
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(electronics_body_style(
                    crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
                ))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
        return list;
    }
    for (index, candidate) in model.candidates.iter().enumerate() {
        list = list.with_child(candidate_row(
            palette,
            index,
            model.candidates.len(),
            candidate,
            model.selected_index,
            &model.component_pins,
            model.is_browsing(),
        ));
    }
    list
}

fn candidate_row(
    palette: StudioUiPalette,
    index: usize,
    candidate_count: usize,
    candidate: &FootprintChooserCandidate,
    selected_index: Option<usize>,
    component_pins: &[String],
    browsing: bool,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let id = format!("electronics.footprint.candidate.{index}");
    let selected = selected_index == Some(index);
    let compatible = footprint_mapping_is_compatible(candidate, component_pins);
    let status_key = if compatible {
        "electronics.footprint.compatible"
    } else {
        "electronics.footprint.incompatible"
    };
    let mut row = UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class(if selected {
            "electronics-footprint-candidate electronics-footprint-candidate-selected"
        } else {
            "electronics-footprint-candidate"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(7.0, 4.0),
            ..UiLayout::fixed(0.0, CANDIDATE_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key("electronics.footprint.select_candidate")
        .with_tooltip_value(candidate.name.clone())
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("electronics.footprint.select:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "electronics.footprint.close",
        ));
    if index > 0 {
        row = row.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("arrowup".to_string()),
            format!("electronics.footprint.select:{}", index - 1),
        ));
    }
    if index + 1 < candidate_count {
        row = row.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("arrowdown".to_string()),
            format!("electronics.footprint.select:{}", index + 1),
        ));
    }
    row = row
        .with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if browsing {
                        UiIconId::Pcb
                    } else if compatible {
                        UiIconId::Success
                    } else {
                        UiIconId::Warning
                    })
                    .with_size(UiIconSize::Small)
                    .with_tint(if browsing {
                        tokens.text_muted
                    } else if compatible {
                        tokens.positive
                    } else {
                        tokens.warning
                    }),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.text"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 0.0,
                    grow: 1.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new(format!("{id}.name"), UiNodeKind::Label)
                        .with_text_value(candidate.name.clone())
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(electronics_body_style(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(
                    UiNode::new(format!("{id}.source"), UiNodeKind::Label)
                        .with_text_value(candidate.source.clone())
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
                ),
        );
    if !browsing {
        row = row.with_child(
            UiNode::new(format!("{id}.status"), UiNodeKind::Label)
                .with_text_key(status_key)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(if compatible {
                    tokens.positive
                } else {
                    tokens.warning
                }))
                .with_layout(UiLayout::fit_content()),
        );
    }
    row
}

fn selected_detail(
    palette: StudioUiPalette,
    model: &FootprintChooserModel,
    available_size: [f32; 2],
    grow: bool,
) -> UiNode {
    let Some(candidate) = model
        .selected_index
        .and_then(|index| model.candidates.get(index))
    else {
        return UiNode::new("electronics.footprint.detail", UiNodeKind::Panel)
            .with_class("electronics-footprint-detail")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                gap: 8.0,
                padding: UiSpacing::same(18.0),
                grow: if grow { 1.0 } else { 0.0 },
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("electronics.footprint.detail.icon", UiNodeKind::Label)
                    .with_icon(UiIcon::new(UiIconId::Pcb).with_size(UiIconSize::Panel)),
            )
            .with_child(
                UiNode::new("electronics.footprint.detail.empty", UiNodeKind::Label)
                    .with_text_key("electronics.footprint.select_hint")
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .with_text_style(electronics_body_style(
                        crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
                    )),
            );
    };

    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let width = available_size[0].max(180.0);
    let preview_width = (width - 28.0).clamp(172.0, 420.0);
    let mut detail = UiNode::new("electronics.footprint.detail", UiNodeKind::Panel)
        .with_class("electronics-footprint-detail")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(10.0),
            grow: if grow { 1.0 } else { 0.0 },
            overflow: if grow {
                UiOverflow::ScrollY
            } else {
                UiOverflow::Visible
            },
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.detail.name", UiNodeKind::Label)
                .with_text_value(candidate.name.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 13.0,
                    line_height_px: 17.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
        );
    let preview_height = PREVIEW_HEIGHT;
    detail = detail
        .with_child(physical_preview(
            palette,
            candidate,
            [preview_width, preview_height],
            model.preview_image_key.as_deref(),
        ))
        .with_child(dimensions_row(palette, candidate, width < 360.0));
    if !model.is_browsing() {
        detail = detail
            .with_child(compatibility_status(
                palette,
                candidate,
                &model.component_pins,
            ))
            .with_child(mapping_list(palette, candidate, &model.component_pins));
    } else if candidate.fabrication_ready {
        detail = detail.with_child(
            UiNode::new(
                "electronics.footprint.compatibility.fabrication",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.footprint.fabrication_ready")
            .with_text_style(electronics_body_style(tokens.positive))
            .with_layout(UiLayout::fixed(0.0, 24.0).fill_width()),
        );
    }
    if !candidate.description.trim().is_empty() {
        detail = detail.with_child(
            UiNode::new(
                "electronics.footprint.detail.description",
                UiNodeKind::Label,
            )
            .with_text_value(candidate.description.clone())
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_text_style(electronics_body_style(tokens.text_muted))
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    if !candidate.tags.is_empty() {
        detail = detail.with_child(
            UiNode::new("electronics.footprint.detail.tags", UiNodeKind::Label)
                .with_text_value(candidate.tags.join(" · "))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    if !candidate.fabrication_ready {
        detail = detail.with_child(
            UiNode::new(
                "electronics.footprint.detail.fabrication",
                UiNodeKind::Label,
            )
            .with_class("electronics-footprint-status-review")
            .with_text_value(if candidate.fabrication_detail.trim().is_empty() {
                "".to_string()
            } else {
                candidate.fabrication_detail.clone()
            })
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_accessibility_role(UiAccessibilityRole::Status)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    detail
}

fn physical_preview(
    palette: StudioUiPalette,
    _candidate: &FootprintChooserCandidate,
    size: [f32; 2],
    image_key: Option<&str>,
) -> UiNode {
    let mut node = UiNode::new("electronics.footprint.preview", UiNodeKind::Panel)
        .with_class("electronics-footprint-preview")
        .with_layout(UiLayout::fixed(size[0], size[1]))
        .with_accessibility_label_key("electronics.footprint.preview");
    if let Some(key) = image_key {
        node = node.with_child(
            UiNode::image(
                "electronics.footprint.preview.image",
                raf_ui::UiImage {
                    source: raf_ui::UiImageSource::new(key),
                    fit: raf_ui::UiImageFit::Contain,
                    tint: None,
                    options: Default::default(),
                },
            )
            .with_layout(UiLayout::fill(UiFlow::Column)),
        );
    } else {
        node = node.with_child(
            UiNode::new(
                "electronics.footprint.preview.no_geometry",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.footprint.preview_no_geometry")
            .with_text_style(electronics_body_style(
                crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
            )),
        );
    }
    node
}
fn dimensions_row(
    palette: StudioUiPalette,
    candidate: &FootprintChooserCandidate,
    compact: bool,
) -> UiNode {
    let size = candidate.bounds_size_mm;
    let text = format!("{:.2} × {:.2} mm", positive(size[0]), positive(size[1]));
    let mut row = UiNode::new("electronics.footprint.dimensions", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.dimensions.label", UiNodeKind::Label)
                .with_text_key("electronics.footprint.dimensions")
                .with_layout(UiLayout::fit_content())
                .with_text_style(electronics_body_style(
                    crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
                )),
        )
        .with_child(
            UiNode::new("electronics.footprint.dimensions.value", UiNodeKind::Label)
                .with_text_value(text)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .grow(1.0),
                )
                .with_text_style(electronics_body_style(
                    crate::panels::electronics_surface::electronics_tokens(palette).text,
                )),
        )
        .with_child(
            UiNode::new("electronics.footprint.dimensions.pads", UiNodeKind::Label)
                .with_text_value(candidate.pads.len().to_string())
                .with_layout(UiLayout::fit_content())
                .with_text_style(electronics_body_style(
                    crate::panels::electronics_surface::electronics_tokens(palette).text,
                )),
        )
        .with_child(
            UiNode::new(
                "electronics.footprint.dimensions.pads_label",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.footprint.pads")
            .with_layout(UiLayout::fit_content())
            .with_text_style(electronics_body_style(
                crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
            )),
        );
    if compact {
        // In an inspector, preserve the complete dimensions rather than squeezing
        // four fields into one line. Keep the same data and accessible node IDs.
        let mut fields = row.children.drain(..);
        let mut label = fields.next().expect("dimensions label");
        let mut value = fields.next().expect("dimensions value");
        let mut pads = fields.next().expect("pad count");
        let mut pads_label = fields.next().expect("pad label");
        label.layout = UiLayout::fixed(88.0, ELECTRONICS_ROW_HEIGHT);
        pads_label.layout = label.layout.clone();
        value.layout = UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT)
            .fill_width()
            .grow(1.0);
        pads.layout = value.layout.clone();
        let line_layout = UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).fill_width()
        };
        return UiNode::new("electronics.footprint.dimensions", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 2.0,
                ..UiLayout::fit_content().fill_width()
            })
            .with_child(
                UiNode::new(
                    "electronics.footprint.dimensions.size-row",
                    UiNodeKind::Panel,
                )
                .with_layout(line_layout.clone())
                .with_child(label)
                .with_child(value),
            )
            .with_child(
                UiNode::new(
                    "electronics.footprint.dimensions.pads-row",
                    UiNodeKind::Panel,
                )
                .with_layout(line_layout)
                .with_child(pads_label)
                .with_child(pads),
            );
    }
    row
}

fn compatibility_status(
    palette: StudioUiPalette,
    candidate: &FootprintChooserCandidate,
    component_pins: &[String],
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let compatible = footprint_mapping_is_compatible(candidate, component_pins);
    let status_key = if compatible {
        "electronics.footprint.compatible"
    } else {
        "electronics.footprint.incompatible"
    };
    let mut row = UiNode::new("electronics.footprint.compatibility", UiNodeKind::Toolbar)
        .with_class(if compatible {
            "electronics-footprint-status-ready"
        } else {
            "electronics-footprint-status-review"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(7.0, 2.0),
            ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(
                "electronics.footprint.compatibility.icon",
                UiNodeKind::Label,
            )
            .with_icon(
                UiIcon::new(if compatible {
                    UiIconId::Success
                } else {
                    UiIconId::Warning
                })
                .with_size(UiIconSize::Small)
                .with_tint(if compatible {
                    tokens.positive
                } else {
                    tokens.warning
                }),
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
        .with_child(
            UiNode::new(
                "electronics.footprint.compatibility.label",
                UiNodeKind::Label,
            )
            .with_text_key(status_key)
            .with_layout(UiLayout::fit_content())
            .with_text_style(electronics_body_style(tokens.text)),
        );
    if !compatible && !candidate.compatibility_detail.trim().is_empty() {
        row = row.with_child(
            UiNode::new(
                "electronics.footprint.compatibility.detail",
                UiNodeKind::Label,
            )
            .with_text_value(candidate.compatibility_detail.clone())
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text_muted))
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    if candidate.fabrication_ready {
        row = row.with_child(
            UiNode::new(
                "electronics.footprint.compatibility.fabrication",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.footprint.fabrication_ready")
            .with_layout(
                UiLayout::fit_content()
                    .with_width_mode(UiSizeMode::Fill)
                    .grow(1.0),
            )
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.positive)),
        );
    }
    row
}

fn mapping_list(
    palette: StudioUiPalette,
    candidate: &FootprintChooserCandidate,
    component_pins: &[String],
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut list = UiNode::new("electronics.footprint.mapping", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(0.0, 2.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.mapping.title", UiNodeKind::Label)
                .with_text_key("electronics.footprint.pin_pad_mapping")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 11.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
        );
    let pins = if component_pins.is_empty() {
        candidate
            .pin_pad_mapping
            .iter()
            .map(|(pin, _)| pin.clone())
            .collect::<Vec<_>>()
    } else {
        component_pins.to_vec()
    };
    if pins.is_empty() {
        list = list.with_child(
            UiNode::new("electronics.footprint.mapping.empty", UiNodeKind::Label)
                .with_text_key("electronics.footprint.mapping_unavailable")
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
        return list;
    }
    for (index, pin) in pins.iter().enumerate() {
        let pad = candidate
            .pin_pad_mapping
            .iter()
            .find(|(mapped_pin, _)| mapped_pin == pin)
            .map_or("", |(_, pad)| pad.as_str());
        let pad_value = if pad.trim().is_empty() {
            "—".to_string()
        } else {
            pad.to_string()
        };
        list = list.with_child(
            UiNode::new(
                format!("electronics.footprint.mapping.{index}"),
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("electronics.footprint.mapping.{index}.pin"),
                    UiNodeKind::Label,
                )
                .with_text_value(pin.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            )
            .with_child(
                UiNode::new(
                    format!("electronics.footprint.mapping.{index}.arrow"),
                    UiNodeKind::Label,
                )
                .with_icon(UiIcon::new(UiIconId::ChevronRight).with_size(UiIconSize::Small))
                .with_layout(UiLayout::fixed(16.0, 16.0)),
            )
            .with_child(
                UiNode::new(
                    format!("electronics.footprint.mapping.{index}.pad"),
                    UiNodeKind::Button,
                )
                .with_class("electronics-footprint-action")
                .with_layout(UiLayout::fixed(68.0, 25.0))
                .with_text_value(pad_value.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.accent))
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_label_key("electronics.footprint.map_pad")
                .with_tooltip_value(format!("{pin} → {pad_value}"))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("electronics.footprint.map:{index}"),
                ))
                .with_event(close_on_escape()),
            ),
        );
    }
    list
}

fn import_field(palette: StudioUiPalette, pending: bool) -> UiNode {
    UiNode::new("electronics.footprint.import.row", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            ..UiLayout::fixed(0.0, IMPORT_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.footprint.import.path-icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::File).with_size(UiIconSize::Small))
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::text_input(
                "electronics.footprint.import_path.input",
                UiTextInput {
                    value_key: "electronics.footprint.import_path".to_string(),
                    placeholder_key: Some(
                        "electronics.footprint.import_path_placeholder".to_string(),
                    ),
                    max_length: 2048,
                    multiline: false,
                    password: false,
                    submit_command: Some("electronics.footprint.import".to_string()),
                },
            )
            .with_class("electronics-footprint-input")
            .with_layout(
                UiLayout::fixed(0.0, IMPORT_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill)
                    .grow(1.0)
                    .with_text_safe_area(true),
            )
            .with_accessibility_role(UiAccessibilityRole::Textbox)
            .with_accessibility_label_key("electronics.footprint.import_path")
            .with_tooltip_key("electronics.footprint.import_path_help")
            .with_text_style(electronics_body_style(
                crate::panels::electronics_surface::electronics_tokens(palette).text,
            ))
            .focusable()
            .with_event(close_on_escape()),
        )
        .with_child(
            action_button(
                palette,
                "electronics.footprint.import.button",
                "electronics.footprint.import_file",
                "electronics.footprint.import",
                UiIconId::Add,
                false,
            )
            .disabled(pending),
        )
}

fn import_status(palette: StudioUiPalette, model: &FootprintChooserModel) -> UiNode {
    let mut status = UiNode::new("electronics.footprint.import.status", UiNodeKind::Label)
        .with_class(if model.import_error {
            "electronics-footprint-status-review"
        } else {
            "electronics-footprint-status-ready"
        })
        .with_text_value(model.import_status.clone())
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_text_style(electronics_body_style(if model.import_error {
            crate::panels::electronics_surface::electronics_tokens(palette).warning
        } else {
            crate::panels::electronics_surface::electronics_tokens(palette).positive
        }))
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill));
    if model.import_status.trim().is_empty() {
        status.layout = UiLayout::fixed(0.0, 0.0).with_width_mode(UiSizeMode::Fill);
    }
    status
}

fn footer(palette: StudioUiPalette, model: &FootprintChooserModel) -> UiNode {
    let mut footer = UiNode::new("electronics.footprint.footer", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::End,
            gap: 8.0,
            ..UiLayout::fixed(0.0, FOOTER_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(action_button(
            palette,
            "electronics.footprint.cancel",
            if model.is_browsing() {
                "app.close"
            } else {
                "electronics.footprint.cancel"
            },
            "electronics.footprint.close",
            UiIconId::Close,
            false,
        ));
    if !model.is_browsing() {
        footer = footer.with_child(
            action_button(
                palette,
                "electronics.footprint.assign",
                "electronics.footprint.assign",
                "electronics.footprint.assign",
                UiIconId::Success,
                true,
            )
            .disabled(
                model.component_designator.trim().is_empty()
                    || !model
                        .selected_index
                        .and_then(|index| model.candidates.get(index))
                        .is_some_and(|candidate| {
                            footprint_mapping_is_compatible(candidate, &model.component_pins)
                        }),
            ),
        );
    }
    footer
}

fn icon_button(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    command: &str,
) -> UiNode {
    let tint = crate::panels::electronics_surface::electronics_tokens(palette).text_muted;
    UiNode::new(id, UiNodeKind::Button)
        .with_class("electronics-footprint-action")
        .with_layout(UiLayout::fixed(30.0, 30.0))
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(tint),
        )
        .with_tooltip_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(close_on_escape())
}

fn action_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    icon: UiIconId,
    primary: bool,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if primary {
            "electronics-footprint-action electronics-footprint-action-primary"
        } else {
            "electronics-footprint-action"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            padding: UiSpacing::xy(9.0, 4.0),
            min_size: [88.0, FOOTER_HEIGHT],
            ..UiLayout::fixed(0.0, FOOTER_HEIGHT)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(label_key)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(label_key)
        .with_tooltip_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(close_on_escape())
}

fn close_on_escape() -> UiEventBinding {
    UiEventBinding::command(
        UiEventKind::KeyPress("escape".to_string()),
        "electronics.footprint.close",
    )
}

/// A footprint is ready to assign only when supported physical geometry and
/// an explicit pin-to-numbered-pad mapping cover the complete component.
pub fn footprint_mapping_is_compatible(
    candidate: &FootprintChooserCandidate,
    component_pins: &[String],
) -> bool {
    if !candidate.fabrication_ready || candidate.pads.is_empty() {
        return false;
    }
    let pins = if component_pins.is_empty() {
        candidate
            .pin_pad_mapping
            .iter()
            .map(|(pin, _)| pin.as_str())
            .collect::<Vec<_>>()
    } else {
        component_pins
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    };
    if pins.is_empty() {
        return candidate.compatible;
    }
    let mapped_pads = pins
        .iter()
        .map(|pin| {
            let mut matches = candidate
                .pin_pad_mapping
                .iter()
                .filter(|(mapped_pin, _)| mapped_pin == pin);
            let pad = matches.next()?.1.trim();
            (matches.next().is_none() && !pad.is_empty()).then_some(pad)
        })
        .collect::<Option<Vec<_>>>();
    let Some(mapped_pads) = mapped_pads else {
        return false;
    };
    if mapped_pads
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != mapped_pads.len()
    {
        return false;
    }
    if mapped_pads.iter().any(|mapped| {
        !candidate
            .pads
            .iter()
            .any(|pad| !pad.number.trim().is_empty() && pad.number == *mapped)
    }) {
        return false;
    }
    candidate
        .pads
        .iter()
        .filter(|pad| !pad.number.trim().is_empty())
        .all(|pad| mapped_pads.iter().any(|mapped| *mapped == pad.number))
}

fn positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;
    use raf_render::api_graphic_basic::ui_surface::UiSurfaceSession;
    use raf_ui::{UiAction, UiInputState};

    fn candidate(id: &str) -> FootprintChooserCandidate {
        FootprintChooserCandidate {
            id: id.to_string(),
            name: "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm".to_string(),
            source: "Project library · Test.pretty".to_string(),
            description: "Eight-pin small outline package".to_string(),
            tags: vec!["SOIC".to_string(), "SMD".to_string()],
            bounds_min_mm: [-3.0, -2.0],
            bounds_size_mm: [6.0, 4.0],
            pads: vec![
                FootprintChooserPad {
                    number: "1".to_string(),
                    position_mm: [-2.0, -1.0],
                    size_mm: [1.2, 0.6],
                    shape: "rect".to_string(),
                    rotation_degrees: 0.0,
                },
                FootprintChooserPad {
                    number: "2".to_string(),
                    position_mm: [2.0, -1.0],
                    size_mm: [1.2, 0.6],
                    shape: "roundrect".to_string(),
                    rotation_degrees: 90.0,
                },
            ],
            pin_pad_mapping: vec![
                ("SDA".to_string(), "1".to_string()),
                ("SCL".to_string(), "".to_string()),
            ],
            compatible: false,
            compatibility_detail: "SCL has no mapped pad".to_string(),
            fabrication_ready: false,
            fabrication_detail: "Unsupported source feature: model".to_string(),
        }
    }

    fn model() -> FootprintChooserModel {
        FootprintChooserModel {
            import_pending: false,
            preview_image_key: Some("electronics://footprint-preview".into()),
            query: "SOIC".to_string(),
            import_path: String::new(),
            component_designator: String::new(),
            component_pins: vec!["SDA".to_string(), "SCL".to_string()],
            current_footprint: "SOIC-8".to_string(),
            import_status: String::new(),
            import_error: false,
            selected_index: Some(0),
            candidates: vec![candidate("project-soic-8")],
        }
    }

    #[test]
    fn browsing_geometry_does_not_claim_component_compatibility_or_offer_assignment() {
        let mut model = model();
        model.component_pins.clear();
        model.current_footprint.clear();
        let surface = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model,
            [880.0, 540.0],
        );
        assert_eq!(
            surface.root.accessibility_label_key.as_deref(),
            Some("electronics.desktop.footprints")
        );
        for id in [
            "electronics.footprint.target",
            "electronics.footprint.candidate.0.status",
            "electronics.footprint.compatibility.label",
            "electronics.footprint.mapping",
            "electronics.footprint.assign",
        ] {
            assert!(
                surface.root.find(id).is_none(),
                "{id} requires a component target"
            );
        }
        assert!(surface.root.find("electronics.footprint.preview").is_some());
        assert!(surface
            .root
            .find("electronics.footprint.dimensions")
            .is_some());
        assert_eq!(
            surface
                .root
                .find("electronics.footprint.cancel")
                .unwrap()
                .text_key
                .as_deref(),
            Some("app.close")
        );
    }

    #[test]
    fn candidate_rows_and_selection_commands_come_from_the_supplied_model() {
        let model = model();
        let surface = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model,
            [980.0, 720.0],
        );
        let row = surface
            .root
            .find("electronics.footprint.candidate.0")
            .expect("model candidate row");
        assert_eq!(row.accessibility_role, UiAccessibilityRole::Button);
        assert!(row.event_handlers.iter().any(|event| {
            matches!(&event.action, UiAction::Command { name }
                if name == "electronics.footprint.select:0")
        }));
        assert_eq!(
            surface
                .root
                .find("electronics.footprint.candidate.0.name")
                .and_then(|node| node.text_value.as_deref()),
            Some("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm")
        );
        assert!(surface
            .root
            .find("electronics.footprint.candidate.1")
            .is_none());
        assert!(surface
            .root
            .find("electronics.footprint.assign")
            .is_some_and(|node| node.disabled));
    }

    #[test]
    fn search_and_import_inputs_publish_stable_host_state_keys() {
        let surface = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model(),
            [980.0, 720.0],
        );
        for (id, key, submit) in [
            (
                "electronics.footprint.query.input",
                "electronics.footprint.query",
                Some("electronics.footprint.query"),
            ),
            (
                "electronics.footprint.import_path.input",
                "electronics.footprint.import_path",
                Some("electronics.footprint.import"),
            ),
        ] {
            let input = surface.root.find(id).expect("text input");
            let control = input.control.text_input().expect("input control");
            assert_eq!(control.value_key, key);
            assert_eq!(control.submit_command.as_deref(), submit);
        }
        for (id, command) in [
            (
                "electronics.footprint.import.button",
                "electronics.footprint.import",
            ),
            (
                "electronics.footprint.assign",
                "electronics.footprint.assign",
            ),
            (
                "electronics.footprint.cancel",
                "electronics.footprint.close",
            ),
            ("electronics.footprint.close", "electronics.footprint.close"),
        ] {
            let action = surface.root.find(id).expect("action button");
            assert!(
                action.event_handlers.iter().any(|event| {
                    matches!(&event.action, UiAction::Command { name } if name == command)
                }),
                "{id} should emit {command}"
            );
            assert!(
                action.event_handlers.iter().any(|event| {
                    matches!(&event.event, UiEventKind::KeyPress(key) if key == "escape")
                }),
                "{id} should close on Escape"
            );
        }
    }

    #[test]
    fn typing_in_search_dispatches_set_text_with_the_query_key() {
        let surface = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model(),
            [980.0, 720.0],
        );
        let mut session = UiSurfaceSession::default();
        session
            .interaction
            .focus
            .request_focus("electronics.footprint.query.input");
        let frame = session.build_layout_frame_at_scale(&surface, 980, 720, [0; 4], 1.0);
        let actions = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                text_input: "0603".to_string(),
                ..Default::default()
            },
        );
        assert!(actions.iter().any(|action| matches!(
            &action.action,
            UiAction::SetText { key, value }
                if key == "electronics.footprint.query" && value == "0603"
        )));
    }

    #[test]
    fn preview_uses_real_candidate_bounds_and_pad_dimensions() {
        let surface = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model(),
            [980.0, 720.0],
        );
        let preview = surface
            .root
            .find("electronics.footprint.preview")
            .expect("physical preview canvas");
        assert!(preview
            .find("electronics.footprint.preview.image")
            .is_some());
        assert!(
            preview
                .find("electronics.footprint.preview.pad.1")
                .is_none(),
            "Physical previews must not substitute rotated bounding boxes for pads"
        );
        let details = surface
            .root
            .find("electronics.footprint.dimensions.value")
            .expect("physical dimensions");
        assert_eq!(details.text_value.as_deref(), Some("6.00 × 4.00 mm"));
        assert_eq!(
            surface
                .root
                .find("electronics.footprint.dimensions.pads")
                .and_then(|node| node.text_value.as_deref()),
            Some("2")
        );
    }

    #[test]
    fn pin_pad_mapping_displays_unmatched_pins_and_fabrication_limits() {
        let surface = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model(),
            [980.0, 720.0],
        );
        assert_eq!(
            surface
                .root
                .find("electronics.footprint.mapping.1.pad")
                .and_then(|node| node.text_value.as_deref()),
            Some("—")
        );
        assert_eq!(
            surface
                .root
                .find("electronics.footprint.compatibility.label")
                .and_then(|node| node.text_key.as_deref()),
            Some("electronics.footprint.incompatible")
        );
        assert!(surface
            .root
            .find("electronics.footprint.detail.fabrication")
            .is_some());
        assert!(surface
            .root
            .find("electronics.footprint.mapping.1.pad")
            .is_some_and(|button| button.event_handlers.iter().any(|event| {
                matches!(&event.action, UiAction::Command { name }
                    if name == "electronics.footprint.map:1")
            })));
    }

    #[test]
    fn chooser_adapts_between_wide_and_compact_layouts_and_passes_layout_gate() {
        let compact = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model(),
            [360.0, 640.0],
        );
        assert_eq!(
            compact
                .root
                .find("electronics.footprint.body")
                .expect("compact body")
                .layout
                .flow,
            UiFlow::Column
        );
        assert_electronics_layout_gate(&compact, 360, 640);

        let wide = build_electronics_footprint_surface(
            StudioUiPalette::IndustrialDark,
            &model(),
            [980.0, 720.0],
        );
        assert_eq!(
            wide.root
                .find("electronics.footprint.body")
                .expect("wide body")
                .layout
                .flow,
            UiFlow::Row
        );
        assert_electronics_layout_gate(&wide, 980, 720);
    }

    #[test]
    fn cpu_surface_layout_renders_and_can_export_a_review_png() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;

        let mut qa_model = model();
        qa_model.query.clear();
        qa_model.current_footprint = "0805".into();
        qa_model.component_designator = "R1".to_string();
        let mut qa_candidate = candidate("qa-soic-8");
        qa_candidate.pin_pad_mapping = vec![
            ("SDA".to_string(), "1".to_string()),
            ("SCL".to_string(), "2".to_string()),
        ];
        qa_candidate.compatible = true;
        qa_candidate.compatibility_detail.clear();
        qa_candidate.fabrication_ready = true;
        qa_candidate.fabrication_detail.clear();
        qa_model.component_pins = vec!["1".into(), "2".into()];
        qa_candidate.name = "R_0805_2012Metric".into();
        qa_candidate.description = "".into();
        qa_candidate.tags = vec!["SMD".into(), "0805".into()];
        qa_candidate.bounds_size_mm = [3.0, 1.4];
        qa_candidate.pin_pad_mapping = vec![("1".into(), "1".into()), ("2".into(), "2".into())];
        qa_model.candidates = vec![qa_candidate];
        qa_model.selected_index = Some(0);

        let mut host = CpuUiSurfaceHost::new(
            build_electronics_footprint_surface(
                StudioUiPalette::IndustrialDark,
                &qa_model,
                [980.0, 720.0],
            ),
            [12, 15, 21, 255],
        );
        let asset=raf_electronics::pcb::parse_kicad_footprint(r#"(footprint "R_0805_2012Metric"
            (pad "1" smd rect (at -1 0) (size 1 1.4) (layers "F.Cu" "F.Mask"))
              (pad "2" smd roundrect (at 1 0) (size 1 1.4) (roundrect_rratio 0.2) (layers "F.Cu" "F.Mask")))"#).unwrap();
        let (size, pixels) = crate::electronics_minimap::footprint_preview_rgba(
            &asset,
            StudioUiPalette::IndustrialDark,
        );
        host.images_mut()
            .insert_rgba("electronics://footprint-preview", size, pixels)
            .unwrap();
        let frame = host.render([980, 720], |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });
        let preview_pad = frame
            .frame
            .layout_boxes
            .iter()
            .find(|node| node.id == "electronics.footprint.preview.image")
            .expect("the physical geometry image is laid out");
        assert!(preview_pad.rect.width > 0.0 && preview_pad.rect.height > 0.0);
        for (id, minimum_width) in [
            ("electronics.footprint.query.input", 800.0),
            ("electronics.footprint.import_path.input", 700.0),
        ] {
            let input = frame
                .frame
                .layout_boxes
                .iter()
                .find(|node| node.id == id)
                .unwrap();
            assert!(input.rect.width >= minimum_width, "{id}: {:?}", input.rect);
        }
        assert!(frame.pixels.chunks_exact(4).any(|pixel| {
            pixel[0] != 12 || pixel[1] != 15 || pixel[2] != 21 || pixel[3] != 255
        }));

        if let Some(path) = std::env::var_os("RAF_ELECTRONICS_FOOTPRINT_QA_PNG") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).expect("create QA image directory");
            }
            image::save_buffer(
                path,
                frame.pixels,
                frame.size[0],
                frame.size[1],
                image::ColorType::Rgba8,
            )
            .expect("save optional footprint chooser QA image");
        }
    }

    #[test]
    fn chooser_and_fabrication_copy_has_english_and_spanish_translations() {
        let keys = [
            "electronics.footprint.title",
            "electronics.footprint.subtitle",
            "electronics.footprint.section_title",
            "electronics.footprint.change",
            "electronics.footprint.target_label",
            "electronics.footprint.search",
            "electronics.footprint.search_help",
            "electronics.footprint.search_placeholder",
            "electronics.footprint.no_results",
            "electronics.footprint.select_hint",
            "electronics.footprint.select_candidate",
            "electronics.footprint.compatible",
            "electronics.footprint.incompatible",
            "electronics.footprint.dimensions",
            "electronics.footprint.pads",
            "electronics.footprint.preview",
            "electronics.footprint.preview_no_geometry",
            "electronics.footprint.pin_pad_mapping",
            "electronics.footprint.mapping_unavailable",
            "electronics.footprint.map_pad",
            "electronics.footprint.fabrication_ready",
            "electronics.footprint.import_file",
            "electronics.footprint.import_path",
            "electronics.footprint.import_path_placeholder",
            "electronics.footprint.import_path_help",
            "electronics.footprint.import_path_required",
            "electronics.footprint.import_bad_extension",
            "electronics.footprint.imported",
            "electronics.footprint.import_failed",
            "electronics.footprint.assign",
            "electronics.footprint.cancel",
            "electronics.footprint.close",
            "electronics.footprint.target_missing",
            "electronics.footprint.target_locked",
            "electronics.footprint.unmapped_pad",
            "electronics.footprint.unmapped_pin",
            "electronics.footprint.unsupported",
            "electronics.footprint.assignment_invalid",
            "electronics.fabrication.export",
            "electronics.fabrication.export_help",
            "electronics.fabrication.saved",
            "electronics.fabrication.blocked",
        ];
        for language in [raf_core::Language::English, raf_core::Language::Spanish] {
            for key in keys {
                assert_ne!(raf_core::i18n::t(key, language), key, "{language:?}: {key}");
            }
        }
    }
}
