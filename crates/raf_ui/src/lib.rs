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
pub mod interaction;
pub mod layout;
pub mod menu;
pub mod motion;
pub mod node;
pub mod overlays;
pub mod state;
pub mod studio;
pub mod studio_diagnostics;
pub mod studio_inspector;
pub mod studio_quality;
pub mod studio_recipes;
pub mod studio_snapshots;
pub mod studio_validator;
pub mod style;
pub mod text;

pub use components::{
    editor_tab, empty_state, floating_action_rail, icon_button, inspector_field, panel_header,
    segmented_option, technical_toolbar, tooltip_node, tree_row,
};
pub use controls::{
    UiControl, UiImage, UiImageFit, UiImageSource, UiRange, UiScrollAxis, UiSkeleton,
    UiSkeletonShape, UiTextInput, UiToggle,
};
pub use docking::{
    DockDropTarget, DockLayout, DockLayoutEntry, DockLayoutFrame, DockPanel, DockPanelPolicy,
    DockSide, DockWorkspaceController, DockWorkspaceEvent, FloatingPanel,
    FLOATING_PANEL_RESIZE_HANDLE_SIZE, FLOATING_PANEL_TITLE_BAR_HEIGHT,
};
pub use document::{
    UiCameraBinding, UiDocument, UiDocumentId, UiDocumentSpace, UI_DOCUMENT_VERSION,
};
pub use environment::{
    UiColorMode, UiDensityContract, UiEnvironment, UiGeometrySnap, UiSamplingMode,
};
pub use events::{UiAction, UiEventBinding, UiEventKind, UiPointerButton};
pub use focus::{UiFocusPolicy, UiFocusState, UiInputState};
pub use geometry::{UiRect, UiSpacing};
pub use hit_test::{hit_test, UiHitRegion, UiHitResult, UiHitTestMode};
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
pub use state::UiControlState;
pub use studio::{
    RafUiStudio, UiStudioEdit, UiStudioRecipe, UiStudioRecipeKind, UiStudioTextPreview,
};
pub use studio_diagnostics::{
    UiStudioDiagnostic, UiStudioDiagnosticCode, UiStudioDiagnosticSeverity, UiStudioDocumentReport,
};
pub use studio_inspector::{UiStudioNodeInspection, UiStudioNodePath, UiStudioPropertyGroup};
pub use studio_quality::{
    UiStudioDensityReport, UiStudioDpiCase, UiStudioDpiMatrix, UiStudioDpiReport,
};
pub use studio_recipes::{UiStudioRecipeCatalog, UiStudioRecipeSpec, UI_STUDIO_RECIPE_VERSION};
pub use studio_snapshots::{
    UiStudioGoldenSnapshot, UiStudioPixelDiff, UiStudioSnapshotCase, UiStudioSnapshotResult,
};
pub use studio_validator::{
    UiStudioCommandRegistry, UiStudioValidationOptions, UiStudioValidationReport,
};
pub use style::{
    StudioUiPalette, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiTheme, UiThemeMetrics, UiTokens, UiVisualState,
};
pub use text::{
    UiFontWeight, UiTextAtlas, UiTextAtlasRect, UiTextAtlasRequest, UiTextAtlasSlot,
    UiTextAtlasSyncStats, UiTextRole, UiTextStyle,
};
