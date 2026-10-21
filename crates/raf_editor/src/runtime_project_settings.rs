//! Validated project-runtime edits shared by retained settings intents.
use raf_core::runtime_config::{ProjectRuntimeSettings, RuntimeInputAction, RuntimeInputAxis};

pub fn apply_text(
    settings: &mut ProjectRuntimeSettings,
    field: &str,
    value: &str,
) -> Result<(), String> {
    if value.len() > 8192 {
        return Err("runtime.settings_too_large".into());
    }
    match field {
        "active_camera" | "startup_session" => {
            let id = if value.trim().is_empty() {
                None
            } else {
                Some(uuid::Uuid::parse_str(value.trim()).map_err(|e| e.to_string())?)
            };
            if field == "active_camera" {
                settings.active_camera = id;
            } else {
                settings.startup_session = id;
            }
        }
        "input_actions" => {
            let actions: Vec<RuntimeInputAction> =
                serde_json::from_str(value).map_err(|e| e.to_string())?;
            let mut candidate = settings.clone();
            candidate.input_actions = actions;
            candidate.validate_input()?;
            settings.input_actions = candidate.input_actions;
        }
        "input_axes" => {
            let axes: Vec<RuntimeInputAxis> =
                serde_json::from_str(value).map_err(|e| e.to_string())?;
            let mut candidate = settings.clone();
            candidate.input_axes = axes;
            candidate.validate_input()?;
            settings.input_axes = candidate.input_axes;
        }
        "progress_schema" | "progress_slot_kib" | "progress_slot_count" | "autosave_slot" => {
            let mut policy = settings.progress.clone();
            let integer = || {
                value
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "runtime.expected_positive_integer".to_string())
            };
            match field {
                "progress_schema" => policy.schema_version = integer()?,
                "progress_slot_kib" => {
                    policy.max_slot_bytes =
                        integer()?.checked_mul(1024).ok_or("runtime.invalid_size")?
                }
                "progress_slot_count" => policy.max_slots = integer()?,
                "autosave_slot" => policy.autosave_slot = value.trim().into(),
                _ => unreachable!(),
            }
            policy.validate().map_err(str::to_owned)?;
            settings.progress = policy;
        }
        _ => return Err("runtime.unknown_setting".into()),
    }
    Ok(())
}
