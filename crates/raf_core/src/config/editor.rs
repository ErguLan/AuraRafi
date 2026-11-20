//! Appearance, language, and scripting enums for engine settings.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/// Visual theme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// Supported UI languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    English,
    Spanish,
}

impl Default for Language {
    fn default() -> Self {
        Self::English
    }
}

impl Language {
    /// Fluent locale identifier.
    pub fn locale_id(&self) -> &str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
        }
    }

    /// Display name in native language.
    pub fn display_name(&self) -> &str {
        match self {
            Self::English => "English",
            Self::Spanish => "Espanol",
        }
    }
}

// ---------------------------------------------------------------------------
// Scripting types
// ---------------------------------------------------------------------------

/// Scripting language for the AuraRafi Host API.
/// See docs/SCRIPTING_SYSTEM.md for the tier system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptLanguage {
    /// Tier 1: sandboxed, pure Rust, beginner-friendly.
    Rhai,
    /// Tier 2: native-performance modules compiled to WASM.
    /// Requires a WASM runtime (Phase D).
    Cpp,
    /// Tier 3: visual node graphs from raf_nodes.
    Nodes,
}

impl Default for ScriptLanguage {
    fn default() -> Self {
        Self::Rhai
    }
}

impl ScriptLanguage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rhai => "Rhai",
            Self::Cpp => "C++ (WASM)",
            Self::Nodes => "Visual Nodes",
        }
    }

    pub fn all() -> [ScriptLanguage; 3] {
        [Self::Rhai, Self::Cpp, Self::Nodes]
    }
}

/// When scripts execute during the project lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptExecutionMode {
    /// Scripts do not run at all.
    Disabled,
    /// Scripts run only inside the editor (Play mode).
    EditorOnly,
    /// Scripts run in the editor and in exported runtime builds.
    Runtime,
}

impl Default for ScriptExecutionMode {
    fn default() -> Self {
        Self::EditorOnly
    }
}

impl ScriptExecutionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::EditorOnly => "Editor Only",
            Self::Runtime => "Runtime",
        }
    }

    pub fn all() -> [ScriptExecutionMode; 3] {
        [Self::Disabled, Self::EditorOnly, Self::Runtime]
    }
}

/// Bitflags for which script languages a project allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptLanguageFlags(pub u8);

impl ScriptLanguageFlags {
    pub const RHAI: Self = Self(0x01);
    pub const CPP: Self = Self(0x02);
    pub const NODES: Self = Self(0x04);
    pub const ALL: Self = Self(0x07);
    pub const DEFAULT: Self = Self(0x05); // Rhai + Nodes

    pub fn has(self, lang: ScriptLanguage) -> bool {
        let bit = match lang {
            ScriptLanguage::Rhai => 0x01,
            ScriptLanguage::Cpp => 0x02,
            ScriptLanguage::Nodes => 0x04,
        };
        (self.0 & bit) != 0
    }

    pub fn set(&mut self, lang: ScriptLanguage, enabled: bool) {
        let bit = match lang {
            ScriptLanguage::Rhai => 0x01,
            ScriptLanguage::Cpp => 0x02,
            ScriptLanguage::Nodes => 0x04,
        };
        if enabled {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

impl Default for ScriptLanguageFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}
