//! Audio operations: play, stop, set_volume.

use crate::host_api::ScriptContext;

pub fn play_audio(ctx: &mut ScriptContext<'_>, name: &str) {
    ctx.play_audio(name);
}

pub fn stop_audio(ctx: &mut ScriptContext<'_>, name: &str) {
    ctx.stop_audio(name);
}

pub fn set_volume(ctx: &mut ScriptContext<'_>, name: &str, volume: f32) {
    ctx.set_volume(name, volume);
}
