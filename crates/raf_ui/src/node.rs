use serde::{Deserialize, Serialize};

use crate::controls::{
    UiColorPicker, UiControl, UiImage, UiRange, UiScrollAxis, UiSelect, UiSkeleton, UiTextInput,
    UiToggle,
};
use crate::events::UiEventBinding;
use crate::icons::UiIcon;
use crate::layout::UiLayout;
use crate::style::{UiStyle, UiSurfaceMaterial};
use crate::text::{UiTextOverflow, UiTextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiAccessibilityRole {
    Generic,
    Button,
    Checkbox,
    Slider,
    Textbox,
    Combobox,
    Option,
    Menu,
    MenuItem,
    Tab,
    Dialog,
    Status,
}

impl Default for UiAccessibilityRole {
    fn default() -> Self {
        Self::Generic
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiNodeKind {
    Root,
    Panel,
    Toolbar,
    Button,
    Canvas,
    Overlay,
    Label,
    Separator,
    DockArea,
    FloatingPanel,
    Menu,
    Tooltip,
    Image,
    TextInput,
    ScrollView,
    Grid,
    Skeleton,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiNode {
    pub id: String,
    pub kind: UiNodeKind,
    pub text_key: Option<String>,
    /// Literal text for runtime values that must not pass through i18n.
    ///
    /// Keeping this separate from `text_key` prevents formatted values such as
    /// `"40 cm"` or `"75%"` from being mistaken for translation keys.
    #[serde(default)]
    pub text_value: Option<String>,
    #[serde(default)]
    pub classes: Vec<String>,
    pub layout: UiLayout,
    pub style: UiStyle,
    #[serde(default)]
    pub material: UiSurfaceMaterial,
    pub children: Vec<UiNode>,
    pub interactive: bool,
    /// Marks a literal text node as selectable without turning it into an
    /// editable text input. Hosts can use the shared RafUI selection and
    /// clipboard path for logs, documentation, chat messages, and inspectors.
    #[serde(default)]
    pub text_selectable: bool,
    #[serde(default)]
    pub focusable: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub invalid: bool,
    #[serde(default)]
    pub tooltip_key: Option<String>,
    /// Literal tooltip text for runtime values that must not pass through i18n.
    #[serde(default)]
    pub tooltip_value: Option<String>,
    #[serde(default)]
    pub accessibility_label_key: Option<String>,
    #[serde(default)]
    pub icon: Option<UiIcon>,
    #[serde(default)]
    pub text_style: Option<UiTextStyle>,
    #[serde(default)]
    pub text_overflow: UiTextOverflow,
    #[serde(default)]
    pub event_handlers: Vec<UiEventBinding>,
    #[serde(default)]
    pub accessibility_role: UiAccessibilityRole,
    #[serde(default)]
    pub accessibility_description_key: Option<String>,
    #[serde(default)]
    pub accessibility_expanded: Option<bool>,
    #[serde(default)]
    pub accessibility_checked: Option<bool>,
    #[serde(default)]
    pub accessibility_selected: Option<bool>,
    #[serde(default)]
    pub control: UiControl,
}

impl UiNode {
    pub fn new(id: impl Into<String>, kind: UiNodeKind) -> Self {
        Self {
            id: id.into(),
            kind,
            text_key: None,
            text_value: None,
            classes: Vec::new(),
            layout: UiLayout::default(),
            style: UiStyle::transparent(),
            material: UiSurfaceMaterial::Opaque,
            children: Vec::new(),
            interactive: false,
            text_selectable: false,
            focusable: false,
            disabled: false,
            invalid: false,
            tooltip_key: None,
            tooltip_value: None,
            accessibility_label_key: None,
            icon: None,
            text_style: None,
            text_overflow: UiTextOverflow::Wrap,
            event_handlers: Vec::new(),
            accessibility_role: UiAccessibilityRole::Generic,
            accessibility_description_key: None,
            accessibility_expanded: None,
            accessibility_checked: None,
            accessibility_selected: None,
            control: UiControl::None,
        }
    }

    pub fn image(id: impl Into<String>, image: UiImage) -> Self {
        Self::new(id, UiNodeKind::Image).with_control(UiControl::Image(image))
    }

    pub fn text_input(id: impl Into<String>, input: UiTextInput) -> Self {
        Self::new(id, UiNodeKind::TextInput).with_control(UiControl::TextInput(input))
    }

    pub fn toggle(id: impl Into<String>, toggle: UiToggle) -> Self {
        Self::new(id, UiNodeKind::Panel).with_control(UiControl::Toggle(toggle))
    }

    pub fn range(id: impl Into<String>, range: UiRange) -> Self {
        Self::new(id, UiNodeKind::Panel).with_control(UiControl::Range(range))
    }

    pub fn color_picker(id: impl Into<String>, picker: UiColorPicker) -> Self {
        Self::new(id, UiNodeKind::Panel).with_control(UiControl::ColorPicker(picker))
    }

    pub fn select(id: impl Into<String>, select: UiSelect) -> Self {
        Self::new(id, UiNodeKind::Button).with_control(UiControl::Select(select))
    }

    pub fn scroll_view(id: impl Into<String>, axis: UiScrollAxis) -> Self {
        Self::new(id, UiNodeKind::ScrollView).with_control(UiControl::ScrollView { axis })
    }

    pub fn grid(id: impl Into<String>) -> Self {
        Self::new(id, UiNodeKind::Grid).with_control(UiControl::Grid)
    }

    pub fn skeleton(id: impl Into<String>, skeleton: UiSkeleton) -> Self {
        Self::new(id, UiNodeKind::Skeleton).with_control(UiControl::Skeleton(skeleton))
    }

    pub fn with_text_key(mut self, text_key: impl Into<String>) -> Self {
        self.text_key = Some(text_key.into());
        self.text_value = None;
        self
    }

    pub fn with_text_value(mut self, text_value: impl Into<String>) -> Self {
        self.text_value = Some(text_value.into());
        self.text_key = None;
        self
    }

    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        let class = class.into();
        if !class.is_empty() && !self.classes.iter().any(|existing| existing == &class) {
            self.classes.push(class);
        }
        self
    }

    pub fn with_layout(mut self, layout: UiLayout) -> Self {
        self.layout = layout;
        self
    }

    pub fn with_style(mut self, style: UiStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_material(mut self, material: UiSurfaceMaterial) -> Self {
        self.material = material;
        self
    }

    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }

    /// Makes the node's rendered literal text selectable and copyable while
    /// keeping it read-only. The text should be supplied with `with_text_value`
    /// so the renderer and input system observe the same content.
    pub fn selectable_text(mut self) -> Self {
        self.text_selectable = true;
        self.interactive = true;
        self.focusable = true;
        self
    }

    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self.interactive = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn with_tooltip_key(mut self, tooltip_key: impl Into<String>) -> Self {
        self.tooltip_key = Some(tooltip_key.into());
        self.tooltip_value = None;
        self
    }

    pub fn with_tooltip_value(mut self, tooltip_value: impl Into<String>) -> Self {
        self.tooltip_value = Some(tooltip_value.into());
        self.tooltip_key = None;
        self
    }

    pub fn with_accessibility_label_key(mut self, label_key: impl Into<String>) -> Self {
        self.accessibility_label_key = Some(label_key.into());
        self
    }

    pub fn with_icon(mut self, icon: UiIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_text_style(mut self, text_style: UiTextStyle) -> Self {
        self.text_style = Some(text_style);
        self
    }

    pub fn with_text_overflow(mut self, overflow: UiTextOverflow) -> Self {
        self.text_overflow = overflow;
        self
    }

    pub fn with_event(mut self, binding: UiEventBinding) -> Self {
        self.event_handlers.push(binding);
        self.interactive = true;
        self
    }

    pub fn with_accessibility_role(mut self, role: UiAccessibilityRole) -> Self {
        self.accessibility_role = role;
        self
    }

    pub fn with_accessibility_description_key(
        mut self,
        description_key: impl Into<String>,
    ) -> Self {
        self.accessibility_description_key = Some(description_key.into());
        self
    }

    pub fn with_accessibility_expanded(mut self, expanded: bool) -> Self {
        self.accessibility_expanded = Some(expanded);
        self
    }

    pub fn with_accessibility_checked(mut self, checked: bool) -> Self {
        self.accessibility_checked = Some(checked);
        self
    }

    pub fn with_accessibility_selected(mut self, selected: bool) -> Self {
        self.accessibility_selected = Some(selected);
        self
    }

    pub fn with_control(mut self, control: UiControl) -> Self {
        self.control = control;
        match &self.control {
            UiControl::TextInput(_)
            | UiControl::Toggle(_)
            | UiControl::Range(_)
            | UiControl::ColorPicker(_)
            | UiControl::Select(_)
            | UiControl::ScrollView { .. } => {
                self.focusable = matches!(
                    &self.control,
                    UiControl::TextInput(_)
                        | UiControl::Toggle(_)
                        | UiControl::Range(_)
                        | UiControl::ColorPicker(_)
                        | UiControl::Select(_)
                );
                self.interactive = true;
            }
            _ => {}
        }
        self
    }

    pub fn with_child(mut self, child: UiNode) -> Self {
        self.children.push(child);
        self
    }

    /// Finds a node by its retained identity without exposing a second tree
    /// representation to hosts. Native adapters use this for focus/IME
    /// coordination while authored surfaces remain declarative.
    pub fn find(&self, id: &str) -> Option<&UiNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut UiNode> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.find_mut(id))
    }
}
