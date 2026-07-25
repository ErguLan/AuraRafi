//! Backend-neutral resource handles owned by ApiGraphicBasic.
//!
//! Backends may store completely different native objects behind these ids.
//! Upper layers must use handles instead of WGPU, Vulkan, DX12, or Metal
//! resource pointers.

use serde::{Deserialize, Serialize};

macro_rules! define_handle {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name {
            index: u32,
            generation: u32,
        }

        impl $name {
            pub const INVALID: Self = Self {
                index: u32::MAX,
                generation: 0,
            };

            pub const fn new(index: u32, generation: u32) -> Self {
                Self { index, generation }
            }

            pub const fn index(self) -> u32 {
                self.index
            }

            pub const fn generation(self) -> u32 {
                self.generation
            }

            pub const fn is_valid(self) -> bool {
                self.index != u32::MAX && self.generation != 0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::INVALID
            }
        }
    };
}

define_handle!(BufferHandle, "A backend-neutral GPU buffer handle.");
define_handle!(TextureHandle, "A backend-neutral GPU texture handle.");
define_handle!(SamplerHandle, "A backend-neutral sampler handle.");
define_handle!(
    PipelineHandle,
    "A backend-neutral graphics or compute pipeline handle."
);
define_handle!(MeshHandle, "A backend-neutral mesh residency handle.");
define_handle!(
    MaterialHandle,
    "A backend-neutral material residency handle."
);
define_handle!(
    SurfaceHandle,
    "A backend-neutral presentation surface handle."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_invalid_until_they_have_a_generation() {
        assert!(!TextureHandle::default().is_valid());
        assert!(TextureHandle::new(4, 1).is_valid());
        assert_eq!(TextureHandle::new(4, 7).index(), 4);
        assert_eq!(TextureHandle::new(4, 7).generation(), 7);
    }

    #[test]
    fn handle_types_keep_generation_identity() {
        let texture = TextureHandle::new(1, 1);
        let buffer = BufferHandle::new(1, 1);
        assert_eq!(texture.index(), buffer.index());
        assert_ne!(texture, TextureHandle::new(1, 2));
    }
}
