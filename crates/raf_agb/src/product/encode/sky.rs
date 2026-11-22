use crate::product::SkyBackdrop;
use crate::{Batch, DrawMode, Error, Result, Vertex};

use super::Encoder;

const SKY_KIND: u32 = 3;

impl Encoder<'_> {
    pub(super) fn sky(&mut self, sky: SkyBackdrop) -> Result<()> {
        if !self.workload.batches.is_empty() {
            return Err(Error::Invalid("draw sky before scene geometry and paint"));
        }
        if [sky.zenith, sky.horizon, sky.ground]
            .into_iter()
            .any(|color| {
                color[..3].iter().any(|value| !(0. ..=4.).contains(value)) || color[3] != 1.0
            })
            || !(0. ..=1.).contains(&sky.haze)
            || !(-1. ..=1.).contains(&sky.sun_disk_cos)
            || !(-1. ..=1.).contains(&sky.halo_cos)
            || !(0. ..=1.).contains(&sky.halo)
            || !(0. ..=1.).contains(&sky.stars_density)
            || !(0. ..=4.).contains(&sky.stars_intensity)
        {
            return Err(Error::Invalid("invalid sky lighting parameters"));
        }
        let texture = sky.texture.map(|index| self.texture(index)).transpose()?;
        let triangle = [[-1., -1.], [3., -1.], [-1., 3.]].map(|[x, y]| Vertex {
            position: [x, y, 0.],
            uv: [0.; 2],
            rgba: [1.; 4],
            normal: [0.; 3],
        });
        let vertices = self.add_vertices(&triangle)?;
        // The sky batch reuses the instance stream. The columns are payload
        // rather than a transform when the shader sees SKY_KIND.
        let index = self.raster_instance(
            SKY_KIND,
            [
                sky.stars_density,
                sky.stars_intensity,
                f32::from(texture.is_some()),
                0.0,
            ],
        )?;
        self.workload.instances[index as usize].transform = [
            sky.zenith,
            sky.horizon,
            sky.ground,
            [sky.haze, sky.sun_disk_cos, sky.halo, sky.halo_cos],
        ];
        self.batch(Batch {
            mode: DrawMode::SceneNoDepth,
            vertices,
            indices: None,
            instances: index..index + 1,
            texture: texture.unwrap_or(self.white),
        })
    }
}
