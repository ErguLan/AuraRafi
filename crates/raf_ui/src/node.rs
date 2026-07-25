use serde::{Deserialize, Serialize};

use crate::controls::{
    UiControl, UiImage, UiRange, UiScrollAxis, UiSkeleton, UiTextInput, UiToggle,
};
use crate::events::UiEventBinding;
use crate::layout::UiLayout;
use crate::style::UiStyle;
use crate::text::UiTextStyle;

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
    #[serde(default)]
    pub classes: Vec<String>,
    pub layout: UiLayout,
    pub style: UiStyle,
    pub children: Vec<UiNode>,
    pub interactive: bool,
    #[serde(default)]
    pub focusable: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub tooltip_key: Option<String>,
    #[serde(default)]
    pub accessibility_label_key: Option<String>,
    #[serde(default)]
    pub text_style: Option<UiTextStyle>,
    #[serde(default)]
    pub event_handlers: Vec<UiEventBinding>,
    #[serde(default)]
    pub control: UiControl,
}

impl UiNode {
    pub fn new(id: impl Into<String>, kind: UiNodeKind) -> Self {
        Self {
            id: id.into(),
            kind,
            text_key: None,
            classes: Vec::new(),
            layout: UiLayout::default(),
            style: UiStyle::transparent(),
            children: Vec::new(),
            interactive: false,
            focusable: false,
            disabled: false,
            tooltip_key: None,
            accessibility_label_key: None,
            text_style: None,
            event_handlers: Vec::new(),
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

    pub fn interactive(mut self) -> Self {
        self.interactive = true;
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

    pub fn with_tooltip_key(mut self, tooltip_key: impl Into<String>) -> Self {
        self.tooltip_key = Some(tooltip_key.into());
        self
    }

    pub fn with_accessibility_label_key(mut self, label_key: impl Into<String>) -> Self {
        self.accessibility_label_key = Some(label_key.into());
        self
    }

    pub fn with_text_style(mut self, text_style: UiTextStyle) -> Self {
        self.text_style = Some(text_style);
        self
    }

    pub fn with_event(mut self, binding: UiEventBinding) -> Self {
        self.event_handlers.push(binding);
        self.interactive = true;
        self
    }

    pub fn with_control(mut self, control: UiControl) -> Self {
        self.control = control;
        match &self.control {
            UiControl::TextInput(_)
            | UiControl::Toggle(_)
            | UiControl::Range(_)
            | UiControl::ScrollView { .. } => {
                self.focusable = matches!(
                    &self.control,
                    UiControl::TextInput(_) | UiControl::Toggle(_) | UiControl::Range(_)
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
}
