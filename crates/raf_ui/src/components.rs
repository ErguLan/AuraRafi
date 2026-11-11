//! Small semantic component recipes.
//!
//! These helpers keep repeated editor vocabulary in one place. They return
//! ordinary `UiNode`s, so authors can still add classes, controls, and event
//! bindings without introducing a second widget system.

mod kit;
mod media;
mod states;
pub use kit::{UiButtonVariant, UiComponents};
pub use media::{UiImageView, UiImageViewShape};

use crate::controls::{UiImageFit, UiSelect, UiTextInput};
use crate::events::{UiAction, UiEventBinding, UiEventKind};
use crate::geometry::UiSpacing;
use crate::icons::{UiIcon, UiIconId, UiIconSize};
use crate::layout::{UiAlign, UiFlow, UiJustify, UiLayout, UiSizeMode};
use crate::node::{UiAccessibilityRole, UiNode, UiNodeKind};
use crate::style::{StudioUiPalette, UiControlKind, UiStyle, UiSurfaceMaterial, UiThemeMetrics};
use crate::text::{
    UiFontWeight, UiTextOverflow, UiTextRole, UiTextRun, UiTextRunContent, UiTextStyle,
};
use crate::visual::{UiContentAlign, UiIconPlacement, UiVisual};

#[derive(Debug, Clone, PartialEq)]
pub struct UiSegmentedOption {
    pub id: String,
    pub label_key: String,
    pub icon: Option<UiIconId>,
    pub action: UiAction,
    pub selected: bool,
    pub show_label: bool,
}

impl UiSegmentedOption {
    pub fn new(id: impl Into<String>, label_key: impl Into<String>, action: UiAction) -> Self {
        Self {
            id: id.into(),
            label_key: label_key.into(),
            icon: None,
            action,
            selected: false,
            show_label: true,
        }
    }

    pub fn with_icon(mut self, icon: UiIconId) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_icon_only(mut self) -> Self {
        self.show_label = false;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSegmentedOrientation {
    Horizontal,
    Vertical,
}

/// Shared group recipe for mutually exclusive actions such as viewport tools.
pub fn segmented_group(
    id: impl Into<String>,
    options: impl IntoIterator<Item = UiSegmentedOption>,
    orientation: UiSegmentedOrientation,
    palette: StudioUiPalette,
    theme: UiThemeMetrics,
) -> UiNode {
    let id = id.into();
    let tokens = palette.tokens();
    let chrome = tokens.chrome();
    let metrics = theme.for_control(UiControlKind::Button);
    let options = options.into_iter().collect::<Vec<_>>();
    let option_count = options.len();
    let mut group = UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_class("segmented-group")
        .with_layout(UiLayout {
            flow: match orientation {
                UiSegmentedOrientation::Horizontal => UiFlow::Row,
                UiSegmentedOrientation::Vertical => UiFlow::Column,
            },
            align_items: UiAlign::Center,
            gap: 0.0,
            ..UiLayout::fit_content()
        });

    for (index, option) in options.into_iter().enumerate() {
        let first = index == 0;
        let last = index + 1 == option_count;
        let single = first && last;
        let radius = metrics.corner_radius.max(0.0);
        let corners = match orientation {
            UiSegmentedOrientation::Horizontal => [
                if first || single { radius } else { 0.0 },
                if last || single { radius } else { 0.0 },
                if last || single { radius } else { 0.0 },
                if first || single { radius } else { 0.0 },
            ],
            UiSegmentedOrientation::Vertical => [
                if first || single { radius } else { 0.0 },
                if first || single { radius } else { 0.0 },
                if last || single { radius } else { 0.0 },
                if last || single { radius } else { 0.0 },
            ],
        };
        let mut button = UiNode::new(option.id.clone(), UiNodeKind::Button)
            .with_class(if option.selected {
                "segmented-option segmented-option-selected"
            } else {
                "segmented-option"
            })
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                padding: UiSpacing::xy(metrics.horizontal_padding, 0.0),
                ..UiLayout::fit_content().fixed_height(metrics.height)
            })
            .with_visual(UiVisual {
                corners: Some(corners),
                content: crate::visual::UiContentLayout {
                    align: UiContentAlign::Center,
                    icon_placement: UiIconPlacement::Start,
                    gap: metrics.icon_gap,
                },
                ..UiVisual::default()
            })
            .with_style(UiStyle {
                fill: if option.selected {
                    tokens.selection
                } else {
                    tokens.surface_alt
                },
                border: if option.selected {
                    tokens.accent
                } else {
                    tokens.border
                },
                text: tokens.text,
                border_width: 1.0,
                radius: metrics.corner_radius,
                opacity: 1.0,
            })
            .with_tooltip_key(option.label_key.clone())
            .with_accessibility_label_key(option.label_key.clone())
            .with_accessibility_role(UiAccessibilityRole::Tab)
            .with_accessibility_selected(option.selected)
            .focusable()
            .with_event(UiEventBinding {
                event: UiEventKind::Click,
                action: option.action,
            });
        if let Some(icon) = option.icon {
            button = button.with_icon(UiIcon::new(icon).with_size(UiIconSize::Toolbar).with_tint(
                if option.selected {
                    chrome.icon_accent
                } else {
                    chrome.icon_muted
                },
            ));
        }
        if option.show_label {
            button = button
                .with_text_key(option.label_key)
                .with_text_style(UiTextStyle::button(tokens.text_muted));
        }
        group = group.with_child(button);
    }
    group
}

/// Thumbnail image recipe that keeps the crop and visible rounded clip on the
/// same retained node.
pub fn thumbnail_image(
    id: impl Into<String>,
    source_key: impl Into<String>,
    fit: crate::controls::UiImageFit,
    layout: UiLayout,
    corner_radius: f32,
) -> UiNode {
    let mut node = UiNode::new(id, UiNodeKind::Image).with_layout(layout);
    set_thumbnail_image(&mut node, source_key, fit, corner_radius);
    node.with_class("thumbnail-image")
}

/// Changes an existing retained node into an image without replacing its id,
/// classes, layout, handlers, accessibility, or interaction state.
pub fn set_thumbnail_image(
    node: &mut UiNode,
    source_key: impl Into<String>,
    fit: UiImageFit,
    corner_radius: f32,
) {
    UiImageView::new(source_key).fit(fit)
        .shape(UiImageViewShape::Rounded(corner_radius)).apply_to(node);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiStatusTone {
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

/// Honest state badge recipe; callers provide a localized key for a real
/// loading, ready, unavailable, or error state.
pub fn status_badge(
    id: impl Into<String>,
    label_key: impl Into<String>,
    tone: UiStatusTone,
    palette: StudioUiPalette,
    theme: UiThemeMetrics,
) -> UiNode {
    let tokens = palette.tokens();
    let metrics = theme.for_control(UiControlKind::Button);
    let color = match tone {
        UiStatusTone::Neutral => tokens.text_muted,
        UiStatusTone::Info => tokens.info,
        UiStatusTone::Success => tokens.positive,
        UiStatusTone::Warning => tokens.warning,
        UiStatusTone::Error => tokens.danger,
    };
    UiNode::new(id, UiNodeKind::Label)
        .with_class("status-badge")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(metrics.horizontal_padding, 0.0),
            ..UiLayout::fit_content().fixed_height(metrics.height)
        })
        .with_style(UiStyle {
            fill: [color[0], color[1], color[2], 42],
            border: color,
            text: color,
            border_width: 1.0,
            radius: metrics.corner_radius,
            opacity: 1.0,
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(color))
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_visual(UiVisual {
            corners: Some([metrics.corner_radius; 4]),
            ..UiVisual::default()
        })
}

/// Builds a real retained row from independently styled text fragments.
///
/// Each fragment remains an ordinary label, so translation and literal
/// runtime values keep their existing resolution paths. Runs stay on one row;
/// use multiple rows when the content must wrap between fragments.
pub fn inline_text_row(id: impl Into<String>, runs: impl IntoIterator<Item = UiTextRun>) -> UiNode {
    let id = id.into();
    let mut row = UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_class("inline-text-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Baseline,
            ..UiLayout::fit_content()
        });

    for (index, run) in runs.into_iter().enumerate() {
        let mut label = UiNode::new(format!("{id}.run-{index}"), UiNodeKind::Label)
            .with_layout(UiLayout::fit_content())
            .with_text_style(run.style)
            .with_text_overflow(UiTextOverflow::Clip);
        label = match run.content {
            UiTextRunContent::Key(key) => label.with_text_key(key),
            UiTextRunContent::Value(value) => label.with_text_value(value),
        };
        row = row.with_child(label);
    }

    row
}

pub fn icon_button(
    id: impl Into<String>,
    command: impl Into<String>,
    tooltip_key: impl Into<String>,
) -> UiNode {
    let tooltip_key = tooltip_key.into();
    UiNode::new(id, UiNodeKind::Button)
        .with_class("icon-button")
        .with_layout(UiLayout::fixed(32.0, 32.0))
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_tooltip_key(tooltip_key.clone())
        .with_accessibility_label_key(tooltip_key)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Icon-button recipe with a semantic icon request. The icon stays data-only;
/// ApiGraphicBasic owns its high-density rasterization and sampling policy.
pub fn icon_button_with_icon(
    id: impl Into<String>,
    icon: UiIconId,
    command: impl Into<String>,
    tooltip_key: impl Into<String>,
) -> UiNode {
    icon_button(id, command, tooltip_key).with_icon(UiIcon::new(icon))
}

pub fn panel_header(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("panel-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            ..UiLayout::fixed(0.0, 32.0)
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle {
            role: UiTextRole::PanelTitle,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Bold,
            color: [237, 239, 242, 255],
            inherit_color: true,
        })
}

pub fn tree_row(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("tree-row")
        .with_layout(UiLayout::fixed(0.0, 28.0))
        .with_text_key(text_key)
        .focusable()
}

/// Hierarchy row variant for documents that already know the entity kind.
pub fn tree_row_with_icon(
    id: impl Into<String>,
    text_key: impl Into<String>,
    icon: UiIconId,
) -> UiNode {
    tree_row(id, text_key).with_icon(UiIcon::new(icon).with_size(crate::icons::UiIconSize::Small))
}

/// Structural command row shared by viewport and editor shell surfaces.
pub fn technical_toolbar(id: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Toolbar)
        .with_class("technical-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 32.0)
        })
}

/// One option inside a segmented control. The parent owns the selected state.
pub fn segmented_option(
    id: impl Into<String>,
    text_key: impl Into<String>,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("segmented-option")
        .with_layout(UiLayout::fit_content())
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Lower-canvas action cluster. It is intentionally a structural overlay so
/// it cannot expand the viewport or change scene coordinates.
pub fn floating_action_rail(id: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("floating-action-rail")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 36.0).with_z_index(40)
        })
}

/// Label/value container used by inspector sections. The value control is
/// added by the owning surface so authoring tools can inspect it as an
/// ordinary child.
pub fn inspector_field(id: impl Into<String>, label_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("inspector-field")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_text_key(label_key)
}

/// Compact disclosure header used by retained Inspector sections.
///
/// The command remains owned by the surface host. Keeping expansion as a
/// semantic command avoids coupling the shared recipe to editor state.
pub fn disclosure_header(
    id: impl Into<String>,
    label_key: impl Into<String>,
    icon: UiIconId,
    expanded: bool,
    command: impl Into<String>,
    palette: StudioUiPalette,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let tokens = palette.tokens();
    UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class("disclosure-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if expanded {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(14.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key.clone())
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_tooltip_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Retained combobox trigger. Options are authored as ordinary menu nodes by
/// the owning surface, so dropdowns remain inspectable and backend-agnostic.
pub fn dropdown_trigger(
    id: impl Into<String>,
    selected_label_key: impl Into<String>,
    command: impl Into<String>,
    expanded: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let id = id.into();
    let selected_label_key = selected_label_key.into();
    let command = command.into();
    let tokens = palette.tokens();
    let mut trigger = UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class("dropdown-trigger")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 29.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_key(selected_label_key.clone())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_child(
            UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if expanded {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .with_tooltip_key(selected_label_key)
        .focusable()
        .with_text_overflow(UiTextOverflow::Clip)
        .with_accessibility_role(UiAccessibilityRole::Combobox)
        .with_accessibility_expanded(expanded)
        .with_event(UiEventBinding::command(UiEventKind::Click, command.clone()));
    trigger = trigger
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("ArrowDown".to_string()),
            format!("{command}:next"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("ArrowUp".to_string()),
            format!("{command}:previous"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("Home".to_string()),
            format!("{command}:first"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("End".to_string()),
            format!("{command}:last"),
        ));
    if expanded {
        trigger = trigger.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            command,
        ));
    }
    trigger
}

/// One full-width option inside a retained dropdown menu.
pub fn dropdown_option(
    id: impl Into<String>,
    label_key: impl Into<String>,
    command: impl Into<String>,
    selected: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if selected {
            "dropdown-option-selected"
        } else {
            "dropdown-option"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Start,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(if selected {
            tokens.text
        } else {
            tokens.text_muted
        }))
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Option)
        .with_accessibility_selected(selected)
        .with_text_overflow(UiTextOverflow::Clip)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Complete retained Select trigger. The option list can be rendered by the
/// owning surface in an overlay while the control keeps selection and
/// keyboard behavior in one serializable contract.
pub fn select_trigger(id: impl Into<String>, select: UiSelect, palette: StudioUiPalette) -> UiNode {
    let id = id.into();
    let is_open = select.open;
    let tokens = palette.tokens();
    let selected_label = select
        .selected_option()
        .map(|option| option.label_key.clone())
        .unwrap_or_else(|| "app.select_empty".to_string());
    UiNode::select(id.clone(), select)
        .with_class("select-trigger")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 29.0).with_width_mode(UiSizeMode::Fill)
        })
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Combobox)
        .with_accessibility_expanded(is_open)
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_key(selected_label)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_text_safe_area(true),
                ),
        )
        .with_child(
            UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if is_open {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
}

/// Standard label/control/helper composition for editor forms. It stays a
/// plain retained tree so callers can replace the control without introducing
/// a second widget abstraction.
pub fn form_field(
    id: impl Into<String>,
    label_key: impl Into<String>,
    control: UiNode,
    helper_key: Option<impl Into<String>>,
    error_key: Option<impl Into<String>>,
    palette: StudioUiPalette,
) -> UiNode {
    let id = id.into();
    let tokens = palette.tokens();
    let mut field = UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_class("form-field")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(control);
    if let Some(helper_key) = helper_key {
        field = field.with_child(
            UiNode::new(format!("{id}.helper"), UiNodeKind::Label)
                .with_text_key(helper_key)
                .with_class("form-field-helper")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    if let Some(error_key) = error_key {
        field = field.with_child(
            UiNode::new(format!("{id}.error"), UiNodeKind::Label)
                .with_text_key(error_key)
                .with_class("form-field-error")
                .with_text_style(UiTextStyle::body([238, 116, 105, 255]))
                .with_layout(UiLayout::fit_content()),
        );
    }
    field
}

pub fn search_field(
    id: impl Into<String>,
    value_key: impl Into<String>,
    placeholder_key: impl Into<String>,
    submit_command: Option<String>,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    let input = UiTextInput {
        value_key: value_key.into(),
        placeholder_key: Some(placeholder_key.into()),
        max_length: 512,
        multiline: false,
        password: false,
        submit_command,
    };
    UiNode::text_input(id, input)
        .with_class("search-field")
        .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
        .with_text_style(UiTextStyle::body(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Textbox)
}

pub fn numeric_field(
    id: impl Into<String>,
    value_key: impl Into<String>,
    submit_command: impl Into<String>,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::text_input(
        id,
        UiTextInput {
            value_key: value_key.into(),
            placeholder_key: Some("app.value".to_string()),
            max_length: 32,
            multiline: false,
            password: false,
            submit_command: Some(submit_command.into()),
        },
    )
    .with_class("numeric-field")
    .with_layout(UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill))
    .with_text_style(UiTextStyle::body(tokens.text))
    .with_accessibility_role(UiAccessibilityRole::Textbox)
}

pub fn password_field(
    id: impl Into<String>,
    value_key: impl Into<String>,
    submit_command: Option<String>,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::text_input(
        id,
        UiTextInput {
            value_key: value_key.into(),
            placeholder_key: None,
            max_length: 512,
            multiline: false,
            password: true,
            submit_command,
        },
    )
    .with_class("password-field")
    .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
    .with_text_style(UiTextStyle::body(tokens.text))
    .with_accessibility_role(UiAccessibilityRole::Textbox)
}

pub fn editor_tab(
    id: impl Into<String>,
    text_key: impl Into<String>,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("editor-tab")
        .with_layout(UiLayout::fit_content())
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
        .focusable()
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub fn empty_state(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("empty-state")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: crate::layout::UiJustify::Center,
            padding: crate::geometry::UiSpacing::xy(16.0, 16.0),
            ..UiLayout::fit_content()
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::body([151, 159, 170, 255]))
}

/// Builds the visual node used by every retained tooltip host. The host is
/// responsible for placing the node in a global overlay layer; this function
/// owns the compact design language and keeps it out of bridge code.
pub fn tooltip_node(
    id: impl Into<String>,
    text_key: impl Into<String>,
    palette: StudioUiPalette,
    opacity: f32,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Tooltip)
        .with_class("tooltip")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_text_key(text_key)
        .with_layout(
            UiLayout::fit_content()
                .with_width_mode(UiSizeMode::FitContent)
                .with_height_mode(UiSizeMode::FitContent),
        )
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: opacity.clamp(0.0, 1.0),
        })
        .with_text_style(UiTextStyle {
            role: UiTextRole::Tooltip,
            size_px: 10.5,
            line_height_px: 14.0,
            weight: UiFontWeight::Regular,
            color: tokens.text,
            inherit_color: false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_button_is_keyboard_accessible_and_has_tooltip_semantics() {
        let button = icon_button("grid", "viewport.grid", "ui.tooltip.grid");
        assert!(button.focusable);
        assert_eq!(button.tooltip_key.as_deref(), Some("ui.tooltip.grid"));
        assert!(button.accessibility_label_key.is_some());
    }

    #[test]
    fn semantic_icon_recipe_does_not_require_a_png_path() {
        let button =
            icon_button_with_icon("grid", UiIconId::Grid, "viewport.grid", "ui.tooltip.grid");
        assert_eq!(button.icon.map(|icon| icon.id), Some(UiIconId::Grid));
    }

    #[test]
    fn dropdown_trigger_exposes_selection_and_keyboard_escape() {
        let trigger = dropdown_trigger(
            "shape",
            "app.primitive_cube",
            "shape.toggle",
            false,
            StudioUiPalette::IndustrialDark,
        );

        assert!(trigger.focusable);
        assert!(!trigger.event_handlers.iter().any(|binding| {
            matches!(binding.event, UiEventKind::KeyPress(ref key) if key == "escape")
        }));
        let expanded = dropdown_trigger(
            "shape.open",
            "app.primitive_cube",
            "shape.toggle",
            true,
            StudioUiPalette::IndustrialDark,
        );
        assert!(expanded.event_handlers.iter().any(|binding| {
            matches!(binding.event, UiEventKind::KeyPress(ref key) if key == "escape")
        }));
        assert_eq!(
            trigger.children[0].text_key.as_deref(),
            Some("app.primitive_cube")
        );
    }

    #[test]
    fn tooltip_recipe_is_intrinsic_and_compact() {
        let tooltip = tooltip_node(
            "tooltip",
            "ui.tooltip.grid",
            StudioUiPalette::IndustrialDark,
            1.0,
        );
        assert_eq!(tooltip.kind, UiNodeKind::Tooltip);
        assert_eq!(tooltip.layout.width_mode, UiSizeMode::FitContent);
        assert_eq!(tooltip.layout.height_mode, UiSizeMode::FitContent);
        assert_eq!(tooltip.layout.basis, [0.0, 0.0]);
    }

    #[test]
    fn literal_tooltip_value_is_kept_out_of_translation_keys() {
        let node = UiNode::new("metadata", UiNodeKind::Label).with_tooltip_value("World_Rafi");

        assert_eq!(node.tooltip_value.as_deref(), Some("World_Rafi"));
        assert!(node.tooltip_key.is_none());
    }
}
