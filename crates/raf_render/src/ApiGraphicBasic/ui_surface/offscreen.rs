//! An owned UI target sampled by the existing scene renderer, without readback.
use super::{DirectUiSurfaceHost, NativeUiWindowHost};
use crate::api_graphic_basic::device::GpuTextureView;
use crate::api_graphic_basic::handles::TextureHandle;
use std::sync::Arc;

pub struct UiOffscreenTarget {
    _texture: wgpu::Texture,
    view: GpuTextureView,
    size: [u32; 2],
}
impl UiOffscreenTarget {
    pub fn view(&self) -> GpuTextureView {
        self.view.clone()
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
}
impl NativeUiWindowHost {
    pub fn create_ui_target(&self, size: [u32; 2]) -> Result<UiOffscreenTarget, String> {
        if size
            .iter()
            .any(|v| *v == 0 || *v > 2048 || *v > self.device.limits().max_texture_dimension_2d)
        {
            return Err("ui.canvas_budget".into());
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.GameUiTarget"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.configuration.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        Ok(UiOffscreenTarget {
            _texture: texture,
            view: GpuTextureView::from_wgpu(view, TextureHandle::INVALID),
            size,
        })
    }
    pub fn render_ui_target<F>(
        &self,
        ui: &mut DirectUiSurfaceHost,
        target: &UiOffscreenTarget,
        resolve: F,
    ) where
        F: FnMut(&str) -> String,
    {
        ui.render_at_scale(
            &self.device,
            &self.queue,
            target.view.as_wgpu(),
            target.size,
            target.size,
            1.0,
            resolve,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CpuUiSurfaceHost, StudioUiPalette, UiSurface};
    use raf_ui::*;
    #[test]
    fn game_ui_anchors_use_the_parent_content_and_do_not_consume_flow_tracks() {
        let anchored = UiNode::new("anchored", UiNodeKind::Panel)
            .with_layout(UiLayout {
                anchored: Some(raf_ui::game_ui::UiAnchoredRect {
                    anchor: [1.0, 0.0],
                    pivot: [1.0, 0.0],
                    relative_size: [0.0; 2],
                    offset: [-12.0, 4.0],
                    size: [80.0, 24.0],
                }),
                ..Default::default()
            })
            .with_style(UiStyle {
                fill: [220, 40, 30, 255],
                ..UiStyle::transparent()
            });
        let normal =
            UiNode::new("normal", UiNodeKind::Panel).with_layout(UiLayout::fixed(20.0, 24.0));
        let root = UiNode::row("root", [normal, anchored]).with_layout(UiLayout::fill(UiFlow::Row));
        let mut host = CpuUiSurfaceHost::new(
            UiSurface::new("anchors", StudioUiPalette::IndustrialDark, root),
            [0; 4],
        );
        host.render([320, 64], |key| key.into());
        assert_eq!(host.layout_rect("anchored").unwrap().x, 228.0);
        assert_eq!(host.layout_rect("normal").unwrap().x, 0.0);
        host.render([480, 64], |key| key.into());
        assert_eq!(host.layout_rect("anchored").unwrap().x, 388.0);
    }
}
