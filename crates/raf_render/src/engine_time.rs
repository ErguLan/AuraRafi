//! Engine clock with a browser-safe `Instant`.
//!
//! Native builds use `std::time::Instant`. On `wasm32-unknown-unknown`,
//! `std::time::Instant::now()` panics because the standard library has no
//! clock implementation, so the browser path measures elapsed milliseconds
//! through `js_sys::Date`. Diagnostics accept the wall-clock caveat; the
//! renderer never uses this clock for simulation ordering.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct Instant(f64);

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        Self(js_sys::Date::now())
    }

    pub fn elapsed(&self) -> std::time::Duration {
        let millis = (js_sys::Date::now() - self.0).max(0.0);
        std::time::Duration::from_secs_f64(millis / 1000.0)
    }
}
