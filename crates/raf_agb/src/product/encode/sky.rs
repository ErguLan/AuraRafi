use glam::Vec3;

use crate::product::SkyBackdrop;
use crate::{Batch, DrawMode, Error, Result, Vertex};

use super::{stream_range, validate_color, Encoder};

const MERIDIANS: u32 = 32;
const PARALLELS: u32 = 16;
const SKY_KIND: u32 = 3;

impl Encoder<'_> {
    pub(super) fn sky(&mut self, sky: SkyBackdrop) -> Result<()> {
        if !self.workload.batches.is_empty() {
            return Err(Error::Invalid("draw sky before scene geometry and paint"));
        }
        for color in [sky.zenith, sky.horizon, sky.ground] {
            validate_color(color)?;
        }
        if !sky.haze.is_finite() || !(0. ..=1.).contains(&sky.haze) {
            return Err(Error::Invalid("invalid environment haze"));
        }
        let tex = sky
            .texture
            .map(|texture| self.texture(texture))
            .transpose()?;
        let pad = tex.map_or(0., |texture| {
            0.5 / self.workload.textures[texture as usize].size[1].max(1) as f32
        });
        let rows = PARALLELS + 1;
        let cols = MERIDIANS + 1;
        let vertices = self.vertex_range((rows * cols) as usize)?;
        let indices = stream_range(
            self.workload.indices.len(),
            (PARALLELS * MERIDIANS * 6) as usize,
        )?;
        self.reserve(indices.len() as u64 * size_of::<u32>() as u64)?;
        for y in 0..rows {
            let v = y as f32 / PARALLELS as f32;
            let (ring, vertical) = (std::f32::consts::PI * v).sin_cos();
            let target = if vertical >= 0. {
                sky.zenith
            } else {
                sky.ground
            };
            let blend = vertical.abs().powf(1. + 2. * sky.haze);
            let rgba = if tex.is_some() {
                [1.; 4]
            } else {
                std::array::from_fn(|i| sky.horizon[i] + (target[i] - sky.horizon[i]) * blend)
            };
            for x in 0..cols {
                let u = x as f32 / MERIDIANS as f32;
                let (sin, cos) = (std::f32::consts::TAU * u).sin_cos();
                let position = Vec3::new(cos * ring, vertical, sin * ring);
                self.workload.vertices.push(Vertex {
                    position: position.to_array(),
                    uv: [u, v.clamp(pad, 1. - pad)],
                    rgba,
                    normal: [0.; 3],
                });
                if x < MERIDIANS && y < PARALLELS {
                    let a = vertices.start + y * cols + x;
                    let b = a + cols;
                    self.workload.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
                }
            }
        }
        let instance = self.raster_instance(SKY_KIND, [1.; 4])?;
        self.batch(Batch {
            mode: DrawMode::SceneNoDepth,
            vertices,
            indices: Some(indices),
            instances: instance..instance + 1,
            texture: tex.unwrap_or(self.white),
        })
    }
}
