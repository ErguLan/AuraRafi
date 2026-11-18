//! Before/after visual evidence comparison for authoring turns.

use super::world::active_project_root;
use super::*;

const MAX_COMPARE_PIXELS: u64 = 64 * 1024 * 1024;

fn relative_project_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn resolve_compare_path(root: &Path, raw: &str, label: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve the project root: {error}"))?;
    let candidate = Path::new(raw);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let candidate = candidate
        .canonicalize()
        .map_err(|error| format!("Could not resolve {label} image '{raw}': {error}"))?;
    if !candidate.starts_with(&root) {
        return Err(format!(
            "{label} image must live inside the active project."
        ));
    }
    Ok(candidate)
}

fn load_compare_image(path: &Path, label: &str) -> Result<image::RgbaImage, String> {
    let image = image::open(path)
        .map_err(|error| format!("Could not decode {label} image '{}': {error}", path.display()))?;
    let rgba = image.to_rgba8();
    let pixels = u64::from(rgba.width()) * u64::from(rgba.height());
    if pixels == 0 {
        return Err(format!("{label} image has no pixels."));
    }
    if pixels > MAX_COMPARE_PIXELS {
        return Err(format!(
            "{label} image is {pixels} pixels; visual diff is limited to {MAX_COMPARE_PIXELS}."
        ));
    }
    Ok(rgba)
}

fn save_flag(request: &ParsedCommand) -> bool {
    if let Some(value) = request.structured_arg("save").and_then(Value::as_bool) {
        return value;
    }
    request
        .arg("save")
        .map(|value| !matches!(value.to_ascii_lowercase().as_str(), "false" | "0" | "no" | "off"))
        .unwrap_or(true)
}

pub(crate) fn execute_visual_diff(request: &ParsedCommand) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let root = active_project_root()
            .ok_or_else(|| "scene.visual_diff requires an active project root.".to_string())?;
        let before_arg = text_argument(request, "before")
            .or_else(|| text_argument(request, "base"))
            .ok_or_else(|| "scene.visual_diff requires before=<project image>.".to_string())?;
        let after_arg = text_argument(request, "after")
            .or_else(|| text_argument(request, "current"))
            .ok_or_else(|| "scene.visual_diff requires after=<project image>.".to_string())?;
        let threshold = request
            .structured_arg("threshold")
            .and_then(Value::as_f64)
            .or_else(|| {
                request
                    .arg("threshold")
                    .and_then(|raw| raw.parse::<f64>().ok())
            })
            .unwrap_or(8.0)
            .clamp(0.0, 255.0);
        let before_path = resolve_compare_path(&root, &before_arg, "before")?;
        let after_path = resolve_compare_path(&root, &after_arg, "after")?;
        let before = load_compare_image(&before_path, "before")?;
        let after = load_compare_image(&after_path, "after")?;
        if before.dimensions() != after.dimensions() {
            return Err(format!(
                "Visual diff requires matching dimensions: before is {}x{}, after is {}x{}.",
                before.width(),
                before.height(),
                after.width(),
                after.height()
            ));
        }

        let width = before.width();
        let height = before.height();
        let mut diff = image::RgbaImage::new(width, height);
        let mut changed_pixels = 0u64;
        let mut delta_sum = 0u128;
        let mut max_delta = 0u8;
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for y in 0..height {
            for x in 0..width {
                let left = before.get_pixel(x, y);
                let right = after.get_pixel(x, y);
                let mut pixel_delta = 0u8;
                for channel in 0..4 {
                    let delta = left[channel].abs_diff(right[channel]);
                    pixel_delta = pixel_delta.max(delta);
                    delta_sum += u128::from(delta);
                }
                max_delta = max_delta.max(pixel_delta);
                if f64::from(pixel_delta) > threshold {
                    changed_pixels += 1;
                    bounds = Some(match bounds {
                        Some((min_x, min_y, max_x, max_y)) => (
                            min_x.min(x),
                            min_y.min(y),
                            max_x.max(x),
                            max_y.max(y),
                        ),
                        None => (x, y, x, y),
                    });
                    diff.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                } else {
                    let luma = (0.299 * f32::from(right[0])
                        + 0.587 * f32::from(right[1])
                        + 0.114 * f32::from(right[2]))
                        * 0.35;
                    let value = luma.clamp(0.0, 255.0) as u8;
                    diff.put_pixel(x, y, image::Rgba([value, value, value, 255]));
                }
            }
        }
        let total_pixels = u64::from(width) * u64::from(height);
        let changed_ratio = changed_pixels as f64 / total_pixels as f64;
        let mean_delta = delta_sum as f64 / (total_pixels as f64 * 4.0);

        let mut artifact = Value::Null;
        if save_flag(request) {
            let requested = text_argument(request, "output");
            let output_path = if let Some(raw) = requested {
                let candidate = Path::new(&raw);
                let candidate = if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else {
                    root.join(candidate)
                };
                candidate.with_extension("png")
            } else {
                world_artifact_directory(&root).join(format!(
                    "visual-diff-{}.png",
                    Uuid::new_v4().simple()
                ))
            };
            let canonical_root = root
                .canonicalize()
                .map_err(|error| format!("Could not resolve the project root: {error}"))?;
            let output_path = if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("Could not create the diff directory: {error}"))?;
                parent
                    .canonicalize()
                    .map(|parent| parent.join(output_path.file_name().unwrap_or_default()))
                    .map_err(|error| format!("Could not resolve the diff directory: {error}"))?
            } else {
                output_path
            };
            if !output_path.starts_with(&canonical_root) {
                return Err("Visual diff output must live inside the active project.".to_string());
            }
            diff.save(&output_path)
                .map_err(|error| format!("Could not save the visual diff: {error}"))?;
            artifact = serde_json::json!({
                "kind": "image/png",
                "uri": relative_project_path(&canonical_root, &output_path),
                "width": width,
                "height": height,
                "caption": format!("Visual diff: {changed_pixels} changed pixel(s)"),
            });
        }

        let bounds_value = bounds
            .map(|(min_x, min_y, max_x, max_y)| serde_json::json!([min_x, min_y, max_x, max_y]))
            .unwrap_or(Value::Null);
        let verdict = if changed_pixels == 0 {
            "identical"
        } else {
            "changed"
        };
        Ok(CommandOutput::info(
            "Visual diff",
            vec![
                format!("changed: {changed_pixels}/{total_pixels} pixel(s)"),
                format!("mean delta: {mean_delta:.3}"),
                format!("verdict: {verdict}"),
            ],
            serde_json::json!({
                "ok": true,
                "verdict": verdict,
                "threshold": threshold,
                "before": {
                    "path": relative_project_path(&root, &before_path),
                    "width": width,
                    "height": height,
                },
                "after": {
                    "path": relative_project_path(&root, &after_path),
                    "width": width,
                    "height": height,
                },
                "changed_pixels": changed_pixels,
                "total_pixels": total_pixels,
                "changed_ratio": changed_ratio,
                "mean_delta": mean_delta,
                "max_delta": max_delta,
                "bounds": bounds_value,
                "artifact": artifact,
            }),
        ))
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Visual diff", error))
}

fn world_artifact_directory(root: &Path) -> PathBuf {
    root.join(".aura_rafi").join("agent_artifacts")
}

#[cfg(test)]
mod tests {
    use super::super::parsed_command_from_params;
    use super::*;

    #[test]
    fn visual_diff_reports_changed_pixels_and_writes_an_artifact() {
        let root = std::env::temp_dir().join(format!(
            "raf-visual-diff-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let artifacts = root.join(".aura_rafi/agent_artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let _guard = crate::commands::game::bind_project_root(Some(root.clone()));
        let before = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        let mut after = before.clone();
        after.put_pixel(1, 1, image::Rgba([250, 0, 0, 255]));
        before.save(artifacts.join("before.png")).unwrap();
        after.save(artifacts.join("after.png")).unwrap();

        let output = execute_visual_diff(&parsed_command_from_params(
            "scene.visual_diff",
            serde_json::json!({
                "before": ".aura_rafi/agent_artifacts/before.png",
                "after": ".aura_rafi/agent_artifacts/after.png",
                "threshold": 8
            }),
        ));
        assert!(
            !matches!(
                output.level,
                crate::commands::output::CommandLevel::Error
            ),
            "{:?}",
            output.lines
        );
        assert_eq!(output.json["verdict"], "changed");
        assert_eq!(output.json["changed_pixels"], 1);
        assert_eq!(output.json["total_pixels"], 4);
        assert_eq!(output.json["bounds"], serde_json::json!([1, 1, 1, 1]));
        let artifact = output.json["artifact"]["uri"].as_str().unwrap();
        assert!(artifact.starts_with(".aura_rafi/agent_artifacts/visual-diff-"));
        assert!(root.join(artifact).exists());

        let identical = execute_visual_diff(&parsed_command_from_params(
            "scene.visual_diff",
            serde_json::json!({
                "before": ".aura_rafi/agent_artifacts/before.png",
                "after": ".aura_rafi/agent_artifacts/before.png",
                "save": false
            }),
        ));
        assert_eq!(identical.json["verdict"], "identical");
        assert_eq!(identical.json["changed_pixels"], 0);
        assert_eq!(identical.json["bounds"], Value::Null);
        assert_eq!(identical.json["artifact"], Value::Null);
        let _ = std::fs::remove_dir_all(root);
    }
}
