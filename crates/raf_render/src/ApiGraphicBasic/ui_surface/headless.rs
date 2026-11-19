//! Owned headless render target for ApiGraphicBasic presentation hosts.
//!
//! A headless host renders into this texture instead of a swapchain image. The
//! target stays inside ApiGraphicBasic: no window, event loop, or second
//! renderer path exists. Frame readback reuses one row-aligned staging buffer
//! and one mapped callback cell so repeated captures do not allocate per frame.

use std::sync::{Arc, Mutex};

/// Largest axis accepted by `NativeUiWindowHost::create_headless`.
pub(super) const HEADLESS_MAX_AXIS: u32 = 3840;
/// Largest frame accepted by the RGBA readback path.
pub(super) const HEADLESS_CAPTURE_LIMIT: [u32; 2] = [3840, 2160];
/// WebGPU requires copy rows to start on this byte boundary.
const COPY_ROW_ALIGNMENT: u32 = 256;

/// Validates a headless target size. Rendering is bounded to keep the owned
/// texture, staging buffer, and capture vector inside a potato-friendly budget.
pub(super) fn validate_size(size: [u32; 2]) -> Result<[u32; 2], String> {
    if size[0] == 0 || size[1] == 0 || size[0] > HEADLESS_MAX_AXIS || size[1] > HEADLESS_MAX_AXIS {
        return Err(format!(
            "headless render size must be 1..={HEADLESS_MAX_AXIS} on each axis, received {}x{}",
            size[0], size[1]
        ));
    }
    Ok(size)
}

/// Owned texture/view pair used by a headless host as its presentation target.
pub(super) struct HeadlessSurface {
    format: wgpu::TextureFormat,
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    capture: Option<HeadlessCapture>,
}

impl HeadlessSurface {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) -> Result<Self, String> {
        let size = validate_size(size)?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.HeadlessTarget"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            format,
            size,
            texture,
            view,
            capture: None,
        })
    }

    pub(super) fn size(&self) -> [u32; 2] {
        self.size
    }

    pub(super) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Recreates the target when the requested bounded size changes. The
    /// capture staging buffer is invalidated with the old texture.
    pub(super) fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<(), String> {
        if self.size == size {
            return Ok(());
        }
        *self = Self::new(device, self.format, size)?;
        Ok(())
    }

    /// Copies the current target into `out` as tightly packed RGBA8 rows.
    ///
    /// The staging buffer is created once per size, reused for every capture,
    /// unmapped afterwards, and bounded by `HEADLESS_CAPTURE_LIMIT`.
    pub(super) fn capture_rgba_into(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        out: &mut Vec<u8>,
    ) -> Result<[u32; 2], String> {
        let size = self.size;
        if size[0] > HEADLESS_CAPTURE_LIMIT[0] || size[1] > HEADLESS_CAPTURE_LIMIT[1] {
            return Err(format!(
                "headless frame capture is limited to {}x{}, received {}x{}",
                HEADLESS_CAPTURE_LIMIT[0], HEADLESS_CAPTURE_LIMIT[1], size[0], size[1]
            ));
        }
        if self
            .capture
            .as_ref()
            .is_none_or(|capture| capture.size != size)
        {
            self.capture = Some(HeadlessCapture::new(device, size));
        }
        let capture = self
            .capture
            .as_mut()
            .expect("headless capture staging exists for the current size");

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.HeadlessCaptureEncoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &capture.buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(capture.padded_bytes_per_row),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        let completion = Arc::clone(&capture.completion);
        capture
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                *completion
                    .lock()
                    .expect("headless frame capture callback lock") = Some(result);
            });
        let _ = device.poll(wgpu::Maintain::Wait);
        let mapped_result = capture
            .completion
            .lock()
            .expect("headless frame capture result lock")
            .take()
            .ok_or_else(|| "The headless frame capture callback did not complete.".to_string())?;
        mapped_result
            .map_err(|error| format!("The headless frame capture readback failed: {error}"))?;

        let row_bytes = size[0] as usize * 4;
        let padded_row_bytes = capture.padded_bytes_per_row as usize;
        let output_len = row_bytes
            .checked_mul(size[1] as usize)
            .ok_or_else(|| "The headless frame capture size overflowed.".to_string())?;
        out.clear();
        out.reserve(output_len);
        {
            let mapped = capture.buffer.slice(..).get_mapped_range();
            for row in mapped.chunks_exact(padded_row_bytes).take(size[1] as usize) {
                out.extend_from_slice(&row[..row_bytes]);
            }
        }
        capture.buffer.unmap();
        Ok(size)
    }
}

struct HeadlessCapture {
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
    size: [u32; 2],
    completion: Arc<Mutex<Option<Result<(), wgpu::BufferAsyncError>>>>,
}

impl HeadlessCapture {
    fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let padded_bytes_per_row = align_to_row(size[0].saturating_mul(4));
        let length = u64::from(padded_bytes_per_row) * u64::from(size[1]);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.HeadlessCaptureStaging"),
            size: length.max(u64::from(COPY_ROW_ALIGNMENT)),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            padded_bytes_per_row,
            size,
            completion: Arc::new(Mutex::new(None)),
        }
    }
}

fn align_to_row(bytes_per_row: u32) -> u32 {
    bytes_per_row.div_ceil(COPY_ROW_ALIGNMENT) * COPY_ROW_ALIGNMENT
}
