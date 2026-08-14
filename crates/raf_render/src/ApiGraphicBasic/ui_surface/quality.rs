//! Deterministic CPU quality matrix for retained RafUI surfaces.
//!
//! This is a headless visual gate, not a second renderer. It exercises the
//! same CPU recovery compositor used when a GPU is unavailable and records a
//! stable pixel hash plus diagnostics at the supported DPI points.

use super::{CpuUiSurfaceHost, UiEnvironment, UiSurface, UiSurfaceDiagnostics};

#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceQualitySample {
    pub scale_percent: u16,
    pub logical_size: [u32; 2],
    pub physical_size: [u32; 2],
    pub pixel_hash: u64,
    pub pixel_count: usize,
    pub atlas_revision: u64,
    pub diagnostics: UiSurfaceDiagnostics,
}

/// Runs the canonical 100/125/150/200% matrix for a surface.
pub fn cpu_quality_matrix<F>(
    surface: &UiSurface,
    logical_size: [u32; 2],
    clear_color: [u8; 4],
    mut resolve: F,
) -> Vec<UiSurfaceQualitySample>
where
    F: FnMut(&str) -> String,
{
    [100_u16, 125, 150, 200]
        .into_iter()
        .map(|scale_percent| {
            let scale = f32::from(scale_percent) / 100.0;
            let physical_size = [
                (logical_size[0] as f32 * scale).round().max(1.0) as u32,
                (logical_size[1] as f32 * scale).round().max(1.0) as u32,
            ];
            let mut host = CpuUiSurfaceHost::new(surface.clone(), clear_color);
            let mut environment = UiEnvironment::new(logical_size[0], logical_size[1]);
            environment.scale_factor = scale;
            let (pixel_hash, pixel_count, diagnostics) = {
                let frame =
                    host.render_at_scale(physical_size, logical_size, scale, |key| resolve(key));
                (
                    stable_pixel_hash(frame.pixels),
                    frame.pixels.len() / 4,
                    UiSurfaceDiagnostics::from_frame_at_density(
                        frame.frame,
                        environment.density_contract(),
                    ),
                )
            };
            UiSurfaceQualitySample {
                scale_percent,
                logical_size,
                physical_size,
                pixel_hash,
                pixel_count,
                atlas_revision: host.session().text_atlas.revision(),
                diagnostics,
            }
        })
        .collect()
}

fn stable_pixel_hash(pixels: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in pixels {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ui::{StudioUiPalette, UiLayout, UiNode, UiNodeKind};

    #[test]
    fn matrix_covers_the_four_supported_densities() {
        let surface = UiSurface::new(
            "quality",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("title", UiNodeKind::Label)
                    .with_layout(UiLayout::fixed(180.0, 24.0))
                    .with_text_key("app.title"),
            ),
        );
        let samples = cpu_quality_matrix(&surface, [160, 80], [0, 0, 0, 255], |_| {
            "RafUI Quality".to_string()
        });

        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.scale_percent)
                .collect::<Vec<_>>(),
            vec![100, 125, 150, 200]
        );
        assert_eq!(samples[3].physical_size, [320, 160]);
        assert!(samples.iter().all(|sample| sample.pixel_count > 0));
        assert!(samples
            .iter()
            .all(|sample| !sample.diagnostics.has_layout_warnings()));
    }
}
