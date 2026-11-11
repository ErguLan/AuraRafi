//! Semantic icon contracts for retained RafUI surfaces.
//!
//! Surface builders refer to an icon by stable meaning rather than by a PNG
//! path. ApiGraphicBasic owns resolving that meaning into vector or bitmap
//! resources at the requested physical density.

use serde::{Deserialize, Serialize};

/// Static source and lookup metadata for one semantic icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiIconDescriptor {
    pub id: UiIconId,
    /// Stable kebab-case resource key.
    pub key: &'static str,
    /// Existing project-relative SVG master path.
    pub master_svg: &'static str,
    /// Width and height of the bundled runtime PNG in pixels.
    pub source_raster_size: [u16; 2],
    /// Width and height of the SVG master's design coordinate canvas.
    pub source_canvas_size: [u16; 2],
    /// Declared default logical display size, matching `UiIconSize::Toolbar`.
    /// This is not a measured or per-icon optical correction.
    pub optical_size_px: u16,
    /// Intentional lookup aliases, including the stable serde variant spelling.
    pub aliases: &'static [&'static str],
}

macro_rules! declare_icon_family {
    ($($variant:ident => {
        key: $key:literal,
        master_svg: $master_svg:literal,
        source_raster_size: [$raster_width:literal, $raster_height:literal],
        source_canvas_size: [$canvas_width:literal, $canvas_height:literal],
        optical_size_px: $optical_size_px:literal,
        aliases: [$($alias:literal),*],
    },)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum UiIconId { $($variant,)* }
        impl UiIconId {
            pub const ALL: [Self; [$(stringify!($variant)),*].len()] = [$(Self::$variant,)*];
            pub const fn descriptor(self) -> UiIconDescriptor {
                match self {
                    $(Self::$variant => UiIconDescriptor {
                        id: Self::$variant,
                        key: $key,
                        master_svg: $master_svg,
                        source_raster_size: [$raster_width, $raster_height],
                        source_canvas_size: [$canvas_width, $canvas_height],
                        optical_size_px: $optical_size_px,
                        aliases: &[$($alias),*],
                    },)*
                }
            }
            pub const fn key(self) -> &'static str { self.descriptor().key }
            pub fn from_alias_key(key: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|id| id.descriptor().aliases.contains(&key))
            }
            pub fn from_key(key: &str) -> Option<Self> {
                match key {
                    $($key => Some(Self::$variant),)*
                    _ => Self::from_alias_key(key),
                }
            }
        }
    };
}
crate::ui_icon_catalog!(declare_icon_family, metadata);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiIconSize {
    Small,
    Toolbar,
    Panel,
    Custom(u16),
}

impl UiIconSize {
    pub const fn logical_pixels(self) -> u16 {
        match self {
            Self::Small => 14,
            Self::Toolbar => 18,
            Self::Panel => 20,
            Self::Custom(size) => size,
        }
    }
}

impl Default for UiIconSize {
    fn default() -> Self {
        Self::Toolbar
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiIconState {
    Normal,
    Hovered,
    Active,
    Disabled,
}

impl Default for UiIconState {
    fn default() -> Self {
        Self::Normal
    }
}

/// A renderer-neutral icon request embedded in a retained node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiIconTone {
    #[default]
    Inherit,
    Normal,
    Muted,
    Accent,
    OnAccent,
    Disabled,
}

/// A renderer-neutral icon request embedded in a retained node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UiIcon {
    #[serde(default)]
    pub tone: UiIconTone,
    pub id: UiIconId,
    #[serde(default)]
    pub size: UiIconSize,
    #[serde(default)]
    pub state: UiIconState,
    #[serde(default)]
    pub tint: Option<[u8; 4]>,
}

impl UiIcon {
    pub const fn new(id: UiIconId) -> Self {
        Self {
            tone: UiIconTone::Inherit,
            id,
            size: UiIconSize::Toolbar,
            state: UiIconState::Normal,
            tint: None,
        }
    }

    pub const fn with_size(mut self, size: UiIconSize) -> Self {
        self.size = size;
        self
    }

    pub const fn with_tint(mut self, tint: [u8; 4]) -> Self {
        self.tint = Some(tint);
        self
    }
    pub const fn with_tone(mut self, tone: UiIconTone) -> Self {
        self.tone = tone;
        self
    }
    pub const fn with_state(mut self, state: UiIconState) -> Self {
        self.state = state;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, fs, path::Path};

    #[test]
    fn icon_identity_is_stable_and_does_not_depend_on_a_file_path() {
        let icon = UiIcon::new(UiIconId::Undo).with_size(UiIconSize::Small);

        assert_eq!(icon.id.key(), "undo");
        assert_eq!(icon.size.logical_pixels(), 14);
    }

    #[test]
    fn all_lists_every_icon_with_a_unique_key() {
        let keys: HashSet<&'static str> = UiIconId::ALL.iter().map(|id| id.key()).collect();

        assert_eq!(keys.len(), UiIconId::ALL.len(), "icon keys must be unique");
        assert_eq!(UiIconId::ALL.len(), 68);
    }

    #[test]
    fn descriptors_and_aliases_are_unique_and_resolve_to_their_canonical_id() {
        let mut lookup_keys = HashSet::new();
        let mut master_paths = HashSet::new();

        for id in UiIconId::ALL {
            let descriptor = id.descriptor();
            assert_eq!(descriptor.id, id);
            assert_eq!(descriptor.key, id.key());
            assert!(
                lookup_keys.insert(descriptor.key),
                "duplicate icon key: {}",
                descriptor.key
            );
            assert!(
                master_paths.insert(descriptor.master_svg),
                "duplicate SVG master: {}",
                descriptor.master_svg
            );
            assert!(descriptor.source_raster_size.iter().all(|size| *size > 0));
            assert!(descriptor.source_canvas_size.iter().all(|size| *size > 0));
            assert_eq!(
                descriptor.optical_size_px,
                UiIconSize::Toolbar.logical_pixels()
            );

            for alias in descriptor.aliases {
                assert!(lookup_keys.insert(alias), "duplicate icon alias: {alias}");
                assert_eq!(UiIconId::from_alias_key(alias), Some(id));
                assert_eq!(UiIconId::from_key(alias), Some(id));
            }
        }

        assert_eq!(UiIconId::from_alias_key("Undo"), Some(UiIconId::Undo));
        assert_eq!(UiIconId::from_alias_key("undo"), None);
        assert_eq!(UiIconId::from_key("unregistered-icon"), None);
    }

    #[test]
    fn descriptor_asset_references_match_existing_svg_and_png_sources() {
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

        for id in UiIconId::ALL {
            let descriptor = id.descriptor();
            let svg_path = repository_root.join(descriptor.master_svg);
            assert!(
                svg_path.is_file(),
                "missing SVG master for {}: {}",
                id.key(),
                svg_path.display()
            );

            let svg = fs::read_to_string(&svg_path).expect("read registered SVG master");
            let expected_svg_header = format!(
                "width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"",
                descriptor.source_raster_size[0],
                descriptor.source_raster_size[1],
                descriptor.source_canvas_size[0],
                descriptor.source_canvas_size[1],
            );
            assert!(
                svg.contains(&expected_svg_header),
                "SVG dimensions disagree with metadata for {}",
                id.key()
            );

            let png_path = repository_root
                .join("crates/raf_render/assets/ui_icons/png")
                .join(format!("{}.png", descriptor.key));
            assert!(
                png_path.is_file(),
                "missing runtime PNG for {}: {}",
                id.key(),
                png_path.display()
            );
            let png = fs::read(&png_path).expect("read registered runtime PNG");
            assert!(png.len() >= 24, "truncated PNG for {}", id.key());
            assert_eq!(
                &png[..8],
                b"\x89PNG\r\n\x1a\n",
                "invalid PNG signature for {}",
                id.key()
            );
            let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
            let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
            assert_eq!(
                [width, height],
                [
                    u32::from(descriptor.source_raster_size[0]),
                    u32::from(descriptor.source_raster_size[1])
                ],
                "runtime PNG dimensions disagree with metadata for {}",
                id.key(),
            );
        }
    }
}
