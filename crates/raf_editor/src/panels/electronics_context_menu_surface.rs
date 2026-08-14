//! Retained RafUI context menu used by the Electronics canvas.
//!
//! The transitional editor shell only positions the popup. Menu semantics,
//! hit testing, styling, and actions belong to RafUI.

use std::hash::{Hash, Hasher};

use eframe::{egui, egui_wgpu};
use raf_core::{i18n::t, Language};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiMotionSpec, UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextStyle,
    UiTween,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

const MENU_WIDTH: f32 = 236.0;
const MENU_PADDING: f32 = 6.0;
const MENU_GAP: f32 = 2.0;
const TITLE_HEIGHT: f32 = 26.0;
const ACTION_HEIGHT: f32 = 28.0;
const MENU_MARGIN: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ElectronicsContextMenuTarget {
    Canvas {
        can_paste: bool,
    },
    Component {
        index: usize,
        label: String,
        locked: bool,
        has_datasheet: bool,
    },
    MultipleComponents {
        count: usize,
    },
    Wire {
        index: usize,
        label: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsContextMenuAction {
    EditValue(usize),
    Rotate(usize),
    Duplicate,
    ToggleLock(usize),
    OpenDatasheet(usize),
    RenameNet(usize),
    Delete,
    PlaceComponent,
    Paste,
    ElectricalTest,
}

#[derive(Debug, Default)]
pub struct ElectronicsContextMenuOutput {
    pub actions: Vec<ElectronicsContextMenuAction>,
    pub dismissed: bool,
}

pub struct ElectronicsContextMenuSurfaceHost {
    bridge: RafUiSurfaceBridge,
    active_key: Option<u64>,
    motion: UiTween,
    last_time_seconds: f64,
}

impl Default for ElectronicsContextMenuSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_electronics_context_menu"),
            active_key: None,
            motion: UiTween::new(0.0, UiMotionSpec::tooltip()),
            last_time_seconds: 0.0,
        }
    }
}

impl ElectronicsContextMenuSurfaceHost {
    pub fn close(&mut self) {
        self.active_key = None;
        self.motion.set_immediate(0.0);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        anchor: egui::Pos2,
        target: &ElectronicsContextMenuTarget,
    ) -> ElectronicsContextMenuOutput {
        let key = menu_key(target);
        let opened_this_frame = self.active_key != Some(key);
        if opened_this_frame {
            self.active_key = Some(key);
            self.motion.set_immediate(0.0);
            self.motion.set_target(1.0);
        }

        let now_seconds = ui.ctx().input(|input| input.time);
        let delta_seconds = if self.last_time_seconds <= 0.0 {
            0.0
        } else {
            (now_seconds - self.last_time_seconds).clamp(0.0, 0.25) as f32
        };
        self.last_time_seconds = now_seconds;
        let progress = self.motion.advance(delta_seconds, false);
        let popup_size = egui::vec2(MENU_WIDTH, menu_height(target));
        let screen = ui.ctx().screen_rect();
        let max_x = (screen.right() - popup_size.x - MENU_MARGIN).max(screen.left());
        let max_y = (screen.bottom() - popup_size.y - MENU_MARGIN).max(screen.top());
        let popup_pos = egui::pos2(
            anchor.x.clamp(screen.left() + MENU_MARGIN, max_x),
            (anchor.y - (1.0 - progress) * 6.0).clamp(screen.top() + MENU_MARGIN, max_y),
        );
        let popup_rect = egui::Rect::from_min_size(popup_pos, popup_size);
        let surface = build_surface(palette, language, target);
        let dispatched = egui::Area::new(egui::Id::new("rafui.electronics-context-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ui.ctx(), |popup_ui| {
                popup_ui
                    .allocate_ui_with_layout(
                        popup_size,
                        egui::Layout::top_down(egui::Align::Min),
                        |popup_ui| {
                            self.bridge.show_transparent(
                                popup_ui,
                                render_state,
                                palette,
                                surface,
                                |key| t(key, language),
                            )
                        },
                    )
                    .inner
            })
            .inner;

        let mut output = ElectronicsContextMenuOutput::default();
        for dispatched in dispatched {
            if let UiAction::Command { name } = dispatched.action {
                if name == "electronics.context.close" {
                    output.dismissed = true;
                } else if let Some(action) = parse_action(&name) {
                    output.actions.push(action);
                }
            }
        }

        let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
        let outside_pressed = !opened_this_frame
            && ui.ctx().input(|input| input.pointer.any_pressed())
            && !pointer.is_some_and(|position| popup_rect.contains(position));
        if outside_pressed || ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
            output.dismissed = true;
        }
        if !self.motion.is_settled() {
            ui.ctx().request_repaint();
        }
        output
    }
}

fn menu_key(target: &ElectronicsContextMenuTarget) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hasher);
    hasher.finish()
}

fn menu_height(target: &ElectronicsContextMenuTarget) -> f32 {
    let action_count = match target {
        ElectronicsContextMenuTarget::Canvas { .. } => 3,
        ElectronicsContextMenuTarget::Component { .. } => 6,
        ElectronicsContextMenuTarget::MultipleComponents { .. } => 2,
        ElectronicsContextMenuTarget::Wire { .. } => 2,
    };
    MENU_PADDING * 2.0
        + TITLE_HEIGHT
        + action_count as f32 * ACTION_HEIGHT
        + action_count as f32 * MENU_GAP
}

fn build_surface(
    palette: StudioUiPalette,
    language: Language,
    target: &ElectronicsContextMenuTarget,
) -> UiSurface {
    let tokens = palette.tokens();
    let title = match target {
        ElectronicsContextMenuTarget::Canvas { .. } => {
            MenuTitle::Key("app.electronics_main_schematic")
        }
        ElectronicsContextMenuTarget::Component { label, .. }
        | ElectronicsContextMenuTarget::Wire { label, .. } => MenuTitle::Value(label.clone()),
        ElectronicsContextMenuTarget::MultipleComponents { count } => MenuTitle::Value(format!(
            "{}: {count}",
            t("app.electronics_selection", language)
        )),
    };
    let mut root = UiNode::new("electronics.context.root", UiNodeKind::Menu)
        .with_class("electronics-context-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: MENU_GAP,
            padding: UiSpacing::same(MENU_PADDING),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_accessibility_label_key("app.more_menu")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "electronics.context.close",
        ));
    let title_node = match title {
        MenuTitle::Key(key) => {
            UiNode::new("electronics.context.title", UiNodeKind::Label).with_text_key(key)
        }
        MenuTitle::Value(value) => {
            UiNode::new("electronics.context.title", UiNodeKind::Label).with_text_value(value)
        }
    };
    root = root.with_child(
        title_node
            .with_text_style(UiTextStyle::panel_title(tokens.text))
            .with_layout(UiLayout::fixed(0.0, TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill)),
    );

    match target {
        ElectronicsContextMenuTarget::Canvas { can_paste } => {
            root = root
                .with_child(menu_button(
                    "place",
                    "app.electronics_place_component",
                    UiIconId::Add,
                    "electronics.context.place",
                    false,
                ))
                .with_child(
                    menu_button(
                        "paste",
                        "app.electronics_paste",
                        UiIconId::Project,
                        "electronics.context.paste",
                        false,
                    )
                    .disabled(!can_paste),
                )
                .with_child(menu_button(
                    "test",
                    "app.electrical_test",
                    UiIconId::Warning,
                    "electronics.context.test",
                    false,
                ));
        }
        ElectronicsContextMenuTarget::Component {
            index,
            locked,
            has_datasheet,
            ..
        } => {
            root = root
                .with_child(menu_button(
                    "edit-value",
                    "app.edit_value",
                    UiIconId::Settings,
                    format!("electronics.context.edit-value:{index}"),
                    false,
                ))
                .with_child(menu_button(
                    "rotate",
                    "app.rotate_r",
                    UiIconId::Rotate,
                    format!("electronics.context.rotate:{index}"),
                    false,
                ))
                .with_child(menu_button(
                    "duplicate",
                    "app.duplicate_ctrl_d",
                    UiIconId::Project,
                    "electronics.context.duplicate",
                    false,
                ))
                .with_child(menu_button(
                    "lock",
                    if *locked {
                        "app.electronics_unlock"
                    } else {
                        "app.electronics_lock"
                    },
                    if *locked {
                        UiIconId::Unlock
                    } else {
                        UiIconId::Lock
                    },
                    format!("electronics.context.toggle-lock:{index}"),
                    false,
                ))
                .with_child(
                    menu_button(
                        "datasheet",
                        "app.electronics_datasheet",
                        UiIconId::Assets,
                        format!("electronics.context.datasheet:{index}"),
                        false,
                    )
                    .disabled(!has_datasheet),
                )
                .with_child(menu_button(
                    "delete",
                    "app.delete_del",
                    UiIconId::Close,
                    "electronics.context.delete",
                    true,
                ));
        }
        ElectronicsContextMenuTarget::MultipleComponents { .. } => {
            root = root
                .with_child(menu_button(
                    "duplicate",
                    "app.duplicate_ctrl_d",
                    UiIconId::Project,
                    "electronics.context.duplicate",
                    false,
                ))
                .with_child(menu_button(
                    "delete",
                    "app.delete_del",
                    UiIconId::Close,
                    "electronics.context.delete",
                    true,
                ));
        }
        ElectronicsContextMenuTarget::Wire { index, .. } => {
            root = root
                .with_child(menu_button(
                    "rename-net",
                    "app.electronics_rename_net",
                    UiIconId::Settings,
                    format!("electronics.context.rename-net:{index}"),
                    false,
                ))
                .with_child(menu_button(
                    "delete-wire",
                    "app.delete_wire_del",
                    UiIconId::Close,
                    "electronics.context.delete",
                    true,
                ));
        }
    }

    let mut surface = UiSurface::new("electronics-context-menu", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

enum MenuTitle {
    Key(&'static str),
    Value(String),
}

fn menu_button(
    suffix: &str,
    label_key: &str,
    icon: UiIconId,
    command: impl Into<String>,
    danger: bool,
) -> UiNode {
    UiNode::new(format!("electronics.context.{suffix}"), UiNodeKind::Button)
        .with_class(if danger {
            "electronics-context-danger"
        } else {
            "electronics-context-action"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, ACTION_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([222, 226, 232, 255]).inherit_theme_color())
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-danger".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.danger),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("electronics-context-danger".to_string()),
                UiStylePatch {
                    fill: Some([tokens.danger[0], tokens.danger[1], tokens.danger[2], 36]),
                    border: Some(tokens.danger),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}

fn parse_action(name: &str) -> Option<ElectronicsContextMenuAction> {
    match name {
        "electronics.context.duplicate" => Some(ElectronicsContextMenuAction::Duplicate),
        "electronics.context.delete" => Some(ElectronicsContextMenuAction::Delete),
        "electronics.context.place" => Some(ElectronicsContextMenuAction::PlaceComponent),
        "electronics.context.paste" => Some(ElectronicsContextMenuAction::Paste),
        "electronics.context.test" => Some(ElectronicsContextMenuAction::ElectricalTest),
        _ => parse_index(name, "electronics.context.edit-value")
            .map(ElectronicsContextMenuAction::EditValue)
            .or_else(|| {
                parse_index(name, "electronics.context.rotate")
                    .map(ElectronicsContextMenuAction::Rotate)
            })
            .or_else(|| {
                parse_index(name, "electronics.context.toggle-lock")
                    .map(ElectronicsContextMenuAction::ToggleLock)
            })
            .or_else(|| {
                parse_index(name, "electronics.context.datasheet")
                    .map(ElectronicsContextMenuAction::OpenDatasheet)
            })
            .or_else(|| {
                parse_index(name, "electronics.context.rename-net")
                    .map(ElectronicsContextMenuAction::RenameNet)
            }),
    }
}

fn parse_index(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix)
        .and_then(|value| value.strip_prefix(':'))
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_actions_keep_target_indices() {
        assert_eq!(
            parse_action("electronics.context.rotate:7"),
            Some(ElectronicsContextMenuAction::Rotate(7))
        );
        assert_eq!(
            parse_action("electronics.context.rename-net:3"),
            Some(ElectronicsContextMenuAction::RenameNet(3))
        );
    }

    #[test]
    fn component_menu_fits_all_actions() {
        let target = ElectronicsContextMenuTarget::Component {
            index: 0,
            label: "D1 LED".to_string(),
            locked: false,
            has_datasheet: false,
        };
        assert!(menu_height(&target) >= TITLE_HEIGHT + ACTION_HEIGHT * 6.0);
    }
}
