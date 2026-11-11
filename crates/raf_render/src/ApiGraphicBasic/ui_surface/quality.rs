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

/// A real retained fixture for inspecting paint capabilities. Callers may
/// present it in the native host or capture it through the CPU quality matrix.
/// It does not install a second authoring UI or run an idle animation.
pub fn craftsmanship_gallery(palette: raf_ui::StudioUiPalette) -> UiSurface {
    use raf_ui::*;
    let tokens = palette.tokens();
    let row = |id: &str, caption: &str, visual: UiVisual| {
        UiNode::new(id, UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(260.0, 40.0).padding(UiSpacing::xy(10.0, 4.0)))
            .with_text_key(caption)
            .with_style(UiStyle {
                fill: tokens.surface_raised,
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 8.0,
                opacity: 1.0,
            })
            .with_visual(visual)
    };
    let root = UiNode::new("gallery", UiNodeKind::Root)
        .with_layout(UiLayout::column().gap(12.0).padding(UiSpacing::same(16.0)))
        .with_children([
            row("gallery.border", "ui.gallery.border", UiVisual::default()).with_style(UiStyle {
                fill: [0; 4],
                border: tokens.accent,
                text: tokens.text,
                border_width: 2.0,
                radius: 8.0,
                opacity: 1.0,
            }),
            row(
                "gallery.gradient",
                "ui.gallery.gradient",
                UiVisual {
                    gradient: Some(UiLinearGradient {
                        start: tokens.surface,
                        end: tokens.selection,
                        direction: [1.0, 0.0],
                    }),
                    ..Default::default()
                },
            ),
            row(
                "gallery.corners",
                "ui.gallery.corners",
                UiVisual {
                    corners: Some([0.0, 12.0, 0.0, 12.0]),
                    ..Default::default()
                },
            ),
            row(
                "gallery.shadow",
                "ui.gallery.shadow",
                UiVisual {
                    shadow: Some(UiShadow {
                        offset: [0.0, 3.0],
                        softness: 5.0,
                        spread: 4.0,
                        color: [0, 0, 0, 90],
                    }),
                    ..Default::default()
                },
            ),
            UiNode::image(
                "gallery.missing",
                UiImage {
                    source: UiImageSource::new("gallery://missing"),
                    fit: UiImageFit::Cover,
                    tint: None,
                    options: UiImageOptions::default(),
                },
            )
            .with_layout(UiLayout::fixed(80.0, 54.0))
            .with_visual(UiVisual {
                corners: Some([8.0; 4]),
                ..Default::default()
            }),
        ]);
    UiSurface::new("rafui.craftsmanship", palette, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ui::{StudioUiPalette, UiLayout, UiNode, UiNodeKind};

    #[test]
    fn rafui_craftsmanship_hollow_border_and_rounded_image_keep_transparent_corners() {
        use raf_ui::*;
        let mut root = UiNode::new("root", UiNodeKind::Root);
        root.children.push(
            UiNode::new("border", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(20.0, 20.0))
                .with_style(UiStyle {
                    fill: [0; 4],
                    border: [255, 0, 0, 255],
                    text: [255; 4],
                    border_width: 2.0,
                    radius: 8.0,
                    opacity: 1.0,
                }),
        );
        let mut host = CpuUiSurfaceHost::new(
            UiSurface::new("border", StudioUiPalette::IndustrialDark, root),
            [0; 4],
        );
        let frame = host.render_at_scale([20, 20], [20, 20], 1.0, |s| s.to_owned());
        assert_eq!(frame.pixels[3], 0, "outer corner must remain transparent");
        assert_eq!(
            frame.pixels[(10 * 20 + 10) * 4 + 3],
            0,
            "hollow center must remain transparent"
        );
        assert!(
            frame.pixels[10 * 4 + 3] > 200,
            "top edge must remain visible"
        );

        let image = UiNode::image(
            "image",
            UiImage {
                source: UiImageSource::new("test"),
                fit: UiImageFit::Cover,
                tint: None,
                options: UiImageOptions::default(),
            },
        )
        .with_layout(UiLayout::fixed(20.0, 20.0))
        .with_visual(UiVisual {
            corners: Some([8.0; 4]),
            ..Default::default()
        });
        let mut host = CpuUiSurfaceHost::new(
            UiSurface::new(
                "image",
                StudioUiPalette::IndustrialDark,
                UiNode::new("root", UiNodeKind::Root).with_child(image),
            ),
            [0; 4],
        );
        host.images_mut()
            .insert_rgba("test", [1, 1], vec![255; 4])
            .unwrap();
        let frame = host.render_at_scale([20, 20], [20, 20], 1.0, |s| s.to_owned());
        assert_eq!(frame.pixels[3], 0);
        assert_eq!(frame.pixels[(10 * 20 + 10) * 4 + 3], 255);
    }

    #[test]
    fn rafui_craftsmanship_gallery_has_finite_boxes_across_themes_and_density() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let samples =
                cpu_quality_matrix(&craftsmanship_gallery(palette), [300, 300], [0; 4], |key| {
                    key.to_owned()
                });
            assert!(samples.iter().all(
                |s| s.diagnostics.invalid_clip_regions == 0 && s.diagnostics.duplicate_ids == 0
            ));
        }
    }

    #[test]
    fn rafui_craftsmanship_declared_labels_have_both_translations() {
        for key in [
            "ui.gallery.border",
            "ui.gallery.gradient",
            "ui.gallery.corners",
            "ui.gallery.shadow",
            "app.more_menu",
        ] {
            for language in [
                raf_core::config::Language::English,
                raf_core::config::Language::Spanish,
            ] {
                assert_ne!(
                    raf_core::i18n::t(key, language),
                    key,
                    "missing translation for {key}"
                );
            }
        }
    }

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
