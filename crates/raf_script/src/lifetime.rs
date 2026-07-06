//! Script lifecycle contract.
//!
//! Every script (Rhai or WASM) may define zero or more of these functions.
//! The runtime calls them at the appropriate times. All are optional.

/// The three lifecycle hooks a script can implement.
pub const HOOK_ON_START: &str = "on_start";
pub const HOOK_ON_UPDATE: &str = "on_update";
pub const HOOK_ON_DESTROY: &str = "on_destroy";

/// Description of when each hook is called.
pub fn hook_description(hook: &str) -> &'static str {
    match hook {
        HOOK_ON_START => "Called once when the scene is loaded or Play mode starts.",
        HOOK_ON_UPDATE => "Called every frame with delta_time (seconds) as argument.",
        HOOK_ON_DESTROY => "Called once when the scene is unloaded or Play mode stops.",
        _ => "Unknown hook.",
    }
}
