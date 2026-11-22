//! RGBA frame capture for a headless ApiGraphicBasic presentation host.
//!
//! Windowed hosts present through their native surface; headless hosts read the
//! last composed product frame back through their owned `ProductDevice`. The
//! call is blocking and belongs in tooling, away from the interactive loop.

use super::NativeUiWindowHost;
use crate::api_graphic_basic::device::SceneFrameCapture;

impl NativeUiWindowHost {
    /// Captures the last composed headless frame as tightly packed RGBA8.
    pub fn capture_frame_rgba(&mut self) -> Result<SceneFrameCapture, String> {
        let mut rgba8 = Vec::new();
        let (width, height) = self.capture_frame_rgba_into(&mut rgba8)?;
        Ok(SceneFrameCapture {
            width,
            height,
            rgba8,
        })
    }

    /// Captures the last composed headless frame into `out`, reusing its
    /// capacity. Returns the captured width and height.
    pub fn capture_frame_rgba_into(&mut self, out: &mut Vec<u8>) -> Result<(u32, u32), String> {
        if !self.is_headless() {
            return Err(
                "frame capture requires a headless host; a windowed host owns its native surface"
                    .to_string(),
            );
        }
        let [width, height] = self.extent;
        let pixels = self
            .device
            .capture()
            .map_err(|error| format!("headless frame capture failed: {error}"))?;
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(format!(
                "headless frame capture returned {} bytes; expected {expected} for {width}x{height} RGBA",
                pixels.len()
            ));
        }
        out.clear();
        out.extend_from_slice(&pixels);
        Ok((width, height))
    }
}
