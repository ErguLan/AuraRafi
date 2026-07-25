//! Resource cache for raster images referenced by retained UI documents.
//!
//! Documents store only `UiImageSource` keys. This cache owns decoded pixels
//! at the surface-host boundary, so a project can replace or unload icons
//! without modifying serialized layout data.

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct UiSurfaceImageData {
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
    pub revision: u64,
}

#[derive(Debug, Clone, Default)]
pub struct UiSurfaceImageStore {
    images: BTreeMap<String, UiSurfaceImageData>,
    next_revision: u64,
}

impl UiSurfaceImageStore {
    pub fn insert_rgba(
        &mut self,
        key: impl Into<String>,
        size: [u32; 2],
        pixels: Vec<u8>,
    ) -> Result<(), String> {
        let width = size[0].max(1);
        let height = size[1].max(1);
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(format!(
                "UI image has {} bytes; expected {} for {}x{} RGBA.",
                pixels.len(),
                expected,
                width,
                height
            ));
        }
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.images.insert(
            key.into(),
            UiSurfaceImageData {
                size: [width, height],
                pixels,
                revision: self.next_revision,
            },
        );
        Ok(())
    }

    pub fn load_png(&mut self, key: impl Into<String>, path: &Path) -> Result<(), String> {
        let image = image::open(path)
            .map_err(|error| format!("Unable to read UI image '{}': {error}", path.display()))?
            .to_rgba8();
        self.insert_rgba(key, [image.width(), image.height()], image.into_raw())
    }

    pub fn remove(&mut self, key: &str) -> Option<UiSurfaceImageData> {
        self.images.remove(key)
    }

    pub fn get(&self, key: &str) -> Option<&UiSurfaceImageData> {
        self.images.get(key)
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_images_require_exact_pixel_storage() {
        let mut store = UiSurfaceImageStore::default();
        assert!(store.insert_rgba("icon", [2, 2], vec![255; 16]).is_ok());
        assert!(store.insert_rgba("bad", [2, 2], vec![255; 12]).is_err());
        assert_eq!(store.get("icon").unwrap().size, [2, 2]);
    }
}
