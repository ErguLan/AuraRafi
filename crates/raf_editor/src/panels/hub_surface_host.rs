//! Native Hub host compatibility surface.
//!
//! Hub behavior now lives in `native_studio.rs`; this module keeps the old
//! panel boundary available without resurrecting a second shell.

pub use crate::native_studio::{
    NativeStudioIntent as HubSurfaceIntent, NativeStudioSurface as HubSurfaceHost,
};
