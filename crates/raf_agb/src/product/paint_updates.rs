//! Rectangular texture updates for retained atlases, icons and images.
//!
//! A text atlas uploads only the rectangles that changed. [`TexturePatch`]
//! validates one sub-rectangle against the full texture before it becomes a
//! [`ResourcePatch`], so a glyph or an icon can be refreshed without replacing
//! the texture or copying the whole page through the staging budget.

use crate::{Error, ResourcePatch, Result};

/// One packed sub-rectangle of a prepared texture.
#[derive(Clone, Copy, Debug)]
pub struct TexturePatch<'a> {
    /// Packet texture index, excluding the implicit white texture.
    pub texture: u32,
    /// Top-left texel of the rectangle inside the texture.
    pub origin: [u32; 2],
    /// Rectangle size in texels. Both dimensions must be non-zero.
    pub size: [u32; 2],
    /// Tightly packed straight-alpha RGBA rows for `size`.
    pub rgba: &'a [u8],
}

impl<'a> TexturePatch<'a> {
    pub const fn new(
        texture: u32,
        origin: [u32; 2],
        size: [u32; 2],
        rgba: &'a [u8],
    ) -> Self {
        Self {
            texture,
            origin,
            size,
            rgba,
        }
    }

    /// Validates the rectangle against the texture extent. `extent` is the full
    /// texture size, not the patched size.
    pub fn validate(&self, extent: [u32; 2]) -> Result<()> {
        let [width, height] = self.size;
        let [x, y] = self.origin;
        let [extent_x, extent_y] = extent;
        if width == 0 || height == 0 {
            return Err(Error::Invalid("empty texture patch"));
        }
        let right = u64::from(x) + u64::from(width);
        let bottom = u64::from(y) + u64::from(height);
        if right > u64::from(extent_x) || bottom > u64::from(extent_y) {
            return Err(Error::Invalid("texture patch outside the texture"));
        }
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(Error::Invalid("texture patch size overflows its budget"))?;
        if self.rgba.len() as u64 != expected {
            return Err(Error::Invalid(
                "texture patch rows do not match its size",
            ));
        }
        Ok(())
    }

    /// Converts the validated rectangle into the device patch it describes.
    pub fn into_resource_patch(self, extent: [u32; 2]) -> Result<ResourcePatch<'a>> {
        self.validate(extent)?;
        Ok(ResourcePatch::Texture {
            index: self.texture,
            origin: self.origin,
            size: self.size,
            rgba: self.rgba,
        })
    }
}
