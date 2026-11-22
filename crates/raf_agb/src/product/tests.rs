use std::cell::RefCell;
use std::rc::Rc;

use glam::Mat4;

use crate::{
    backends::recorder::Recorder, AdapterInfo, Backend, DrawMode, Error as AgbError, Frame,
    FrameMetrics, FramePlan, Instance, Result as AgbResult, Texture, Vertex, Workload,
};

use super::*;

fn packet() -> ProductPacket {
    let vertices = [
        [-0.8, -0.8, 0.5],
        [0.8, -0.8, 0.5],
        [0.8, 0.8, 0.5],
        [-0.8, 0.8, 0.5],
    ]
    .map(|position| Vertex {
        position,
        uv: [0.5; 2],
        rgba: [1.; 4],
        normal: [0., 1., 0.],
    });
    ProductPacket {
        meshes: vec![Mesh {
            vertices: vertices.to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
        }],
        materials: vec![Material {
            texture: Some(0),
            ..Material::default()
        }],
        textures: vec![Texture {
            size: [1, 1],
            rgba: vec![255, 0, 0, 255],
        }],
        commands: vec![
            Command::Clear([0., 0., 0., 1.]),
            Command::SetPipeline(Pipeline::LIT),
            Command::DrawMesh {
                mesh: 0,
                material: 0,
                instance: Instance::new(Mat4::IDENTITY, [1.; 4]),
            },
        ],
    }
}

fn open_recorder() -> ProductDevice<Recorder> {
    let mut device =
        ProductDevice::new(Recorder::default(), GraphicsMemoryBudget::potato()).unwrap();
    device.configure([64, 64], Output::Capture).unwrap();
    device
}

#[test]
fn packet_reuse_and_failed_patch_preserve_the_current_revision() {
    let mut device = open_recorder();
    let key = PacketKey(9);
    let handle = device.prepare(key, 10, packet).unwrap();
    let bytes = device.ledger().resident_bytes;
    assert_eq!(
        device
            .prepare(key, 10, || panic!("rebuilding a retained revision"))
            .unwrap(),
        handle
    );
    assert_eq!(
        device.mesh_span(handle, 0, true).unwrap(),
        Some(MeshSpan {
            vertices: 0..4,
            indices: Some(0..6),
        })
    );
    assert_eq!(device.instance_span(handle, 2).unwrap(), Some(1..2));
    let pixel = [0, 255, 0, 255];
    let out_of_bounds = ResourcePatch::Texture {
        index: 0,
        origin: [1, 0],
        size: [1, 1],
        rgba: &pixel,
    };
    assert!(matches!(
        device.patch(handle, 10, 11, out_of_bounds),
        Err(Error::Invalid(_))
    ));
    let patch = ResourcePatch::Texture {
        index: 0,
        origin: [0, 0],
        size: [1, 1],
        rgba: &pixel,
    };
    device.patch(handle, 10, 11, patch).unwrap();
    assert!(matches!(
        device.prepare(key, 10, packet),
        Err(Error::Invalid(_))
    ));
    assert_eq!(device.ledger().resident_bytes, bytes);
    let frame = device.draw(handle, &FrameUniforms::default()).unwrap();
    assert_eq!(frame.upload_bytes, 4);
    assert_eq!(
        device
            .draw(handle, &FrameUniforms::default())
            .unwrap()
            .upload_bytes,
        0
    );
    let replacement = device.prepare(key, 12, packet).unwrap();
    assert_ne!(handle, replacement);
    assert!(matches!(
        device.draw(handle, &FrameUniforms::default()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        open_recorder().draw(replacement, &FrameUniforms::default()),
        Err(Error::Invalid(_))
    ));
    assert_eq!(device.ledger().resident_bytes, bytes);
}

#[test]
fn coverage_rejects_unsupported_commands_without_discarding_the_current_packet() {
    let mut device = open_recorder();
    let key = PacketKey(7);
    let handle = device.prepare(key, 1, packet).unwrap();
    for (command, feature) in [(
        Command::Paint(Paint::Effect(Feature::RoundedClips)),
        Feature::RoundedClips,
    )] {
        let result = device.prepare(key, 2, || {
            let mut input = packet();
            input.commands.push(command);
            input
        });
        assert!(matches!(result, Err(Error::Unsupported(cause)) if cause == feature.name()));
    }
    assert_eq!(
        device
            .draw(handle, &FrameUniforms::default())
            .unwrap()
            .draw_calls,
        1
    );
}

#[cfg(all(windows, feature = "vulkan", feature = "dx12"))]
#[test]
#[ignore = "requires Vulkan and D3D12 hardware in the current session"]
fn native_executors_draw_indexed_lighting_and_apply_partial_updates() {
    use crate::backends::native::{NativeApi, NativeBackend, NativeRequest};
    use glam::Vec3;
    let center = (32 * 64 + 32) * 4;
    for api in [NativeApi::Vulkan, NativeApi::Dx12] {
        let backend = NativeBackend::headless(NativeRequest::Exact(api)).unwrap();
        assert!(
            backend.info().hardware,
            "{api} initialized a software device"
        );
        let mut device = ProductDevice::new(backend, GraphicsMemoryBudget::potato()).unwrap();
        device.configure([64, 64], Output::Capture).unwrap();
        let handle = device.prepare(PacketKey(1), 1, packet).unwrap();
        let mut uniforms = FrameUniforms::default();
        device.draw(handle, &uniforms).unwrap();
        assert_eq!(
            &device.capture().unwrap()[center..center + 4],
            &[255, 0, 0, 255]
        );
        device
            .patch(
                handle,
                1,
                2,
                ResourcePatch::Texture {
                    index: 0,
                    origin: [0, 0],
                    size: [1, 1],
                    rgba: &[0, 255, 0, 255],
                },
            )
            .unwrap();
        uniforms.light.toward_light = -Vec3::Y;
        device.draw(handle, &uniforms).unwrap();
        let color = device.capture().unwrap();
        assert!(
            (88..=91).contains(&color[center + 1]),
            "{api}: {:?}",
            &color[center..center + 4]
        );

        let instances = device.instance_span(handle, 2).unwrap().unwrap();
        device
            .patch(
                handle,
                2,
                3,
                ResourcePatch::Instances {
                    first: instances.start,
                    data: &[Instance::new(Mat4::from_translation(Vec3::X * 3.), [1.; 4])],
                },
            )
            .unwrap();
        device.draw(handle, &uniforms).unwrap();
        assert_eq!(
            &device.capture().unwrap()[center..center + 4],
            &[0, 0, 0, 255]
        );
    }
}

#[derive(Default)]
struct ProbeState {
    frame: Option<Frame>,
    modes: Vec<DrawMode>,
}

/// Records the validated frame and batch modes without a GPU, so the viewport
/// handoff is checked at the contract boundary every executor consumes.
#[derive(Clone)]
struct FrameProbe {
    info: AdapterInfo,
    state: Rc<RefCell<ProbeState>>,
}

impl FrameProbe {
    fn new() -> Self {
        Self {
            info: AdapterInfo {
                backend: "probe",
                name: "CPU".into(),
                driver: String::new(),
                hardware: false,
                max_texture: 16384,
                max_buffer: u32::MAX.into(),
                gpu_timestamps: false,
            },
            state: Rc::new(RefCell::new(ProbeState::default())),
        }
    }

    fn state(&self) -> std::cell::Ref<'_, ProbeState> {
        self.state.borrow()
    }
}

impl Backend for FrameProbe {
    type Resident = u32;

    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn configure(&mut self, _: &FramePlan) -> AgbResult<()> {
        Ok(())
    }

    fn upload(&mut self, workload: &Workload) -> AgbResult<u32> {
        self.state.borrow_mut().modes =
            workload.batches.iter().map(|batch| batch.mode).collect();
        Ok(workload.batches.len() as u32)
    }

    fn draw(&mut self, _: &u32, _: &FramePlan, frame: &Frame) -> AgbResult<FrameMetrics> {
        let mut state = self.state.borrow_mut();
        state.frame = Some(*frame);
        Ok(FrameMetrics {
            draw_calls: state.modes.len() as u32,
            submits: 1,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> AgbResult<()> {
        Ok(())
    }

    fn capture(&mut self) -> AgbResult<Vec<u8>> {
        Err(AgbError::Invalid("the frame probe has no pixels"))
    }
}

fn open_probe(extent: [u32; 2]) -> (ProductDevice<FrameProbe>, FrameProbe) {
    let probe = FrameProbe::new();
    let mut device =
        ProductDevice::new(probe.clone(), GraphicsMemoryBudget::potato()).unwrap();
    device.configure(extent, Output::Capture).unwrap();
    (device, probe)
}

fn paint_packet() -> ProductPacket {
    let mut input = packet();
    input.commands.push(Command::Paint(Paint::Solid(PaintQuad {
        rect: [0., 0., 1., 1.],
        clip: [0., 0., 1., 1.],
        uv_rect: [0., 0., 1., 1.],
        color: [0.1, 0.2, 0.3, 1.],
    })));
    input
}

fn with_scene_background(color: [f32; 4]) -> ProductPacket {
    let mut input = packet();
    input
        .commands
        .insert(1, Command::SceneBackground(color));
    input
}

#[test]
fn scene_background_precedes_geometry_and_only_runs_in_the_scene_group() {
    let color = [0.02, 0.03, 0.05, 1.];
    let encoded = super::encode::encode(with_scene_background(color), u64::MAX).unwrap();
    let background = &encoded.workload.batches[0];
    assert_eq!(background.mode, DrawMode::SceneNoDepth);
    assert!(background.mode.is_scene());
    let vertices = &encoded.workload.vertices
        [background.vertices.start as usize..background.vertices.end as usize];
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0].position, [-1., -1., 0.]);
    assert_eq!(vertices[1].position, [3., -1., 0.]);
    assert_eq!(vertices[2].position, [-1., 3., 0.]);
    assert_eq!(vertices[0].uv, [1., 1.]);
    assert!(vertices.iter().all(|vertex| vertex.rgba == color));
    assert!(encoded
        .workload
        .batches
        .iter()
        .any(|batch| batch.mode == DrawMode::Scene));

    let mut after_paint = packet();
    after_paint.commands.push(Command::Paint(Paint::Solid(PaintQuad {
        rect: [0., 0., 1., 1.],
        clip: [0., 0., 1., 1.],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
    })));
    after_paint.commands.push(Command::SceneBackground(color));
    assert!(matches!(
        super::encode::encode(after_paint, u64::MAX),
        Err(Error::Invalid(_))
    ));

    let mut bad_color = packet();
    bad_color
        .commands
        .insert(1, Command::SceneBackground([2., 0., 0., 1.]));
    assert!(matches!(
        super::encode::encode(bad_color, u64::MAX),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn frame_reports_the_full_target_viewport_by_default() {
    let (mut device, probe) = open_probe([64, 32]);
    let handle = device.prepare(PacketKey(20), 1, packet).unwrap();
    device.draw(handle, &FrameUniforms::default()).unwrap();
    let state = probe.state();
    let frame = state.frame.expect("frame submitted");
    assert_eq!(frame.viewport, [0., 0., 1., 1.]);
    assert_eq!(frame.uniforms([64, 32]).viewport, [64., 32., 0., 0.]);
    assert_eq!(frame.viewport_rect([64, 32]).pixels, [0, 0, 64, 32]);
}

#[test]
fn scene_viewport_reports_its_pixel_size_and_keeps_paint_full_target() {
    let (mut device, probe) = open_probe([1600, 900]);
    let handle = device.prepare(PacketKey(21), 1, paint_packet).unwrap();
    let uniforms = FrameUniforms {
        viewport: [0.25, 0.1, 0.5, 0.6],
        ..Default::default()
    };
    device.draw(handle, &uniforms).unwrap();
    let state = probe.state();
    let frame = state.frame.expect("frame submitted");
    assert_eq!(frame.viewport, [0.25, 0.1, 0.5, 0.6]);
    assert_eq!(frame.uniforms([1600, 900]).viewport, [800., 540., 0., 0.]);
    let scene = frame.viewport_rect([1600, 900]);
    assert_eq!(scene.pixels, [400, 90, 800, 540]);
    assert_eq!(
        crate::device::ViewportRect::for_mode(scene, DrawMode::Overlay, [1600, 900]),
        crate::device::ViewportRect::full([1600, 900])
    );
    assert_eq!(state.modes, [DrawMode::Scene, DrawMode::Overlay]);
}

#[test]
fn invalid_scene_viewport_is_rejected_before_the_backend_draws() {
    let (mut device, probe) = open_probe([1600, 900]);
    let handle = device.prepare(PacketKey(22), 1, packet).unwrap();
    for viewport in [
        [0.5, 0., 0.6, 1.],
        [0., 0., 0., 1.],
        [f32::NAN, 0., 1., 1.],
        [0., -0.1, 1., 1.],
    ] {
        let uniforms = FrameUniforms {
            viewport,
            ..Default::default()
        };
        assert!(matches!(
            device.draw(handle, &uniforms),
            Err(Error::Invalid(_))
        ));
    }
    assert!(probe.state().frame.is_none());
}
