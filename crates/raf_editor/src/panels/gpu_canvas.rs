use eframe::{egui, egui_wgpu, wgpu};
use glam::Mat4;

use raf_render::api_graphic_basic::device::SceneFrameOutput;

pub struct GpuCanvas {
    texture_name: &'static str,
    texture: Option<egui::TextureHandle>,
    gpu_texture_id: Option<egui::TextureId>,
    last_size: [u32; 2],
}

impl GpuCanvas {
    pub fn new(texture_name: &'static str) -> Self {
        Self {
            texture_name,
            texture: None,
            gpu_texture_id: None,
            last_size: [1, 1],
        }
    }

    pub fn present(
        &mut self,
        ctx: &egui::Context,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        output: SceneFrameOutput,
        fallback_width: u32,
        fallback_height: u32,
    ) {
        match output {
            SceneFrameOutput::CpuPixels(pixels) => {
                if pixels.is_empty() || fallback_width == 0 || fallback_height == 0 {
                    return;
                }

                let size = [fallback_width as usize, fallback_height as usize];
                let image = egui::ColorImage::from_rgba_premultiplied(size, pixels.as_slice());
                self.upload_image(ctx, wgpu_render_state, image, fallback_width, fallback_height);
            }
            SceneFrameOutput::GpuTexture { view, width, height } => {
                self.update_gpu_texture(wgpu_render_state, &view, width, height);
            }
        }
    }

    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        let Some(texture_id) = self.current_texture_id() else {
            return;
        };

        painter.image(
            texture_id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    pub fn is_ready(&self) -> bool {
        self.current_texture_id().is_some()
    }

    fn current_texture_id(&self) -> Option<egui::TextureId> {
        self.gpu_texture_id
            .or_else(|| self.texture.as_ref().map(|texture| texture.id()))
    }

    fn upload_image(
        &mut self,
        ctx: &egui::Context,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        image: egui::ColorImage,
        width: u32,
        height: u32,
    ) {
        self.free_gpu_texture(wgpu_render_state);

        if let Some(texture) = &mut self.texture {
            if self.last_size == [width, height] {
                texture.set(image, egui::TextureOptions::LINEAR);
            } else {
                *texture = ctx.load_texture(self.texture_name, image, egui::TextureOptions::LINEAR);
            }
        } else {
            self.texture = Some(ctx.load_texture(self.texture_name, image, egui::TextureOptions::LINEAR));
        }

        self.last_size = [width, height];
    }

    fn update_gpu_texture(
        &mut self,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        texture_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        let Some(render_state) = wgpu_render_state else {
            return;
        };

        let mut renderer = render_state.renderer.write();
        if let Some(texture_id) = self.gpu_texture_id {
            renderer.update_egui_texture_from_wgpu_texture(
                render_state.device.as_ref(),
                texture_view,
                wgpu::FilterMode::Linear,
                texture_id,
            );
        } else {
            let texture_id = renderer.register_native_texture(
                render_state.device.as_ref(),
                texture_view,
                wgpu::FilterMode::Linear,
            );
            self.gpu_texture_id = Some(texture_id);
        }

        self.last_size = [width, height];
    }

    fn free_gpu_texture(&mut self, wgpu_render_state: Option<&egui_wgpu::RenderState>) {
        let Some(texture_id) = self.gpu_texture_id.take() else {
            return;
        };
        let Some(render_state) = wgpu_render_state else {
            return;
        };

        render_state.renderer.write().free_texture(&texture_id);
    }
}

pub fn canvas_view_projection(left: f32, right: f32, top: f32, bottom: f32) -> Mat4 {
    let width = (right - left).max(0.001);
    let height = (bottom - top).max(0.001);
    let scale_x = 2.0 / width;
    let scale_y = -2.0 / height;
    let translate_x = -(right + left) / width;
    let translate_y = (bottom + top) / height;

    Mat4::from_cols_array(&[
        scale_x, 0.0, 0.0, 0.0,
        0.0, scale_y, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        translate_x, translate_y, 0.0, 1.0,
    ])
}