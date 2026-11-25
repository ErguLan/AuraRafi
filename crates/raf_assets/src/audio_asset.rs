//! Bounded WAV metadata and waveform peaks for the Assets browser.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const MAX_AUDIO_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const WAVEFORM_BUCKETS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAssetMeta {
    pub version: u32,
    pub path: String,
    pub bytes: u64,
    pub duration_seconds: f32,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub waveform: Vec<f32>,
}

impl AudioAssetMeta {
    pub const VERSION: u32 = 1;

    /// Read PCM WAV metadata. Compressed or malformed files return `None`.
    pub fn read_for_asset_file(path: &Path) -> Option<Self> {
        let bytes = read_bounded(path).ok()?;
        parse_wav(path, &bytes)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != Self::VERSION {
            return Err("Unsupported audio metadata version.".into());
        }
        if !self.duration_seconds.is_finite()
            || self.duration_seconds < 0.0
            || self.duration_seconds > 86_400.0
            || self.channels == 0
            || self.channels > 2
            || self.sample_rate < 8_000
            || self.sample_rate > 192_000
            || self.bits_per_sample != 16
            || self.waveform.len() > WAVEFORM_BUCKETS
            || self
                .waveform
                .iter()
                .any(|peak| !peak.is_finite() || *peak < 0.0 || *peak > 1.0)
        {
            return Err("Audio metadata is outside the supported WAV contract.".into());
        }
        Ok(())
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_AUDIO_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_AUDIO_FILE_BYTES {
        return Err("Audio file exceeds the 32 MiB metadata limit.".into());
    }
    Ok(bytes)
}

fn parse_wav(path: &Path, bytes: &[u8]) -> Option<AudioAssetMeta> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut byte_rate = 0u32;
    let mut block_align = 0u16;
    let mut data: Option<&[u8]> = None;
    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let start = offset + 8;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        match &bytes[offset..offset + 4] {
            b"fmt " if size >= 16 => {
                channels = u16::from_le_bytes(bytes[start..start + 2].try_into().ok()?);
                sample_rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().ok()?);
                byte_rate = u32::from_le_bytes(bytes[start + 8..start + 12].try_into().ok()?);
                block_align = u16::from_le_bytes(bytes[start + 12..start + 14].try_into().ok()?);
                bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().ok()?);
            }
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        offset = end + (size & 1);
    }
    let data = data?;
    if channels == 0
        || channels > 2
        || sample_rate < 8_000
        || sample_rate > 192_000
        || bits != 16
        || block_align != channels * 2
        || byte_rate != sample_rate * block_align as u32
        || data.is_empty()
        || data.len() % block_align as usize != 0
    {
        return None;
    }
    let frames = data.len() / block_align as usize;
    let duration_seconds = frames as f32 / sample_rate as f32;
    let samples = data
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
        .collect::<Vec<_>>();
    let frames_per_bucket = (frames / WAVEFORM_BUCKETS).max(1);
    let mut waveform = Vec::with_capacity(WAVEFORM_BUCKETS);
    for bucket in 0..WAVEFORM_BUCKETS {
        let start = bucket * frames_per_bucket * channels as usize;
        if start >= samples.len() {
            break;
        }
        let end = ((bucket + 1) * frames_per_bucket * channels as usize).min(samples.len());
        let peak = samples[start..end]
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
            .clamp(0.0, 1.0);
        waveform.push(peak);
    }
    let meta = AudioAssetMeta {
        version: AudioAssetMeta::VERSION,
        path: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio.wav")
            .to_string(),
        bytes: bytes.len() as u64,
        duration_seconds,
        channels,
        sample_rate,
        bits_per_sample: bits,
        waveform,
    };
    meta.validate().ok()?;
    Some(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn wav_bytes() -> Vec<u8> {
        let samples = [0i16, 16384, -16384, 0, 8192, -8192, 0, 0];
        let data = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
        bytes
    }

    #[test]
    fn reads_wav_metadata_and_waveform() {
        let path = std::env::temp_dir().join(format!(
            "raf-audio-meta-{}.wav",
            uuid::Uuid::new_v4()
        ));
        let mut file = File::create(&path).unwrap();
        file.write_all(&wav_bytes()).unwrap();
        let meta = AudioAssetMeta::read_for_asset_file(&path).unwrap();
        assert_eq!(meta.channels, 1);
        assert_eq!(meta.sample_rate, 8000);
        assert!((meta.duration_seconds - 0.001).abs() < 0.0001);
        assert!(!meta.waveform.is_empty());
        assert!(meta.waveform.iter().all(|peak| (0.0..=1.0).contains(peak)));
        let _ = std::fs::remove_file(path);
    }
}
