//! The ScriptContext: the single entry point for all script execution.
//!
//! Every tier (Rhai, WASM, Visual Nodes) receives a `&mut ScriptContext`
//! and calls the same Host API functions. No tier touches `SceneGraph`
//! directly.
//!
//! Construction is the responsibility of the (future) `ScriptRuntime`
//! system in the editor or standalone runtime. The context is built
//! per-frame from live engine state.

use raf_core::scene::graph::SceneNodeId;
use raf_core::scene::SceneGraph;

use crate::node_handle::NodeHandle;
use crate::value::ScriptValue;
use crate::ScriptResult;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Snapshot of input state for one frame. Filled by the editor/runtime
/// from the host window's input events. Engine-agnostic: does not depend
/// on a particular UI or window framework.
#[derive(Debug, Clone, Default)]
pub struct InputSnapshot {
    /// Keys currently held down, normalized to lowercase strings
    /// (e.g. "w", "space", "ctrl", "shift").
    pub keys_held: Vec<String>,
    /// Keys pressed this frame (edge-triggered).
    pub keys_pressed: Vec<String>,
    /// Mouse buttons held (0=left, 1=right, 2=middle).
    pub mouse_held: Vec<i32>,
}

impl InputSnapshot {
    /// Check if a key is currently held.
    pub fn is_key_held(&self, key: &str) -> bool {
        let lower = key.to_lowercase();
        self.keys_held.iter().any(|k| *k == lower)
    }

    /// Check if a key was pressed this frame (edge).
    pub fn was_key_pressed(&self, key: &str) -> bool {
        let lower = key.to_lowercase();
        self.keys_pressed.iter().any(|k| *k == lower)
    }

    /// Check if a mouse button is held.
    pub fn is_mouse_held(&self, button: i32) -> bool {
        self.mouse_held.iter().any(|&b| b == button)
    }
}

/// Queue of audio commands produced by scripts during a frame.
/// Drained by the (future) audio system after script execution.
#[derive(Debug, Clone, Default)]
pub struct AudioCommandQueue {
    pub commands: Vec<AudioCommand>,
}

#[derive(Debug, Clone)]
pub struct ScriptEvent {
    pub name: String,
    pub target: Option<NodeHandle>,
    pub value: ScriptValue,
}
#[derive(Debug, Clone, Default)]
pub struct ScriptEventQueue {
    pub events: Vec<ScriptEvent>,
}

/// A single audio command emitted by a script.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play { name: String },
    Stop { name: String },
    SetVolume { name: String, volume: f32 },
}

impl AudioCommandQueue {
    pub fn play(&mut self, name: &str) {
        self.commands.push(AudioCommand::Play {
            name: name.to_string(),
        });
    }

    pub fn stop(&mut self, name: &str) {
        self.commands.push(AudioCommand::Stop {
            name: name.to_string(),
        });
    }

    pub fn set_volume(&mut self, name: &str, volume: f32) {
        self.commands.push(AudioCommand::SetVolume {
            name: name.to_string(),
            volume,
        });
    }

    /// Drain all queued commands (called by the audio system after scripts run).
    pub fn drain(&mut self) -> Vec<AudioCommand> {
        std::mem::take(&mut self.commands)
    }
}

/// Time info for the current frame.
#[derive(Debug, Clone, Copy)]
pub struct TimeInfo {
    /// Seconds since the scene was loaded.
    pub elapsed: f32,
    /// Seconds since the last frame.
    pub delta_time: f32,
}

impl Default for TimeInfo {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            delta_time: 0.016,
        }
    }
}

/// The per-frame context passed to every script.
///
/// Scripts receive this and call Host API functions on it. The context
/// owns mutable access to the scene, audio queue, and read-only input/time.
pub struct ScriptContext<'a> {
    pub view: &'a mut crate::view::RuntimeViewState,
    pub scene: &'a mut SceneGraph,
    pub input: &'a InputSnapshot,
    pub audio: &'a mut AudioCommandQueue,
    pub events: &'a mut ScriptEventQueue,
    pub time: TimeInfo,
    /// Namespace shared by scripts within one runtime, never across players.
    pub instance_id: u64,
    pub owner: Option<SceneNodeId>,
    /// Includes tombstones so repeated spawn/destroy cannot grow without bound.
    pub entity_limit: usize,
    pub cancellation: Option<&'a AtomicBool>,
    pub deadline: Option<Instant>,
}

impl<'a> ScriptContext<'a> {
    /// Events are delivered at the next safe tick, never by a reentrant call.
    pub fn emit_event(
        &mut self,
        target: Option<NodeHandle>,
        name: &str,
        value: ScriptValue,
    ) -> ScriptResult<()> {
        if self.events.events.len() >= 256 || name.is_empty() || name.len() > 128 {
            return Err(crate::ScriptError::InvalidArgument(
                "event queue budget exceeded or invalid event name".into(),
            ));
        }
        if let Some(target) = target {
            target.resolve(self)?;
        }
        match &value {
            ScriptValue::None | ScriptValue::Bool(_) | ScriptValue::Int(_) => {}
            ScriptValue::Float(v) if v.is_finite() => {}
            ScriptValue::String(v) if v.len() <= 4096 => {}
            _ => {
                return Err(crate::ScriptError::InvalidArgument(
                    "event payload requires a bounded scalar value".into(),
                ))
            }
        }
        self.events.events.push(ScriptEvent {
            name: name.into(),
            target,
            value,
        });
        Ok(())
    }
    pub fn check_budget(&self) -> ScriptResult<()> {
        if self
            .cancellation
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(crate::ScriptError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(crate::ScriptError::Timeout);
        }
        Ok(())
    }
    /// Delta time in seconds (convenience accessor).
    pub fn delta_time(&self) -> f32 {
        self.time.delta_time
    }

    /// Elapsed time in seconds since scene load.
    pub fn elapsed(&self) -> f32 {
        self.time.elapsed
    }

    // -----------------------------------------------------------------------
    // Scene operations (host::scene_ops)
    // -----------------------------------------------------------------------

    /// Find a root-level entity by name.
    pub fn get_node(&self, name: &str) -> Option<NodeHandle> {
        if name.is_empty() {
            return None;
        }
        let mut matches = self
            .scene
            .iter()
            .filter(|(id, node)| self.scene.is_valid_node(*id) && node.name == name);
        let (id, _) = matches.next()?;
        // Ambiguous names are not an implicit association either.
        if matches.next().is_some() {
            return None;
        }
        Some(NodeHandle::scoped(id, self))
    }

    pub fn self_node(&self) -> Option<NodeHandle> {
        let id = self.owner?;
        self.scene
            .is_valid_node(id)
            .then(|| NodeHandle::scoped(id, self))
    }

    pub fn find_child(&self, parent: NodeHandle, name: &str) -> ScriptResult<Option<NodeHandle>> {
        let id = parent.resolve(self)?;
        let node = self
            .scene
            .get(id)
            .ok_or(crate::ScriptError::InvalidHandle(parent.raw()))?;
        let mut matches = node.children.iter().filter(|id| {
            self.scene.is_valid_node(**id)
                && self.scene.get(**id).is_some_and(|node| node.name == name)
        });
        let Some(&child) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Ok(None);
        }
        Ok(Some(NodeHandle::scoped(child, self)))
    }

    pub fn get_parent(&self, child: NodeHandle) -> ScriptResult<Option<NodeHandle>> {
        let id = child.resolve(self)?;
        Ok(self
            .scene
            .get(id)
            .and_then(|node| node.parent)
            .filter(|id| self.scene.is_valid_node(*id))
            .map(|id| NodeHandle::scoped(id, self)))
    }

    /// Spawn a new entity with a primitive shape. Returns a handle.
    pub fn spawn_entity(&mut self, name: &str, primitive: &str) -> ScriptResult<NodeHandle> {
        if self.scene.len() >= self.entity_limit || name.is_empty() || name.len() > 256 {
            return Err(crate::ScriptError::InvalidArgument(
                "entity budget exceeded or invalid name".into(),
            ));
        }
        let prim = parse_primitive(primitive)?;
        let id = self.scene.add_root_with_primitive(name, prim);
        Ok(NodeHandle::scoped(id, self))
    }

    /// Destroy an entity by handle.
    pub fn destroy_entity(&mut self, handle: NodeHandle) -> ScriptResult<()> {
        let id = handle.resolve(self)?;
        let removed = self.scene.remove_node(id);
        if removed {
            Ok(())
        } else {
            Err(crate::errors::ScriptError::InvalidHandle(handle.raw()))
        }
    }

    // -----------------------------------------------------------------------
    // Input operations (host::input_ops)
    // -----------------------------------------------------------------------

    /// Check if a key is currently held down.
    pub fn is_key_pressed(&self, key: &str) -> bool {
        self.input.is_key_held(key)
    }

    /// Check if a key was pressed this frame (edge-triggered).
    pub fn was_key_just_pressed(&self, key: &str) -> bool {
        self.input.was_key_pressed(key)
    }

    /// Check if a mouse button is held (0=left, 1=right, 2=middle).
    pub fn is_mouse_pressed(&self, button: i32) -> bool {
        self.input.is_mouse_held(button)
    }

    // -----------------------------------------------------------------------
    // Audio operations (host::audio_ops)
    // -----------------------------------------------------------------------

    /// Play an audio asset by name.
    pub fn play_audio(&mut self, name: &str) {
        if self.audio.commands.len() < 256 {
            self.audio.play(name);
        }
    }

    /// Stop a playing audio asset.
    pub fn stop_audio(&mut self, name: &str) {
        if self.audio.commands.len() < 256 {
            self.audio.stop(name);
        }
    }

    /// Set the volume of an audio source (0.0 to 1.0).
    pub fn set_volume(&mut self, name: &str, volume: f32) {
        if self.audio.commands.len() < 256 {
            self.audio.set_volume(name, volume.clamp(0.0, 1.0));
        }
    }

    // -----------------------------------------------------------------------
    // Time operations (host::time_ops)
    // -----------------------------------------------------------------------

    /// Get the delta time in seconds for this frame.
    pub fn get_delta_time(&self) -> f32 {
        self.time.delta_time
    }

    /// Get elapsed time in seconds since scene load.
    pub fn get_elapsed_time(&self) -> f32 {
        self.time.elapsed
    }

    // -----------------------------------------------------------------------
    // Script interop (host::interop_ops)
    // -----------------------------------------------------------------------

    /// Call a function defined in another script.
    /// Tier 1 only for now (Rhai calling Rhai). WASM interop in Phase D.
    pub fn call_script_function(
        &mut self,
        _script_path: &str,
        _function: &str,
        _args: Vec<ScriptValue>,
    ) -> ScriptResult<ScriptValue> {
        Err(crate::ScriptError::InvalidArgument(
            "direct inter-script calls are not supported; use emit_event or send_event".into(),
        ))
    }
}

/// Parse a primitive name string into a `Primitive` enum value.
fn parse_primitive(name: &str) -> ScriptResult<raf_core::scene::Primitive> {
    use raf_core::scene::Primitive;
    Ok(match name.to_lowercase().as_str() {
        "cube" | "box" => Primitive::Cube,
        "sphere" | "ball" => Primitive::Sphere,
        "plane" | "ground" => Primitive::Plane,
        "cylinder" | "tube" => Primitive::Cylinder,
        // Sprite2D was retired: 2D game content is an orthographic 3D plane.
        "sprite" | "sprite2d" | "billboard" => Primitive::Plane,
        "empty" | "group" => Primitive::Empty,
        _ => {
            return Err(crate::ScriptError::InvalidArgument(format!(
                "unknown primitive: {name}"
            )))
        }
    })
}
