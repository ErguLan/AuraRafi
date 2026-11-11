//! Lightweight retained UI data model.
//!
//! `raf_ui` is Rust-native and renderer-agnostic. It stores layout, style,
//! docking, text keys, and event bindings as serializable data. Rendering is
//! handled by `raf_render::ApiGraphicBasic`.

pub mod action_bar;
pub mod automation;
pub mod authoring;
pub mod components;
mod composition;
pub mod editing;
pub mod controls;
pub mod docking;
pub mod document;
pub mod environment;
pub mod events;
pub mod focus;
pub mod geometry;
pub mod game_ui;
pub mod hit_test;
mod icon_catalog;
pub mod icons;
pub mod interaction;
pub mod layout;
pub mod menu;
pub mod motion;
pub mod node;
pub mod numeric;
pub mod overlays;
pub mod performance;
pub mod state;
pub mod style;
pub mod text;
pub mod table;
pub mod visual;
pub mod window;

pub use action_bar::{
    action_bar_node, plan_action_bar, UiActionBarItem, UiActionBarPlan, UiActionPriority,
};
pub use components::{
    disclosure_header, dropdown_option, dropdown_trigger, editor_tab, empty_state,
    floating_action_rail, form_field, icon_button, icon_button_with_icon, inline_text_row,
    inspector_field, numeric_field, panel_header, password_field, search_field, segmented_group,
    segmented_option, select_trigger, status_badge, technical_toolbar, thumbnail_image,
    tooltip_node, tree_row, tree_row_with_icon, UiSegmentedOption, UiSegmentedOrientation,
    UiStatusTone,
};
pub use components::{UiButtonVariant, UiComponents, UiImageView, UiImageViewShape};
pub use editing::{UiTextBinding, UiTextDraftBank};
pub use controls::{
    UiColorPicker, UiColorPickerHit, UiControl, UiImage, UiImageFit, UiImageInsets, UiImageOptions,
    UiImagePlaceholder, UiImageRect, UiImageResourceStatus, UiImageSampling, UiImageSource,
    UiRange, UiRangeOrientation, UiScrollAxis, UiSelect, UiSelectOption, UiSkeleton,
    UiSkeletonShape, UiTextInput, UiToggle, UiTogglePresentation,
};
pub use docking::{
    BottomDockLayout, DockDropTarget, DockLayout, DockLayoutEntry, DockLayoutFrame, DockPanel,
    DockPanelPolicy, DockSide, DockTab, DockTabGroup, DockWorkspaceController, DockWorkspaceEvent,
    FloatingPanel, BOTTOM_DOCK_LAYOUT_VERSION, FLOATING_PANEL_RESIZE_HANDLE_SIZE,
    FLOATING_PANEL_TITLE_BAR_HEIGHT, MAX_BOTTOM_DOCK_GROUPS,
};
pub use document::{
    UiCameraBinding, UiDocument, UiDocumentId, UiDocumentSpace, UiDocumentUpdate,
    UiDocumentUpdateError, UI_DOCUMENT_VERSION,
};
pub use environment::{
    UiColorMode, UiDensityContract, UiEnvironment, UiGeometrySnap, UiSamplingMode,
};
pub use events::{UiAction, UiCursorIcon, UiEventBinding, UiEventKind, UiPointerButton};
pub use focus::{UiFocusPolicy, UiFocusState, UiInputState, UiModifiers, KEYBOARD_CAPTURE_TEMP_ID};
pub use geometry::{UiRect, UiSpacing};
pub use hit_test::{hit_test, UiHitRegion, UiHitResult, UiHitTestMode};
pub use icons::{UiIcon, UiIconDescriptor, UiIconId, UiIconSize, UiIconState, UiIconTone};
pub use interaction::{UiDispatchedAction, UiInteractionState};
pub use layout::{
    UiAlign, UiCompactMode, UiFlow, UiGridLayout, UiJustify, UiLayout, UiOverflow, UiPositionMode,
    UiResponsiveRule, UiSizeMode,
};
pub use menu::{UiApplicationMenu, UiMenu, UiMenuActivation, UiMenuCommand, UiMenuItem};
pub use motion::{UiEasing, UiMotionSpec, UiTween};
pub use node::{UiAccessibilityRole, UiNode, UiNodeKind};
pub use numeric::{
    UiNumericBinding, UiNumericDraftIssue, UiNumericEditAction, UiNumericEditBank,
    UiNumericEditSession, UiNumericEditValue, UiNumericSpec, UiNumericUnit, UiNumericValidation,
};
pub use overlays::{
    place_overlay, UiOverlayLayer, UiOverlayManager, UiOverlayPlacement, UiOverlayRequest,
    UiPlacement,
};
pub use state::{UiControlState, UiTextEditState, UiVirtualRange};
pub use performance::{UiPerformanceMonitor, UiPerformanceSnapshot, UiRegionCost, UiWorkAllowance, UiWorkCause, UiWorkLimits, UiWorkPhase, UiWorkSample};
pub use style::{
    StudioUiPalette, UiComponentMetricOverrides, UiComponentMetrics, UiControlKind,
    UiResolvedComponentMetrics, UiStyle, UiStyleLayer, UiStyleLayers, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurfaceMaterial, UiTheme, UiThemeMetrics,
    UiTokens, UiVisualState,
};
pub use text::{
    UiFontWeight, UiTextAtlasRequest, UiTextOverflow, UiTextRole, UiTextRun, UiTextRunContent,
    UiTextStyle,
};
pub use visual::{
    UiChromeTokens, UiContentAlign, UiContentLayout, UiFocusRing, UiIconPlacement,
    UiLinearGradient, UiShadow, UiVisual,
};
pub use window::{UiResizeEdge, UiWindowCommand, UiWindowHitTest};
