//! Data-driven world-lighting presets.
//!
//! The palette values and patch payloads live in `presets.json` so adding a
//! new look never requires editing UI code. Every patch is validated through
//! the same property contract used by Console, Agent and the Inspector.

use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::OnceLock;

use super::EnvironmentSettings;

#[derive(Debug, Clone)]
pub struct EnvironmentPreset {
    pub id: String,
    pub name_key: String,
    pub description_key: String,
    /// Flat `environment.property` patch, ready for `lighting.set`.
    pub patch: Map<String, Value>,
    /// Fully validated settings used by previews and tests.
    pub settings: EnvironmentSettings,
}

#[derive(Deserialize)]
struct PresetFile {
    version: u32,
    presets: Vec<PresetTemplate>,
}

#[derive(Deserialize)]
struct PresetTemplate {
    id: String,
    name_key: String,
    description_key: String,
    patch: Map<String, Value>,
}

static PRESETS: OnceLock<Vec<EnvironmentPreset>> = OnceLock::new();

pub fn environment_presets() -> &'static [EnvironmentPreset] {
    PRESETS.get_or_init(|| {
        let file: PresetFile = serde_json::from_str(include_str!("presets.json"))
            .expect("environment presets must be valid JSON");
        assert_eq!(file.version, 1, "unsupported environment preset version");
        file.presets
            .into_iter()
            .filter_map(|template| {
                let settings = EnvironmentSettings::default()
                    .patched(&template.patch)
                    .ok()?;
                Some(EnvironmentPreset {
                    id: template.id,
                    name_key: template.name_key,
                    description_key: template.description_key,
                    patch: template.patch,
                    settings,
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_presets_are_valid_and_data_driven() {
        let presets = environment_presets();
        assert!(
            presets.len() >= 4,
            "expected Studio, Overcast, Sunset and Night"
        );
        for preset in presets {
            assert!(!preset.id.is_empty());
            assert!(!preset.name_key.is_empty());
            assert!(!preset.patch.is_empty());
            preset.settings.validate().unwrap();
            assert_ne!(
                preset.settings.render_fingerprint(),
                EnvironmentSettings::default().render_fingerprint(),
                "{} must produce a distinct look",
                preset.id
            );
        }
    }

    #[test]
    fn presets_compile_into_one_atomic_patch() {
        let preset = environment_presets()
            .iter()
            .find(|preset| preset.id == "sunset")
            .expect("sunset preset");
        let direct = EnvironmentSettings::default()
            .patched(&preset.patch)
            .unwrap();
        assert_eq!(direct, preset.settings);
    }
}
