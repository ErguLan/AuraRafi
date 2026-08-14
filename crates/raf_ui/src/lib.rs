//! Lightweight retained UI data model.
//!
//! `raf_ui` is Rust-native and renderer-agnostic. It stores layout, style,
//! docking, text keys, and event bindings as serializable data. Rendering is
//! handled by `raf_render::ApiGraphicBasic`.

pub mod components;
pub mod controls;
pub mod docking;
pub mod document;
pub mod environment;
pub mod events;
pub mod focus;
pub mod geometry;
pub mod hit_test;
pub mod icons;
pub mod interaction;
pub mod layout;
pub mod menu;
pub mod motion;
pub mod node;
pub mod overlays;
pub mod state;
pub mod style;
pub mod text;
pub mod window;

pub use components::{
    editor_tab, empty_state, floating_action_rail, icon_button, icon_button_with_icon,
    inspector_field, panel_header, segmented_option, technical_toolbar, tooltip_node, tree_row,
    tree_row_with_icon,
};
pub use controls::{
    UiControl, UiImage, UiImageFit, UiImageSource, UiRange, UiScrollAxis, UiSkeleton,
    UiSkeletonShape, UiTextInput, UiToggle,
};
pub use docking::{
    BottomDockLayout, DockDropTarget, DockLayout, DockLayoutEntry, DockLayoutFrame, DockPanel,
    DockPanelPolicy, DockSide, DockTab, DockTabGroup, DockWorkspaceController, DockWorkspaceEvent,
    FloatingPanel, BOTTOM_DOCK_LAYOUT_VERSION, FLOATING_PANEL_RESIZE_HANDLE_SIZE,
    FLOATING_PANEL_TITLE_BAR_HEIGHT, MAX_BOTTOM_DOCK_GROUPS,
};
pub use document::{
    UiCameraBinding, UiDocument, UiDocumentId, UiDocumentSpace, UI_DOCUMENT_VERSION,
};
pub use environment::{
    UiColorMode, UiDensityContract, UiEnvironment, UiGeometrySnap, UiSamplingMode,
};
pub use events::{UiAction, UiCursorIcon, UiEventBinding, UiEventKind, UiPointerButton};
pub use focus::{UiFocusPolicy, UiFocusState, UiInputState, UiModifiers, KEYBOARD_CAPTURE_TEMP_ID};
pub use geometry::{UiRect, UiSpacing};
pub use hit_test::{hit_test, UiHitRegion, UiHitResult, UiHitTestMode};
pub use icons::{UiIcon, UiIconId, UiIconSize, UiIconState};
pub use interaction::{UiDispatchedAction, UiInteractionState};
pub use layout::{
    UiAlign, UiCompactMode, UiFlow, UiGridLayout, UiJustify, UiLayout, UiOverflow, UiPositionMode,
    UiResponsiveRule, UiSizeMode,
};
pub use menu::{UiApplicationMenu, UiMenu, UiMenuActivation, UiMenuCommand, UiMenuItem};
pub use motion::{UiEasing, UiMotionSpec, UiTween};
pub use node::{UiNode, UiNodeKind};
pub use overlays::{
    place_overlay, UiOverlayLayer, UiOverlayManager, UiOverlayPlacement, UiOverlayRequest,
    UiPlacement,
};
pub use state::{UiControlState, UiTextEditState, UiVirtualRange};
pub use style::{
    StudioUiPalette, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiTheme, UiThemeMetrics, UiTokens, UiVisualState,
};
pub use text::{UiFontWeight, UiTextAtlasRequest, UiTextRole, UiTextStyle};
pub use window::{UiResizeEdge, UiWindowCommand, UiWindowHitTest};
