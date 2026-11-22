use std::cell::RefCell;
use std::mem::{offset_of, size_of};
use std::rc::Rc;

use super::*;

#[derive(Default)]
struct LayerProbeState {
    draws: u32,
}

struct LayerProbe {
    info: AdapterInfo,
    state: Rc<RefCell<LayerProbeState>>,
}

impl Backend for LayerProbe {
    type Resident = u32;

    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn configure(&mut self, _: &FramePlan) -> Result<()> {
        Ok(())
    }

    fn upload(&mut self, _: &Workload) -> Result<u32> {
        Ok(0)
    }

    fn draw(&mut self, _: &u32, _: &FramePlan, _: &Frame) -> Result<FrameMetrics> {
        self.state.borrow_mut().draws += 1;
        Ok(FrameMetrics {
            draw_calls: 1,
            submits: 1,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> Result<()> {
        Ok(())
    }

    fn capture(&mut self) -> Result<Vec<u8>> {
        Err(Error::Invalid("the layer probe has no pixels"))
    }
}

#[test]
fn uniforms_map_every_lighting_style_channel_to_lighting() {
    let frame = Frame {
        light_color: [0.2, 0.4, 0.6, 1.],
        lighting_style: [1., 0., 4., 0.25],
        ..crate::fixtures::camera([800, 600], 0.)
    };
    let uniforms = frame.uniforms([800, 600]);
    assert_eq!(uniforms.lighting, [1., 0., 4., 0.25]);
    assert_eq!(uniforms.light_color, [0.2, 0.4, 0.6, 1.]);
}

#[test]
fn uniforms_keep_viewport_and_camera_position_out_of_lighting() {
    let frame = Frame {
        viewport: [0.25, 0.1, 0.5, 0.6],
        eye: [1., 2., 3.],
        lighting_style: [1., 1., 6., 1.],
        ..crate::fixtures::camera([1600, 900], 0.)
    };
    let uniforms = frame.uniforms([1600, 900]);
    assert_eq!(uniforms.viewport, [800., 540., 0., 0.]);
    assert_eq!(uniforms.eye, [1., 2., 3., 0.]);
    assert_eq!(uniforms.lighting, [1., 1., 6., 1.]);
}

#[test]
fn default_product_flags_leave_lighting_zeroed() {
    let frame = crate::product::FrameUniforms::default()
        .raster([0.05, 0.07, 0.10, 1.])
        .unwrap();
    let uniforms = frame.uniforms([640, 360]);
    assert_eq!(uniforms.lighting, [0.; 4]);
    assert_eq!(uniforms.light_color[3], 1.);
    assert_eq!(uniforms.viewport, [640., 360., 0., 0.]);
    assert_eq!(uniforms.eye[3], 0.);
}

#[test]
fn raster_uniform_bytes_match_the_shader_frame() {
    let body = crate::SHADER
        .split_once("struct Frame {")
        .and_then(|(_, rest)| rest.split_once("};"))
        .map(|(body, _)| body)
        .expect("scene frame struct");
    let fields: Vec<&str> = body
        .lines()
        .filter_map(|line| line.trim().split_once(':').map(|(name, _)| name))
        .collect();
    assert_eq!(
        fields,
        [
            "view_projection",
            "light",
            "light_color",
            "viewport",
            "eye",
            "environment",
            "lighting",
        ]
    );
    // One mat4 plus one vec4 per remaining field, packed without padding.
    assert_eq!(size_of::<RasterUniforms>(), 16 * 4 + (fields.len() - 1) * 16);
    assert_eq!(size_of::<RasterUniforms>(), 160);
    assert_eq!(offset_of!(RasterUniforms, lighting), 144);

    let js = include_str!("../../web/backend.js");
    let declared = js
        .lines()
        .find_map(|line| line.trim().strip_prefix("const CAMERA_BYTES = "))
        .and_then(|value| value.strip_suffix(';'))
        .expect("web camera size");
    assert_eq!(declared, size_of::<RasterUniforms>().to_string());
}

#[test]
fn releasing_a_workload_rejects_stale_handles_and_returns_its_budget() {
    let mut device = Device::new(
        crate::backends::recorder::Recorder::default(),
        GraphicsMemoryBudget::potato(),
    )
    .unwrap();
    device.configure([640, 360], Output::Capture).unwrap();
    let fixture = crate::fixtures::scene(true, 3);
    let frame = crate::fixtures::camera([640, 360], 0.8);
    let before = device.ledger().resident_bytes;
    let handle = device.upload(&fixture).unwrap();
    assert_eq!(device.draw(handle, &frame).unwrap().draw_calls, 3);
    device.release(handle).unwrap();
    assert!(matches!(
        device.draw(handle, &frame),
        Err(Error::Invalid(_))
    ));
    assert_eq!(device.ledger().resident_bytes, before);
    assert_ne!(device.upload(&fixture).unwrap(), handle);
}

#[test]
fn rejects_invalid_geometry_before_admitting_any_bytes() {
    let mut device = Device::new(
        crate::backends::recorder::Recorder::default(),
        GraphicsMemoryBudget::potato(),
    )
    .unwrap();
    let mut fixture = crate::fixtures::scene(false, 3);
    fixture.batches[0].vertices.end += 1;
    assert!(matches!(device.upload(&fixture), Err(Error::Invalid(_))));
    assert_eq!(device.ledger().resident_bytes, 0);
}

#[test]
fn default_layered_draw_delegates_one_layer_and_rejects_more() {
    let state = Rc::new(RefCell::new(LayerProbeState::default()));
    let probe = LayerProbe {
        info: AdapterInfo {
            backend: "layer-probe",
            name: "CPU".into(),
            driver: String::new(),
            hardware: false,
            max_texture: 16384,
            max_buffer: u32::MAX.into(),
            gpu_timestamps: false,
        },
        state: Rc::clone(&state),
    };
    let mut device = Device::new(probe, GraphicsMemoryBudget::potato()).unwrap();
    device.configure([64, 64], Output::Capture).unwrap();
    assert!(!device.supports_layers());
    let first = device.upload(&crate::fixtures::scene(true, 2)).unwrap();
    let second = device.upload(&crate::fixtures::scene(true, 2)).unwrap();
    let frame = crate::fixtures::camera([64, 64], 0.5);

    assert_eq!(device.draw_layers(&[first], &frame).unwrap().draw_calls, 1);
    assert_eq!(state.borrow().draws, 1);
    assert!(matches!(
        device.draw_layers(&[first, second], &frame),
        Err(Error::Unsupported("layered composition"))
    ));
    assert_eq!(
        state.borrow().draws,
        1,
        "a rejected layered draw must not reach the executor"
    );
    assert!(matches!(
        device.draw_layers(&[], &frame),
        Err(Error::Invalid(_))
    ));
}
