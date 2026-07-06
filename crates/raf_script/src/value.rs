//! Dynamic value type that crosses the script-engine boundary.
//!
//! Rhai, WASM, and Visual Nodes all produce/consume `ScriptValue`. It is the
//! single lingua franca for data flowing through the Host API.

use serde::{Deserialize, Serialize};

/// A runtime value exchanged between scripts and the engine.
///
/// All numeric transforms are in SI units (meters, radians, seconds).
/// Colors are 0-255 RGBA. Strings are UTF-8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScriptValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f32),
    String(String),
    Vec3([f32; 3]),
    Color([u8; 4]),
    Handle(u64),
    List(Vec<ScriptValue>),
}

impl Default for ScriptValue {
    fn default() -> Self {
        Self::None
    }
}

impl ScriptValue {
    /// Coerce to bool. Non-empty strings and non-zero numbers are true.
    pub fn as_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::String(s) => !s.is_empty(),
            _ => false,
        }
    }

    /// Coerce to f32 (meters, radians, seconds).
    pub fn as_float(&self) -> f32 {
        match self {
            Self::Float(f) => *f,
            Self::Int(i) => *i as f32,
            Self::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    /// Coerce to i64.
    pub fn as_int(&self) -> i64 {
        match self {
            Self::Int(i) => *i,
            Self::Float(f) => *f as i64,
            Self::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Coerce to string.
    pub fn as_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Float(f) => format!("{}", f),
            Self::Int(i) => format!("{}", i),
            Self::Bool(b) => format!("{}", b),
            Self::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
            Self::Color(c) => format!("rgba({},{},{},{})", c[0], c[1], c[2], c[3]),
            Self::Handle(h) => format!("handle:{}", h),
            Self::List(items) => format!("[{} items]", items.len()),
            Self::None => String::new(),
        }
    }

    /// Coerce to a Vec3 array. Strings/numbers broadcast to all axes.
    pub fn as_vec3(&self) -> [f32; 3] {
        match self {
            Self::Vec3(v) => *v,
            Self::Float(f) => [*f, *f, *f],
            Self::Int(i) => {
                let f = *i as f32;
                [f, f, f]
            }
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Coerce to a Color array (RGBA 0-255).
    pub fn as_color(&self) -> [u8; 4] {
        match self {
            Self::Color(c) => *c,
            Self::Int(i) => {
                let v = (*i as u8).max(0).min(255);
                [v, v, v, 255]
            }
            _ => [255, 255, 255, 255],
        }
    }

    /// Extract a raw handle id.
    pub fn as_handle(&self) -> Option<u64> {
        match self {
            Self::Handle(h) => Some(*h),
            _ => None,
        }
    }

    /// Build a Vec3 value.
    pub fn vec3(x: f32, y: f32, z: f32) -> Self {
        Self::Vec3([x, y, z])
    }

    /// Build a Color value from 0-255 components.
    pub fn color(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::Color([r, g, b, a])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_float() {
        assert_eq!(ScriptValue::Int(5).as_float(), 5.0);
        assert_eq!(ScriptValue::Bool(true).as_float(), 1.0);
    }

    #[test]
    fn coerce_vec3() {
        assert_eq!(ScriptValue::Float(2.0).as_vec3(), [2.0, 2.0, 2.0]);
        assert_eq!(ScriptValue::Vec3([1.0, 2.0, 3.0]).as_vec3(), [1.0, 2.0, 3.0]);
    }
}
