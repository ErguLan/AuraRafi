//! Retained modal for custom component creation.
//!
//! The surface is presentation only: it reads an immutable draft snapshot and
//! emits semantic commands. Persistence, validation and library refresh stay
//! in `electronics_controller` / `electronics_component_authoring`.
//!
//! The symbol preview rasterizes the same bounded ink the CAD canvas receives,
//! at a 2x target scale that only shrinks when a large pin count cannot fit.
//! It never allocates in a paint path: the preview tree is rebuilt with the
//! surrounding document, not per frame.

use std::collections::BTreeMap;

use glam::Vec2;
use raf_electronics::component::{
    MAX_COMPONENT_CATEGORY, MAX_COMPONENT_DESIGNATOR, MAX_COMPONENT_FOOTPRINT, MAX_COMPONENT_NAME,
    MAX_COMPONENT_VALUE,
};
use raf_electronics::symbol::{SymbolPreset, DEFAULT_SYMBOL_SCALE, SYMBOL_GRID_UNIT};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiScrollAxis, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRuleState, UiStyleSheet, UiSurface, UiSurfaceMaterial, UiTextInput,
    UiTextOverflow, UiTextStyle, UiTokens,
};
use raf_ui::{UiAlign, UiFontWeight, UiJustify, UiTextRole};

use crate::electronics_controller::{
    preset_slug, ComponentCreatorDraft, ComponentCreatorField, ComponentCreatorPhase,
    COMPONENT_CREATOR_CANCEL_COMMAND, COMPONENT_CREATOR_PRESET_PREFIX,
    COMPONENT_CREATOR_SAVE_COMMAND,
};
use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_disabled_rule, electronics_focus_rule, electronics_state_rule, with_alpha,
    ELECTRONICS_BORDER_WIDTH, ELECTRONICS_CORNER_RADIUS,
};

/// Presented width of the component creator modal, in logical points.
pub const ELECTRONICS_COMPONENT_CREATOR_WIDTH: f32 = 620.0;
/// Presented height of the component creator modal, in logical points.
pub const ELECTRONICS_COMPONENT_CREATOR_HEIGHT: f32 = 560.0;
/// Width of the live symbol preview column, in logical points.
pub const ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH: f32 = 220.0;
/// Height of the live symbol preview raster area, in logical points.
pub const ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT: f32 = 330.0;

const CREATOR_PADDING_X: f32 = 18.0;
const CREATOR_PADDING_Y: f32 = 16.0;
/// Minimum inset from the window edges when the modal is centered.
const CREATOR_WINDOW_MARGIN: f32 = 24.0;
const CREATOR_GAP: f32 = 12.0;
const CREATOR_HEADER_HEIGHT: f32 = 28.0;
const CREATOR_STATUS_HEIGHT: f32 = 22.0;
const CREATOR_FOOTER_HEIGHT: f32 = 30.0;
const CREATOR_FIELD_HEIGHT: f32 = 50.0;
const CREATOR_LABEL_HEIGHT: f32 = 16.0;
const CREATOR_CONTROL_HEIGHT: f32 = 30.0;
const CREATOR_FIELD_GAP: f32 = 4.0;
const CREATOR_FORM_GAP: f32 = 8.0;
const CREATOR_BUTTON_WIDTH: f32 = 96.0;
const CREATOR_SAVE_WIDTH: f32 = 112.0;
const CREATOR_PREVIEW_HINT_HEIGHT: f32 = 36.0;
const CREATOR_PREVIEW_GAP: f32 = 6.0;
const CREATOR_PREVIEW_SCALE: f32 = 2.0;
const CREATOR_PREVIEW_PADDING: f32 = 12.0;
const CREATOR_PREVIEW_CELL: f32 = 2.0;
const CREATOR_PREVIEW_SAMPLE_STEP: f32 = 1.5;
const CREATOR_PREVIEW_MAX_SEGMENT_SAMPLES: usize = 512;
const CREATOR_PREVIEW_MAX_SAMPLES: usize = 12_000;
const CREATOR_PREVIEW_MAX_RUNS: usize = 480;
const CREATOR_PREVIEW_PIN_MARK: f32 = 4.0;
const CREATOR_PRESET_COUNT: usize = 5;
/// Digits needed to type the largest accepted pin count.
const CREATOR_PIN_INPUT_DIGITS: usize = 3;

const CREATOR_MODAL_ID: &str = "electronics.component_creator";
const CREATOR_BACKDROP_ID: &str = "electronics.component_creator.backdrop";
const CREATOR_FORM_ID: &str = "electronics.component_creator.form";
const CREATOR_PREVIEW_ID: &str = "electronics.component_creator.preview";
const CREATOR_STATUS_ID: &str = "electronics.component_creator.status";
const CREATOR_SAVE_ID: &str = "electronics.component_creator.save";
const CREATOR_CANCEL_ID: &str = "electronics.component_creator.cancel";

/// Host-owned motion samples for the creator modal.
///
/// `opacity` is 0.0 while closed and 1.0 when fully open. A reduced-motion host
/// resolves it to 1.0, and the surface never animates on its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectronicsComponentCreatorMotion {
    pub opacity: f32,
    pub validation_reveal: f32,
}

impl Default for ElectronicsComponentCreatorMotion {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            validation_reveal: 1.0,
        }
    }
}

impl ElectronicsComponentCreatorMotion {
    pub fn new(opacity: f32, validation_reveal: f32) -> Self {
        Self {
            opacity: opacity.clamp(0.0, 1.0),
            validation_reveal: validation_reveal.clamp(0.0, 1.0),
        }
    }

    fn resolved_opacity(self) -> f32 {
        if self.opacity.is_finite() {
            self.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    fn resolved_validation(self) -> f32 {
        if self.validation_reveal.is_finite() {
            self.validation_reveal.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

/// Centers and clamps the modal rect inside the available window area.
pub fn electronics_component_creator_rect(area: UiRect) -> UiRect {
    let width =
        ELECTRONICS_COMPONENT_CREATOR_WIDTH.min((area.width - CREATOR_WINDOW_MARGIN).max(1.0));
    let height =
        ELECTRONICS_COMPONENT_CREATOR_HEIGHT.min((area.height - CREATOR_WINDOW_MARGIN).max(1.0));
    UiRect::new(
        area.x + (area.width - width) * 0.5,
        area.y + (area.height - height) * 0.5,
        width,
        height,
    )
}

/// Explicit keyboard traversal order for the modal.
///
/// The host must install this on the modal's focus policy so focus cannot leave
/// the dialog while it is open.
pub fn electronics_component_creator_focus_order() -> Vec<String> {
    let mut order = vec![
        field_focus_id(ComponentCreatorField::Name),
        field_focus_id(ComponentCreatorField::Category),
        field_focus_id(ComponentCreatorField::Designator),
        field_focus_id(ComponentCreatorField::Value),
        field_focus_id(ComponentCreatorField::Pins),
    ];
    order.extend(
        preset_slugs()
            .iter()
            .map(|slug| format!("electronics.component_creator.preset.{slug}")),
    );
    order.push(field_focus_id(ComponentCreatorField::Footprint));
    order.push(CREATOR_CANCEL_ID.to_string());
    order.push(CREATOR_SAVE_ID.to_string());
    order
}

/// Dimmed scrim presented below the creator modal.
pub fn build_electronics_component_creator_backdrop_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new(CREATOR_BACKDROP_ID, UiNodeKind::Overlay)
        .with_class("electronics-component-creator-backdrop")
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
        .with_accessibility_label_key("electronics.component_creator.title")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            COMPONENT_CREATOR_CANCEL_COMMAND,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            COMPONENT_CREATOR_CANCEL_COMMAND,
        ));
    UiSurface::new(CREATOR_BACKDROP_ID, palette, root)
}

/// Builds the creator modal for one immutable draft snapshot.
pub fn build_electronics_component_creator_surface(
    palette: StudioUiPalette,
    rect: UiRect,
    draft: &ComponentCreatorDraft,
    phase: ComponentCreatorPhase,
    error_key: Option<&str>,
    saved: bool,
    motion: ElectronicsComponentCreatorMotion,
) -> UiSurface {
    let tokens = palette.tokens();
    let opacity = motion.resolved_opacity();
    let can_save = matches!(
        phase,
        ComponentCreatorPhase::Editing | ComponentCreatorPhase::Error
    ) && !saved;
    let mut root = UiNode::new(CREATOR_MODAL_ID, UiNodeKind::FloatingPanel)
        .with_class("electronics-component-creator")
        .with_material(UiSurfaceMaterial::ModalSurface)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CREATOR_GAP,
            padding: UiSpacing::xy(CREATOR_PADDING_X, CREATOR_PADDING_Y),
            ..UiLayout::absolute(rect).with_z_index(61)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: ELECTRONICS_BORDER_WIDTH,
            radius: ELECTRONICS_CORNER_RADIUS + 2.0,
            opacity,
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("electronics.component_creator.title")
        .with_accessibility_description_key("electronics.component_creator.hint")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            COMPONENT_CREATOR_CANCEL_COMMAND,
        ));
    root = root
        .with_child(header(tokens))
        .with_child(body(tokens, draft, error_key))
        .with_child(status(tokens, error_key, saved, motion))
        .with_child(footer(tokens, can_save, phase, saved));
    let mut surface = UiSurface::new(CREATOR_MODAL_ID, palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

/// Builds the scrim and modal together for a window-sized area.
///
/// The host pushes both surfaces in this order when
/// `ComponentCreatorState::is_open()`; the modal is placed in window
/// coordinates and never inside a panel clip region.
pub fn build_electronics_component_creator_layers(
    palette: StudioUiPalette,
    area: UiRect,
    draft: &ComponentCreatorDraft,
    phase: ComponentCreatorPhase,
    error_key: Option<&str>,
    saved: bool,
    motion: ElectronicsComponentCreatorMotion,
) -> (UiSurface, UiSurface) {
    (
        build_electronics_component_creator_backdrop_surface(palette),
        build_electronics_component_creator_surface(
            palette,
            electronics_component_creator_rect(area),
            draft,
            phase,
            error_key,
            saved,
            motion,
        ),
    )
}

/// Returns the creator layers only while the live editor reports an open
/// creator. This is the host-facing entry point: present the two surfaces in
/// order above the workbench and install
/// [`electronics_component_creator_focus_order`] on the modal focus policy.
///
/// Rebuild the surfaces when `NativeElectronicsEditor::ui_revision` changes,
/// not per frame; the symbol preview raster is bounded but is document work.
pub fn build_electronics_component_creator_from_editor(
    palette: StudioUiPalette,
    area: UiRect,
    editor: &crate::electronics_controller::NativeElectronicsEditor,
    motion: ElectronicsComponentCreatorMotion,
) -> Option<(UiSurface, UiSurface)> {
    let state = editor.component_creator();
    state.is_open().then(|| {
        build_electronics_component_creator_layers(
            palette,
            area,
            &state.draft,
            state.phase,
            state.error_key,
            state.saved,
            motion,
        )
    })
}

fn header(tokens: UiTokens) -> UiNode {
    UiNode::new("electronics.component_creator.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, CREATOR_HEADER_HEIGHT).fill_width()
        })
        .with_child(
            UiNode::new(
                "electronics.component_creator.header.icon",
                UiNodeKind::Label,
            )
            .with_icon(
                UiIcon::new(UiIconId::Add)
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.accent),
            )
            .with_layout(UiLayout::fixed(16.0, 20.0)),
        )
        .with_child(
            UiNode::new(
                "electronics.component_creator.header.title",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.component_creator.title")
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::PanelTitle,
                size_px: 13.0,
                line_height_px: 18.0,
                weight: UiFontWeight::Bold,
                color: tokens.text,
                inherit_color: false,
            })
            .with_layout(UiLayout::fit_content().fill_width().grow(1.0)),
        )
}

fn body(tokens: UiTokens, draft: &ComponentCreatorDraft, error_key: Option<&str>) -> UiNode {
    let invalid_field = invalid_field_for_error(error_key);
    let mut form =
        UiNode::scroll_view(CREATOR_FORM_ID, UiScrollAxis::Vertical).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CREATOR_FORM_GAP,
            overflow: UiOverflow::ScrollY,
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    for field in [
        ComponentCreatorField::Name,
        ComponentCreatorField::Category,
        ComponentCreatorField::Designator,
        ComponentCreatorField::Value,
        ComponentCreatorField::Pins,
    ] {
        form = form.with_child(text_field(
            tokens,
            field,
            invalid_field == Some(field),
            field == ComponentCreatorField::Pins && draft.preset == SymbolPreset::Ground,
            (field == ComponentCreatorField::Pins && draft.preset == SymbolPreset::Ground)
                .then(|| draft.effective_pin_count().to_string()),
        ));
    }
    form = form.with_child(preset_field(tokens, draft));
    form = form.with_child(text_field(
        tokens,
        ComponentCreatorField::Footprint,
        invalid_field == Some(ComponentCreatorField::Footprint),
        false,
        None,
    ));

    UiNode::new("electronics.component_creator.body", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Start,
            gap: 16.0,
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_child(form)
        .with_child(preview_column(tokens, draft))
}

fn invalid_field_for_error(error_key: Option<&str>) -> Option<ComponentCreatorField> {
    match error_key? {
        "electronics.component_creator.error.name_required"
        | "electronics.component_creator.error.exists" => Some(ComponentCreatorField::Name),
        "electronics.component_creator.error.designator" => Some(ComponentCreatorField::Designator),
        "electronics.component_creator.error.value" => Some(ComponentCreatorField::Value),
        "electronics.component_creator.error.pins" => Some(ComponentCreatorField::Pins),
        _ => None,
    }
}

fn text_field(
    tokens: UiTokens,
    field: ComponentCreatorField,
    invalid: bool,
    disabled: bool,
    fixed_text: Option<String>,
) -> UiNode {
    let id = field_id(field);
    let label_key = field_label_key(field);
    let max_length = match field {
        ComponentCreatorField::Name => MAX_COMPONENT_NAME,
        ComponentCreatorField::Category => MAX_COMPONENT_CATEGORY,
        ComponentCreatorField::Designator => MAX_COMPONENT_DESIGNATOR,
        ComponentCreatorField::Value => MAX_COMPONENT_VALUE,
        ComponentCreatorField::Pins => CREATOR_PIN_INPUT_DIGITS,
        ComponentCreatorField::Footprint => MAX_COMPONENT_FOOTPRINT,
    };
    let input = UiNode::text_input(
        format!("{id}.control"),
        UiTextInput {
            value_key: field.value_key().to_string(),
            placeholder_key: Some(label_key.to_string()),
            max_length,
            multiline: false,
            password: false,
            submit_command: Some(COMPONENT_CREATOR_SAVE_COMMAND.to_string()),
        },
    )
    .with_class("electronics-component-creator-input")
    .with_layout(
        UiLayout::fixed(0.0, CREATOR_CONTROL_HEIGHT)
            .fill_width()
            .with_text_safe_area(true),
    )
    .with_text_style(electronics_body_style(tokens.text))
    .with_text_overflow(UiTextOverflow::Clip)
    .with_accessibility_label_key(label_key)
    .with_accessibility_role(UiAccessibilityRole::Textbox)
    .invalid(invalid)
    .disabled(disabled);
    let input = if let Some(value) = fixed_text {
        input.with_text_value(value)
    } else {
        input
    };
    field_row(id, label_key, input, tokens)
}

fn preset_field(tokens: UiTokens, draft: &ComponentCreatorDraft) -> UiNode {
    let id = "electronics.component_creator.field.symbol";
    let label = label_node(
        "electronics.component_creator.presets.label",
        "electronics.component_creator.symbol",
        tokens,
    );
    let mut row = UiNode::new("electronics.component_creator.presets", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Stretch,
            gap: 4.0,
            ..UiLayout::fixed(0.0, CREATOR_CONTROL_HEIGHT).fill_width()
        });
    for slug in preset_slugs() {
        let active = preset_slug(draft.preset) == slug;
        row = row.with_child(preset_button(tokens, slug, active));
    }
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CREATOR_FIELD_GAP,
            ..UiLayout::fixed(0.0, CREATOR_FIELD_HEIGHT).fill_width()
        })
        .with_child(label)
        .with_child(row)
}

fn preset_button(tokens: UiTokens, slug: &'static str, active: bool) -> UiNode {
    let label_key = preset_label_key(slug);
    let command = format!("{}{slug}", COMPONENT_CREATOR_PRESET_PREFIX);
    let mut node = UiNode::new(
        format!("electronics.component_creator.preset.{slug}"),
        UiNodeKind::Button,
    )
    .with_class("electronics-component-creator-preset")
    .with_layout(
        UiLayout {
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fixed(0.0, CREATOR_CONTROL_HEIGHT)
        }
        .fill_width()
        .grow(1.0),
    )
    .with_text_key(label_key)
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(UiTextStyle::button(if active {
        tokens.text
    } else {
        tokens.text_muted
    }))
    .with_tooltip_key(label_key)
    .with_accessibility_label_key(label_key)
    .with_accessibility_role(UiAccessibilityRole::Button)
    .with_accessibility_selected(active)
    .focusable()
    .with_event(UiEventBinding::command(UiEventKind::Click, command.clone()))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("enter".to_string()),
        command,
    ));
    if active {
        node = node.with_class("electronics-component-creator-preset-active");
    }
    node
}

fn field_row(id: String, label_key: &'static str, control: UiNode, tokens: UiTokens) -> UiNode {
    UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CREATOR_FIELD_GAP,
            ..UiLayout::fixed(0.0, CREATOR_FIELD_HEIGHT).fill_width()
        })
        .with_child(label_node(&format!("{id}.label"), label_key, tokens))
        .with_child(control)
}

fn label_node(id: &str, label_key: &'static str, tokens: UiTokens) -> UiNode {
    UiNode::new(id.to_string(), UiNodeKind::Label)
        .with_text_key(label_key)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle {
            role: UiTextRole::Label,
            size_px: 11.0,
            line_height_px: CREATOR_LABEL_HEIGHT,
            weight: UiFontWeight::Medium,
            color: tokens.text_muted,
            inherit_color: false,
        })
        .with_layout(UiLayout::fixed(0.0, CREATOR_LABEL_HEIGHT).fill_width())
}

fn preview_column(tokens: UiTokens, draft: &ComponentCreatorDraft) -> UiNode {
    let preview = UiNode::new(CREATOR_PREVIEW_ID, UiNodeKind::Panel)
        .with_class("electronics-component-creator-preview")
        .with_layout(UiLayout {
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(
                ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH,
                ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT,
            )
        })
        .with_style(UiStyle {
            fill: tokens.canvas,
            border: tokens.border,
            text: tokens.text,
            border_width: ELECTRONICS_BORDER_WIDTH,
            radius: ELECTRONICS_CORNER_RADIUS,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Image)
        .with_accessibility_label_key("electronics.component_creator.preview");
    let mut preview = preview;
    for node in preview_ink(tokens, draft) {
        preview = preview.with_child(node);
    }
    UiNode::new(
        "electronics.component_creator.preview-column",
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: CREATOR_PREVIEW_GAP,
        ..UiLayout::fixed(
            ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH,
            CREATOR_LABEL_HEIGHT
                + CREATOR_PREVIEW_GAP * 2.0
                + ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT
                + CREATOR_PREVIEW_HINT_HEIGHT,
        )
    })
    .with_child(label_node(
        "electronics.component_creator.preview-column.label",
        "electronics.component_creator.preview",
        tokens,
    ))
    .with_child(preview)
    .with_child(
        UiNode::new(
            "electronics.component_creator.preview-hint",
            UiNodeKind::Label,
        )
        .with_text_key("electronics.component_creator.preview_hint")
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_text_style(electronics_body_style(tokens.text_muted))
        .with_layout(
            UiLayout::fixed(
                ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH,
                CREATOR_PREVIEW_HINT_HEIGHT,
            )
            .with_text_safe_area(false),
        ),
    )
}

fn status(
    tokens: UiTokens,
    error_key: Option<&str>,
    saved: bool,
    motion: ElectronicsComponentCreatorMotion,
) -> UiNode {
    let reveal = motion.resolved_validation();
    let mut row = UiNode::new(CREATOR_STATUS_ID, UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, CREATOR_STATUS_HEIGHT).fill_width()
        })
        .with_accessibility_role(UiAccessibilityRole::Status);
    if let Some(key) = error_key {
        row = row.with_child(
            UiNode::new(
                "electronics.component_creator.status.icon",
                UiNodeKind::Label,
            )
            .with_icon(
                UiIcon::new(UiIconId::Error)
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.danger),
            )
            .with_layout(UiLayout::fixed(16.0, 18.0)),
        );
        row = row.with_child(
            UiNode::new(
                "electronics.component_creator.status.text",
                UiNodeKind::Label,
            )
            .with_text_key(key)
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: 12.0,
                line_height_px: 16.0,
                weight: UiFontWeight::Medium,
                color: tokens.danger,
                inherit_color: false,
            })
            .with_style(UiStyle::transparent().with_opacity(reveal))
            .invalid(true)
            .with_accessibility_label_key(key)
            .with_layout(UiLayout::fit_content().fill_width().grow(1.0)),
        );
    } else if saved {
        row = row.with_child(
            UiNode::new(
                "electronics.component_creator.status.icon",
                UiNodeKind::Label,
            )
            .with_icon(
                UiIcon::new(UiIconId::Success)
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.positive),
            )
            .with_layout(UiLayout::fixed(16.0, 18.0)),
        );
        row = row.with_child(
            UiNode::new(
                "electronics.component_creator.status.text",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.component_creator.saved")
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: 12.0,
                line_height_px: 16.0,
                weight: UiFontWeight::Medium,
                color: tokens.positive,
                inherit_color: false,
            })
            .with_accessibility_label_key("electronics.component_creator.saved")
            .with_layout(UiLayout::fit_content().fill_width().grow(1.0)),
        );
    } else {
        row = row.with_child(
            UiNode::new(
                "electronics.component_creator.status.text",
                UiNodeKind::Label,
            )
            .with_text_key("electronics.component_creator.hint")
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text_muted))
            .with_layout(UiLayout::fit_content().fill_width().grow(1.0)),
        );
    }
    row
}

fn footer(tokens: UiTokens, can_save: bool, phase: ComponentCreatorPhase, saved: bool) -> UiNode {
    let disabled = !can_save || phase == ComponentCreatorPhase::Saving;
    let cancel_key = if saved {
        "electronics.component_creator.close"
    } else {
        "electronics.component_creator.cancel"
    };
    UiNode::new("electronics.component_creator.footer", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::End,
            gap: 8.0,
            ..UiLayout::fixed(0.0, CREATOR_FOOTER_HEIGHT).fill_width()
        })
        .with_child(action_button(
            CREATOR_CANCEL_ID,
            cancel_key,
            COMPONENT_CREATOR_CANCEL_COMMAND,
            "electronics-component-creator-secondary",
            CREATOR_BUTTON_WIDTH,
            false,
            tokens,
        ))
        .with_child(action_button(
            CREATOR_SAVE_ID,
            "electronics.component_creator.save",
            COMPONENT_CREATOR_SAVE_COMMAND,
            "electronics-component-creator-primary",
            CREATOR_SAVE_WIDTH,
            disabled,
            tokens,
        ))
        .with_child(
            // Keeps the footer's intrinsic height stable in tests and when the
            // save button is disabled mid-write.
            UiNode::new(
                "electronics.component_creator.footer.anchor",
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::fixed(1.0, 1.0))
            .with_style(UiStyle::transparent()),
        )
}

fn action_button(
    id: &'static str,
    label_key: &'static str,
    command: &'static str,
    class: &'static str,
    width: f32,
    disabled: bool,
    tokens: UiTokens,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(width, CREATOR_FOOTER_HEIGHT).with_text_safe_area(true))
        .with_text_key(label_key)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(label_key)
        .with_tooltip_key(label_key)
        .focusable()
        .disabled(disabled)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command,
        ))
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let mut rules = vec![
        electronics_class_rule(
            "electronics-component-creator",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-component-creator-input",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-component-creator-preview",
            tokens.canvas,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-component-creator-preset",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        electronics_class_rule(
            "electronics-component-creator-preset-active",
            tokens.selection,
            tokens.accent,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-component-creator-secondary",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-component-creator-primary",
            with_alpha(tokens.accent, 56),
            tokens.accent,
            tokens.text,
        ),
    ];
    for class in [
        "electronics-component-creator-input",
        "electronics-component-creator-secondary",
        "electronics-component-creator-preset",
    ] {
        rules.push(electronics_state_rule(
            UiStyleRuleState::Hovered,
            class,
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        ));
        rules.push(electronics_focus_rule(class, tokens));
        rules.push(electronics_active_rule(class, tokens));
    }
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-component-creator-primary",
        UiStylePatch {
            fill: Some(with_alpha(tokens.accent, 84)),
            border: Some(tokens.accent_hot),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule(
        "electronics-component-creator-primary",
        tokens,
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Active,
        "electronics-component-creator-primary",
        UiStylePatch {
            fill: Some(with_alpha(tokens.accent, 112)),
            border: Some(tokens.accent_hot),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_disabled_rule(
        "electronics-component-creator-primary",
        tokens,
    ));
    rules.push(electronics_disabled_rule(
        "electronics-component-creator-secondary",
        tokens,
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-component-creator-preset-active",
        UiStylePatch {
            fill: Some(with_alpha(tokens.accent, 48)),
            border: Some(tokens.accent),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule(
        "electronics-component-creator-preset-active",
        tokens,
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Invalid,
        "electronics-component-creator-input",
        UiStylePatch {
            border: Some(tokens.danger),
            ..UiStylePatch::default()
        },
    ));
    UiStyleSheet { rules }
}

fn field_id(field: ComponentCreatorField) -> String {
    match field {
        ComponentCreatorField::Name => "electronics.component_creator.field.name".to_string(),
        ComponentCreatorField::Category => {
            "electronics.component_creator.field.category".to_string()
        }
        ComponentCreatorField::Designator => {
            "electronics.component_creator.field.designator".to_string()
        }
        ComponentCreatorField::Value => "electronics.component_creator.field.value".to_string(),
        ComponentCreatorField::Pins => "electronics.component_creator.field.pins".to_string(),
        ComponentCreatorField::Footprint => {
            "electronics.component_creator.field.footprint".to_string()
        }
    }
}

fn field_focus_id(field: ComponentCreatorField) -> String {
    format!("{}.control", field_id(field))
}

fn field_label_key(field: ComponentCreatorField) -> &'static str {
    match field {
        ComponentCreatorField::Name => "electronics.component_creator.name",
        ComponentCreatorField::Category => "electronics.component_creator.category",
        ComponentCreatorField::Designator => "electronics.component_creator.designator",
        ComponentCreatorField::Value => "electronics.component_creator.value",
        ComponentCreatorField::Pins => "electronics.component_creator.pins",
        ComponentCreatorField::Footprint => "electronics.component_creator.footprint",
    }
}

fn preset_slugs() -> [&'static str; CREATOR_PRESET_COUNT] {
    ["passive", "box", "ic", "connector", "ground"]
}

fn preset_label_key(slug: &str) -> &'static str {
    match slug {
        "box" => "electronics.component_creator.preset.box",
        "ic" => "electronics.component_creator.preset.ic",
        "connector" => "electronics.component_creator.preset.connector",
        "ground" => "electronics.component_creator.preset.ground",
        _ => "electronics.component_creator.preset.passive",
    }
}

/// Rasterizes the bounded symbol ink into retained separator nodes.
fn preview_ink(tokens: UiTokens, draft: &ComponentCreatorDraft) -> Vec<UiNode> {
    let symbol = draft.symbol();
    let paths = symbol.to_paths();
    let pins = draft
        .pin_offsets()
        .into_iter()
        .map(|offset| offset * SYMBOL_GRID_UNIT * DEFAULT_SYMBOL_SCALE)
        .collect::<Vec<_>>();
    let Some((min, max)) = ink_bounds(&paths, &pins) else {
        return Vec::new();
    };
    let extent = (max - min).max(Vec2::splat(1.0));
    let available = Vec2::new(
        (ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH - CREATOR_PREVIEW_PADDING * 2.0).max(1.0),
        (ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT - CREATOR_PREVIEW_PADDING * 2.0).max(1.0),
    );
    let fit = (available.x / extent.x).min(available.y / extent.y);
    let scale = CREATOR_PREVIEW_SCALE.min(fit).max(0.01);
    let center = (min + max) * 0.5;
    let preview_center = Vec2::new(
        ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH * 0.5,
        ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT * 0.5,
    );
    let project = |point: Vec2| preview_center + (point - center) * scale;

    let columns =
        (ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH / CREATOR_PREVIEW_CELL).ceil() as i32 + 1;
    let rows =
        (ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT / CREATOR_PREVIEW_CELL).ceil() as i32 + 1;
    let mut cells: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    let mut samples = 0usize;
    'paths: for path in &paths {
        for segment in path.windows(2) {
            let from = project(segment[0]);
            let to = project(segment[1]);
            let length = from.distance(to);
            let steps = ((length / CREATOR_PREVIEW_SAMPLE_STEP).ceil() as usize)
                .clamp(1, CREATOR_PREVIEW_MAX_SEGMENT_SAMPLES);
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let point = from.lerp(to, t);
                if mark_cell(&mut cells, point, columns, rows) {
                    samples += 1;
                    if samples >= CREATOR_PREVIEW_MAX_SAMPLES {
                        break 'paths;
                    }
                }
            }
        }
    }
    let mut nodes = Vec::new();
    'rows: for (row, xs) in cells {
        let mut xs = xs;
        xs.sort_unstable();
        xs.dedup();
        let mut start = xs[0];
        let mut previous = start;
        for x in xs.into_iter().skip(1) {
            if x == previous + 1 {
                previous = x;
                continue;
            }
            nodes.push(ink_run_node(nodes.len(), start, row, previous, tokens.text));
            if nodes.len() >= CREATOR_PREVIEW_MAX_RUNS {
                break 'rows;
            }
            start = x;
            previous = x;
        }
        nodes.push(ink_run_node(nodes.len(), start, row, previous, tokens.text));
        if nodes.len() >= CREATOR_PREVIEW_MAX_RUNS {
            break;
        }
    }
    for (index, pin) in pins.into_iter().enumerate() {
        let point = project(pin);
        if !point.is_finite()
            || point.x < 0.0
            || point.y < 0.0
            || point.x > ELECTRONICS_COMPONENT_CREATOR_PREVIEW_WIDTH
            || point.y > ELECTRONICS_COMPONENT_CREATOR_PREVIEW_HEIGHT
        {
            continue;
        }
        nodes.push(
            UiNode::new(
                format!("electronics.component_creator.preview.pin.{index}"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::absolute(UiRect::new(
                point.x - CREATOR_PREVIEW_PIN_MARK * 0.5,
                point.y - CREATOR_PREVIEW_PIN_MARK * 0.5,
                CREATOR_PREVIEW_PIN_MARK,
                CREATOR_PREVIEW_PIN_MARK,
            )))
            .with_style(UiStyle {
                fill: tokens.accent,
                border: [0, 0, 0, 0],
                text: tokens.text,
                border_width: 0.0,
                radius: 1.0,
                opacity: 1.0,
            }),
        );
    }
    nodes
}

fn ink_bounds(paths: &[Vec<Vec2>], pins: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut found = false;
    for point in paths.iter().flatten().chain(pins.iter()) {
        if !point.is_finite() {
            continue;
        }
        min = min.min(*point);
        max = max.max(*point);
        found = true;
    }
    found.then_some((min, max))
}

/// Marks one preview cell. Returns true when a new cell was added.
fn mark_cell(cells: &mut BTreeMap<i32, Vec<i32>>, point: Vec2, columns: i32, rows: i32) -> bool {
    if !point.is_finite() {
        return false;
    }
    let x = (point.x / CREATOR_PREVIEW_CELL).floor() as i32;
    let y = (point.y / CREATOR_PREVIEW_CELL).floor() as i32;
    if x < 0 || y < 0 || x >= columns || y >= rows {
        return false;
    }
    let row = cells.entry(y).or_default();
    if row.contains(&x) {
        return false;
    }
    row.push(x);
    true
}

fn ink_run_node(index: usize, min_x: i32, row: i32, max_x: i32, color: [u8; 4]) -> UiNode {
    let width = ((max_x - min_x + 1) as f32 * CREATOR_PREVIEW_CELL).max(CREATOR_PREVIEW_CELL);
    UiNode::new(
        format!("electronics.component_creator.preview.ink.{index}"),
        UiNodeKind::Separator,
    )
    .with_layout(UiLayout::absolute(UiRect::new(
        min_x as f32 * CREATOR_PREVIEW_CELL,
        row as f32 * CREATOR_PREVIEW_CELL,
        width,
        CREATOR_PREVIEW_CELL,
    )))
    .with_style(UiStyle {
        fill: [color[0], color[1], color[2], 236],
        border: [0, 0, 0, 0],
        text: color,
        border_width: 0.0,
        radius: 0.5,
        opacity: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electronics_controller::{
        COMPONENT_CREATOR_CATEGORY_VALUE_KEY, COMPONENT_CREATOR_DESIGNATOR_VALUE_KEY,
        COMPONENT_CREATOR_FOOTPRINT_VALUE_KEY, COMPONENT_CREATOR_NAME_VALUE_KEY,
        COMPONENT_CREATOR_PINS_VALUE_KEY, COMPONENT_CREATOR_VALUE_VALUE_KEY,
    };
    use crate::panels::electronics_surface::assert_electronics_layout_gate;

    fn draft() -> ComponentCreatorDraft {
        ComponentCreatorDraft {
            name: "Op Amp".to_string(),
            category: "Analog".to_string(),
            designator_prefix: "U".to_string(),
            value: "LM358".to_string(),
            pin_count: 8,
            preset: SymbolPreset::Ic,
            footprint: "DIP-8".to_string(),
        }
    }

    #[test]
    fn modal_builds_a_labeled_focusable_document() {
        let surface = build_electronics_component_creator_surface(
            StudioUiPalette::IndustrialDark,
            UiRect::new(
                0.0,
                0.0,
                ELECTRONICS_COMPONENT_CREATOR_WIDTH,
                ELECTRONICS_COMPONENT_CREATOR_HEIGHT,
            ),
            &draft(),
            ComponentCreatorPhase::Editing,
            None,
            false,
            ElectronicsComponentCreatorMotion::default(),
        );
        assert_electronics_layout_gate(
            &surface,
            ELECTRONICS_COMPONENT_CREATOR_WIDTH as u32,
            ELECTRONICS_COMPONENT_CREATOR_HEIGHT as u32,
        );
        for (id, value_key) in [
            (
                "electronics.component_creator.field.name",
                COMPONENT_CREATOR_NAME_VALUE_KEY,
            ),
            (
                "electronics.component_creator.field.category",
                COMPONENT_CREATOR_CATEGORY_VALUE_KEY,
            ),
            (
                "electronics.component_creator.field.designator",
                COMPONENT_CREATOR_DESIGNATOR_VALUE_KEY,
            ),
            (
                "electronics.component_creator.field.value",
                COMPONENT_CREATOR_VALUE_VALUE_KEY,
            ),
            (
                "electronics.component_creator.field.pins",
                COMPONENT_CREATOR_PINS_VALUE_KEY,
            ),
            (
                "electronics.component_creator.field.footprint",
                COMPONENT_CREATOR_FOOTPRINT_VALUE_KEY,
            ),
        ] {
            let input = surface
                .root
                .find(&format!("{id}.control"))
                .unwrap_or_else(|| panic!("missing field {id}"));
            assert_eq!(
                input
                    .control
                    .text_input()
                    .map(|input| input.value_key.as_str()),
                Some(value_key)
            );
            assert!(input.accessibility_label_key.is_some());
        }
        assert!(surface
            .root
            .find(CREATOR_PREVIEW_ID)
            .is_some_and(|preview| !preview.children.is_empty()));
    }

    #[test]
    fn presets_emit_commands_and_mark_the_active_choice() {
        let surface = build_electronics_component_creator_surface(
            StudioUiPalette::IndustrialDark,
            UiRect::new(
                0.0,
                0.0,
                ELECTRONICS_COMPONENT_CREATOR_WIDTH,
                ELECTRONICS_COMPONENT_CREATOR_HEIGHT,
            ),
            &draft(),
            ComponentCreatorPhase::Editing,
            None,
            false,
            ElectronicsComponentCreatorMotion::default(),
        );
        let active = surface
            .root
            .find("electronics.component_creator.preset.ic")
            .expect("active preset");
        assert!(active
            .classes
            .iter()
            .any(|class| class.as_str() == "electronics-component-creator-preset-active"));
        assert!(active.event_handlers.iter().any(|event| {
            matches!(&event.action, raf_ui::UiAction::Command { name }
                if name == "electronics.component.create.preset:ic")
        }));
        assert_eq!(active.accessibility_selected, Some(true));
    }

    #[test]
    fn validation_and_saved_states_are_presented_as_text() {
        let error = build_electronics_component_creator_surface(
            StudioUiPalette::IndustrialDark,
            UiRect::new(
                0.0,
                0.0,
                ELECTRONICS_COMPONENT_CREATOR_WIDTH,
                ELECTRONICS_COMPONENT_CREATOR_HEIGHT,
            ),
            &draft(),
            ComponentCreatorPhase::Error,
            Some("electronics.component_creator.error.exists"),
            false,
            ElectronicsComponentCreatorMotion::new(1.0, 0.0),
        );
        let error_label = error
            .root
            .find("electronics.component_creator.status.text")
            .expect("error text");
        assert_eq!(
            error_label.text_key.as_deref(),
            Some("electronics.component_creator.error.exists")
        );
        assert!(error_label.invalid);

        let saved = build_electronics_component_creator_surface(
            StudioUiPalette::IndustrialDark,
            UiRect::new(
                0.0,
                0.0,
                ELECTRONICS_COMPONENT_CREATOR_WIDTH,
                ELECTRONICS_COMPONENT_CREATOR_HEIGHT,
            ),
            &draft(),
            ComponentCreatorPhase::Editing,
            None,
            true,
            ElectronicsComponentCreatorMotion::default(),
        );
        assert_eq!(
            saved
                .root
                .find("electronics.component_creator.status.text")
                .and_then(|label| label.text_key.as_deref()),
            Some("electronics.component_creator.saved")
        );
        let save = saved.root.find(CREATOR_SAVE_ID).expect("save action");
        assert!(save.disabled);
    }

    #[test]
    fn focus_order_covers_every_editable_and_actionable_control() {
        let order = electronics_component_creator_focus_order();
        for id in [
            "electronics.component_creator.field.name.control",
            "electronics.component_creator.field.category.control",
            "electronics.component_creator.field.designator.control",
            "electronics.component_creator.field.value.control",
            "electronics.component_creator.field.pins.control",
            "electronics.component_creator.preset.passive",
            "electronics.component_creator.preset.box",
            "electronics.component_creator.preset.ic",
            "electronics.component_creator.preset.connector",
            "electronics.component_creator.preset.ground",
            "electronics.component_creator.field.footprint.control",
            CREATOR_CANCEL_ID,
            CREATOR_SAVE_ID,
        ] {
            assert!(
                order.iter().any(|entry| entry.as_str() == id),
                "missing focus id {id}"
            );
        }
    }

    #[test]
    fn every_preset_preview_stays_bounded() {
        for preset in [
            SymbolPreset::Passive,
            SymbolPreset::Box,
            SymbolPreset::Ic,
            SymbolPreset::Connector,
            SymbolPreset::Ground,
        ] {
            let mut draft = draft();
            draft.preset = preset;
            draft.pin_count = 16;
            let nodes = preview_ink(StudioUiPalette::IndustrialDark.tokens(), &draft);
            assert!(nodes.len() <= CREATOR_PREVIEW_MAX_RUNS + 256);
        }
    }
}
