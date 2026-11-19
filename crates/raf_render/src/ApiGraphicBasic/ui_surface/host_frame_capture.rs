//! RGBA frame capture for a headless ApiGraphicBasic presentation host.
//!
//! Windowed hosts present through their swapchain; headless hosts read back
//! their owned target through the reusable staging path in `headless.rs`.

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
        let Some(target) = self.headless.as_mut() else {
            return Err(
                "frame capture requires a headless host; a windowed host owns a swapchain image"
                    .to_string(),
            );
        };
        let [width, height] = target.capture_rgba_into(&self.device, &self.queue, out)?;
        Ok((width, height))
    }
}
