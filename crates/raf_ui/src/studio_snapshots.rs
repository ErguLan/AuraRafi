//! Deterministic structural and pixel snapshot helpers for RafUI Studio.

use serde::{Deserialize, Serialize};

use crate::{UiColorMode, UiDocument, UiEnvironment};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioSnapshotCase {
    pub name: String,
    pub logical_size: [u32; 2],
    pub scale_factor_bits: u32,
    pub color_mode: UiColorMode,
    pub structure_fingerprint: u64,
}

impl UiStudioSnapshotCase {
    pub fn from_document(
        name: impl Into<String>,
        document: &UiDocument,
        environment: UiEnvironment,
    ) -> Self {
        let serialized = serde_json::to_vec(document).unwrap_or_default();
        Self {
            name: name.into(),
            logical_size: [environment.width() as u32, environment.height() as u32],
            scale_factor_bits: environment.scale_factor.to_bits(),
            color_mode: environment.color_mode,
            structure_fingerprint: stable_hash(&serialized),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioGoldenSnapshot {
    pub name: String,
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
    pub fingerprint: u64,
}

impl UiStudioGoldenSnapshot {
    pub fn from_rgba(
        name: impl Into<String>,
        size: [u32; 2],
        pixels: Vec<u8>,
    ) -> Result<Self, String> {
        expected_len(size, pixels.len())?;
        Ok(Self {
            name: name.into(),
            size,
            fingerprint: stable_hash(&pixels),
            pixels,
        })
    }

    pub fn compare(&self, actual: &[u8]) -> Result<UiStudioSnapshotResult, String> {
        expected_len(self.size, actual.len())?;
        Ok(UiStudioSnapshotResult {
            matches: self.pixels == actual,
            diff: UiStudioPixelDiff::between(&self.pixels, actual),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioPixelDiff {
    pub compared_pixels: u64,
    pub different_pixels: u64,
    pub max_channel_delta: u8,
    pub total_channel_delta: u64,
}

impl UiStudioPixelDiff {
    pub fn between(expected: &[u8], actual: &[u8]) -> Self {
        let compared_bytes = expected.len().min(actual.len());
        let mut different_pixels = 0;
        let mut max_channel_delta = 0;
        let mut total_channel_delta = 0_u64;
        for index in (0..compared_bytes).step_by(4) {
            let mut pixel_different = false;
            for channel in 0..4 {
                let left = expected[index + channel];
                let right = actual[index + channel];
                let delta = left.abs_diff(right);
                max_channel_delta = max_channel_delta.max(delta);
                total_channel_delta += u64::from(delta);
                pixel_different |= delta != 0;
            }
            if pixel_different {
                different_pixels += 1;
            }
        }
        Self {
            compared_pixels: (compared_bytes / 4) as u64,
            different_pixels,
            max_channel_delta,
            total_channel_delta,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioSnapshotResult {
    pub matches: bool,
    pub diff: UiStudioPixelDiff,
}

fn expected_len(size: [u32; 2], actual: usize) -> Result<(), String> {
    let expected = usize::try_from(size[0])
        .ok()
        .and_then(|width| {
            usize::try_from(size[1])
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .unwrap_or(0);
    if actual != expected {
        return Err(format!(
            "RGBA snapshot has {actual} bytes; expected {expected} for {}x{}.",
            size[0], size[1]
        ));
    }
    Ok(())
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_snapshot_reports_real_differences() {
        let golden =
            UiStudioGoldenSnapshot::from_rgba("toolbar", [1, 1], vec![0, 0, 0, 255]).unwrap();
        let result = golden.compare(&[0, 1, 0, 255]).unwrap();
        assert!(!result.matches);
        assert_eq!(result.diff.different_pixels, 1);
        assert_eq!(result.diff.max_channel_delta, 1);
    }

    #[test]
    fn structural_snapshot_is_deterministic() {
        let document = UiDocument::blank("snapshot");
        let env = UiEnvironment::new(320, 200);
        let first = UiStudioSnapshotCase::from_document("a", &document, env);
        let second = UiStudioSnapshotCase::from_document("a", &document, env);
        assert_eq!(first, second);
    }
}
