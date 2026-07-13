//! Direct retained CAD surface host without Egui presentation.

use glam::Vec2;
use raf_electronics::CadScene;

use crate::api_graphic_basic::canvas_presenter::DirectSceneSurfaceHost;
use crate::api_graphic_basic::device::BasicDevice;

use super::cad_surface::{
    build_cad_surface_frame, CadSurfaceFrame, CadSurfaceHitRegion, CadSurfaceOptions,
};

pub struct DirectCadSurfaceHost {
    scene_host: DirectSceneSurfaceHost,
    frame: Option<CadSurfaceFrame>,
}

impl DirectCadSurfaceHost {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            scene_host: DirectSceneSurfaceHost::new(device, color_format, clear_color),
            frame: None,
        }
    }

    pub fn rebuild(
        &mut self,
        scene: &CadScene,
        width: u32,
        height: u32,
        options: CadSurfaceOptions,
    ) -> &CadSurfaceFrame {
        self.frame = Some(build_cad_surface_frame(scene, width, height, options));
        self.frame.as_ref().expect("CAD frame was just built")
    }

    pub fn frame(&self) -> Option<&CadSurfaceFrame> {
        self.frame.as_ref()
    }

    pub fn hit_test(&self, point: Vec2) -> Option<&CadSurfaceHitRegion> {
        self.frame.as_ref()?.hit_test(point)
    }

    pub fn render(
        &mut self,
        basic_device: &mut BasicDevice,
        presentation_device: &wgpu::Device,
        presentation_queue: &wgpu::Queue,
        target: &wgpu::TextureView,
    ) -> bool {
        let Some(frame) = self.frame.as_ref() else {
            return false;
        };
        self.scene_host.render_frame(
            basic_device,
            presentation_device,
            presentation_queue,
            target,
            &frame.frame,
        );
        true
    }
}
