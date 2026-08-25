//! Compatibility entry point for integrations that used the former editor
//! shell name.
//!
//! The old 5k-line monolithic coordinator is gone. The authoritative application is
//! now the smaller Winit/RafUI/ApiGraphicBasic composition in
//! `native_application.rs`; this module deliberately contains no second
//! event loop or second viewport implementation.

pub use crate::native_application::{run_native, NativeEditorApplication};
