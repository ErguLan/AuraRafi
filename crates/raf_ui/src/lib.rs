//! Lightweight retained UI data model.
//!
//! `raf_ui` is Rust-native and renderer-agnostic. It stores layout, style,
//! docking, text keys, and event bindings as serializable data. Rendering is
//! handled by `raf_render::ApiGraphicBasic`.

pub mod docking;
pub mod document;
pub mod events;
pub mod focus;
pub mod geometry;
pub mod hit_test;
pub mod interaction;
pub mod layout;
pub mod node;
pub mod style;
pub mod text;

pub use docking::{
    DockDropTarget, DockLayout, DockLayoutEntry, DockLayoutFrame, DockPanel, DockSide,
    DockWorkspaceController, DockWorkspaceEvent, FloatingPanel, FLOATING_PANEL_RESIZE_HANDLE_SIZE,
    FLOATING_PANEL_TITLE_BAR_HEIGHT,
};
pub use document::{
    UiCameraBinding, UiDocument, UiDocumentId, UiDocumentSpace, UI_DOCUMENT_VERSION,
};
pub use events::{UiAction, UiEventBinding, UiEventKind, UiPointerButton};
pub use focus::{UiFocusPolicy, UiFocusState, UiInputState};
pub use geometry::{UiRect, UiSpacing};
pub use hit_test::{hit_test, UiHitRegion, UiHitResult, UiHitTestMode};
pub use interaction::{UiDispatchedAction, UiInteractionState};
pub use layout::{UiFlow, UiLayout, UiPositionMode};
pub use node::{UiNode, UiNodeKind};
pub use style::{
    StudioUiPalette, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiTokens, UiVisualState,
};
pub use text::{
    UiFontWeight, UiTextAtlas, UiTextAtlasRect, UiTextAtlasRequest, UiTextAtlasSlot,
    UiTextAtlasSyncStats, UiTextRole, UiTextStyle,
};
