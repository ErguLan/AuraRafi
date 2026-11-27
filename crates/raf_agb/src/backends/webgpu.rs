use js_sys::Promise;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlCanvasElement;

use crate::{
    capabilities::GraphicsMemoryBudget, fixtures, AdapterInfo, Backend, DrawMode, Error, Frame,
    FrameMetrics, FramePlan, Instance, Output, ResourcePatch, Result, Vertex, Workload,
};

#[wasm_bindgen(module = "/web/backend.js")]
extern "C" {
    type BrowserGpu;
    type BrowserResident;

    #[wasm_bindgen(js_name = openGpu)]
    fn open_gpu(canvas: HtmlCanvasElement, shader: &str) -> Promise;

    #[wasm_bindgen(method, getter, js_name = adapterName)]
    fn adapter_name(this: &BrowserGpu) -> String;
    #[wasm_bindgen(method, getter, js_name = maxTexture)]
    fn max_texture(this: &BrowserGpu) -> u32;
    #[wasm_bindgen(method, getter, js_name = maxBuffer)]
    fn max_buffer(this: &BrowserGpu) -> f64;
    #[wasm_bindgen(method, getter)]
    fn hardware(this: &BrowserGpu) -> bool;
    #[wasm_bindgen(method, getter)]
    fn lost(this: &BrowserGpu) -> bool;
    #[wasm_bindgen(method, getter, js_name = lossSignal)]
    fn loss_signal(this: &BrowserGpu) -> Promise;
    #[wasm_bindgen(method, catch)]
    fn configure(this: &BrowserGpu, width: u32, height: u32) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(method, catch, js_name = uploadGeometry)]
    fn upload_geometry(
        this: &BrowserGpu,
        vertices: &[u8],
        instances: &[u8],
        indices: &[u8],
        draws: &[u32],
    ) -> std::result::Result<BrowserResident, JsValue>;
    #[wasm_bindgen(method, catch, js_name = uploadTexture)]
    fn upload_texture(
        this: &BrowserGpu,
        resident: &BrowserResident,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(method, catch, js_name = patchBuffer)]
    fn patch_buffer(
        this: &BrowserGpu,
        resident: &BrowserResident,
        instances: bool,
        offset: u32,
        bytes: &[u8],
    ) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(method, catch, js_name = patchTexture)]
    fn patch_texture(
        this: &BrowserGpu,
        resident: &BrowserResident,
        index: u32,
        origin: &[u32],
        size: &[u32],
        rgba: &[u8],
    ) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn draw(
        this: &BrowserGpu,
        resident: &BrowserResident,
        view: &[f32],
        clear: &[f32],
        viewport: &[u32],
    ) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(method, js_name = waitAsync)]
    fn wait_async(this: &BrowserGpu) -> Promise;
    #[wasm_bindgen(method, js_name = closeDevice)]
    fn close_device(this: &BrowserGpu);
    #[wasm_bindgen(method)]
    fn destroy(this: &BrowserResident);
}

pub struct WebGpu {
    raw: BrowserGpu,
    info: AdapterInfo,
    last_camera: Option<crate::RasterUniforms>,
}
pub struct Resident {
    raw: BrowserResident,
    draws: u32,
}

impl WebGpu {
    pub async fn open(canvas: HtmlCanvasElement) -> std::result::Result<Self, JsValue> {
        let raw: BrowserGpu = JsFuture::from(open_gpu(canvas, crate::SHADER))
            .await?
            .unchecked_into();
        let info = AdapterInfo {
            backend: "WebGPU",
            name: raw.adapter_name(),
            driver: String::new(),
            hardware: raw.hardware(),
            max_texture: raw.max_texture(),
            max_buffer: raw.max_buffer() as u64,
            gpu_timestamps: false,
        };
        Ok(Self {
            raw,
            info,
            last_camera: None,
        })
    }

    fn tracking_handle(&self) -> BrowserGpu {
        <BrowserGpu as AsRef<JsValue>>::as_ref(&self.raw)
            .clone()
            .unchecked_into()
    }
}

impl Drop for WebGpu {
    fn drop(&mut self) {
        self.raw.close_device();
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        self.raw.destroy();
    }
}

impl Backend for WebGpu {
    type Resident = Resident;

    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn supports_frame_instance_patches(&self) -> bool {
        true
    }

    fn configure(&mut self, plan: &FramePlan) -> Result<()> {
        if plan.output != Output::Present {
            return Err(Error::Invalid("browser probe uses a canvas target"));
        }
        self.raw
            .configure(plan.extent[0], plan.extent[1])
            .map_err(js_error)
    }

    fn upload(&mut self, workload: &Workload) -> Result<Resident> {
        let draws: Vec<u32> = workload
            .batches
            .iter()
            .flat_map(|batch| {
                let range = batch.indices.as_ref().unwrap_or(&batch.vertices);
                [
                    // Keep these indices in sync with the pipeline array order
                    // in web/backend.js.
                    match batch.mode {
                        DrawMode::Scene => 0,
                        DrawMode::SceneTwoSided => 1,
                        DrawMode::Overlay | DrawMode::SceneNoDepth => 2,
                        DrawMode::DepthReadOnly => 3,
                        DrawMode::SceneTransparent => 4,
                        DrawMode::SceneTransparentTwoSided => 5,
                    },
                    batch.texture,
                    range.start,
                    range.len() as u32,
                    batch.instances.start,
                    batch.instances.len() as u32,
                    u32::from(batch.indices.is_some()),
                    u32::from(batch.mode.is_scene()),
                ]
            })
            .collect();
        let raw = self
            .raw
            .upload_geometry(
                bytemuck::cast_slice(&workload.vertices),
                bytemuck::cast_slice(&workload.instances),
                bytemuck::cast_slice(&workload.indices),
                &draws,
            )
            .map_err(js_error)?;
        let resident = Resident {
            raw,
            draws: workload.batches.len() as u32,
        };
        for texture in &workload.textures {
            self.raw
                .upload_texture(
                    &resident.raw,
                    texture.size[0],
                    texture.size[1],
                    &texture.rgba,
                )
                .map_err(js_error)?;
        }
        Ok(resident)
    }

    unsafe fn patch(&mut self, resident: &mut Resident, patch: ResourcePatch<'_>) -> Result<()> {
        if self.raw.lost() {
            return Err(Error::DeviceLost);
        }
        let result = match patch {
            ResourcePatch::Vertices { first, data } => self.raw.patch_buffer(
                &resident.raw,
                false,
                first * size_of::<Vertex>() as u32,
                bytemuck::cast_slice(data),
            ),
            ResourcePatch::Instances { first, data } => self.raw.patch_buffer(
                &resident.raw,
                true,
                first * size_of::<Instance>() as u32,
                bytemuck::cast_slice(data),
            ),
            ResourcePatch::Texture {
                index,
                origin,
                size,
                rgba,
            } => self
                .raw
                .patch_texture(&resident.raw, index, &origin, &size, rgba),
        };
        result.map_err(js_error)
    }

    fn draw(
        &mut self,
        resident: &Resident,
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        if self.raw.lost() {
            return Err(Error::DeviceLost);
        }
        if plan.graph.order.as_slice() != [0, 1, 2] {
            return Err(Error::Invalid("unexpected browser pass order"));
        }
        let uniforms = frame.uniforms(plan.extent);
        let dirty = self.last_camera != Some(uniforms);
        let view = if dirty {
            bytemuck::cast_slice(std::slice::from_ref(&uniforms))
        } else {
            &[]
        };
        let scene = frame.viewport_rect(plan.extent);
        self.raw
            .draw(&resident.raw, view, &frame.clear, &scene.pixels)
            .map_err(js_error)?;
        self.last_camera = Some(uniforms);
        Ok(FrameMetrics {
            draw_calls: resident.draws,
            submits: 1,
            upload_bytes: std::mem::size_of_val(view) as u64,
            gpu_ms: None,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> Result<()> {
        // Graphics work stays on the GPU timeline. Destroyed browser objects
        // retain physical backing until their previous operations complete.
        Ok(())
    }

    fn capture(&mut self) -> Result<Vec<u8>> {
        Err(Error::Invalid("use browser canvas capture"))
    }
}

#[wasm_bindgen]
pub struct WebProbe {
    gpu: BrowserGpu,
    device: crate::Device<WebGpu>,
    handle: crate::handles::MeshHandle,
    extent: [u32; 2],
}

#[wasm_bindgen]
impl WebProbe {
    pub async fn open(
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
        fixture: Option<String>,
    ) -> std::result::Result<WebProbe, JsValue> {
        let backend = WebGpu::open(canvas).await?;
        let gpu = backend.tracking_handle();
        let mut device =
            crate::Device::new(backend, GraphicsMemoryBudget::potato()).map_err(to_js)?;
        let extent = [width, height];
        device.configure(extent, Output::Present).map_err(to_js)?;
        let workload = match fixture {
            Some(contents) => serde_json::from_str::<Workload>(&contents).map_err(to_js)?,
            None => fixtures::scene(false, 3),
        };
        let handle = device.upload(&workload).map_err(to_js)?;
        Ok(Self {
            gpu,
            device,
            handle,
            extent,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> std::result::Result<(), JsValue> {
        let extent = [width, height];
        if extent != self.extent {
            self.device
                .configure(extent, Output::Present)
                .map_err(to_js)?;
            self.extent = extent;
        }
        Ok(())
    }

    pub fn frame(&mut self, orbit: f32) -> std::result::Result<(), JsValue> {
        self.device
            .draw(self.handle, &fixtures::camera(self.extent, orbit))
            .map_err(to_js)?;
        Ok(())
    }

    pub async fn wait_for_gpu(&self) -> std::result::Result<(), JsValue> {
        JsFuture::from(self.gpu.wait_async()).await?;
        Ok(())
    }

    pub fn adapter(&self) -> String {
        self.device.info().name.clone()
    }
}

#[wasm_bindgen]
pub struct WebStartup {
    gpu: BrowserGpu,
    device: crate::product::ProductDevice<WebGpu>,
    layout: Option<([u32; 2], f64)>,
    revision: u64,
}

#[wasm_bindgen]
impl WebStartup {
    pub async fn open(canvas: HtmlCanvasElement) -> std::result::Result<Self, JsValue> {
        let backend = WebGpu::open(canvas).await?;
        let gpu = backend.tracking_handle();
        let device = crate::product::ProductDevice::new(backend, GraphicsMemoryBudget::potato())
            .map_err(to_js)?;
        Ok(Self {
            gpu,
            device,
            layout: None,
            revision: 0,
        })
    }

    pub fn redraw(
        &mut self,
        width: u32,
        height: u32,
        scale: f64,
    ) -> std::result::Result<(), JsValue> {
        let extent = [width, height];
        if extent.contains(&0) || !scale.is_finite() || scale <= 0. {
            return Err(to_js(Error::Invalid("invalid startup extent or scale")));
        }
        if self
            .layout
            .is_none_or(|(configured, _)| configured != extent)
        {
            self.device
                .configure(extent, Output::Present)
                .map_err(to_js)?;
        }
        if self.layout != Some((extent, scale)) {
            self.layout = Some((extent, scale));
            self.revision += 1;
        }
        crate::product::startup::draw(&mut self.device, extent, scale, self.revision)
            .map_err(to_js)?;
        Ok(())
    }

    pub async fn wait_for_gpu(&self) -> std::result::Result<(), JsValue> {
        JsFuture::from(self.gpu.wait_async()).await?;
        Ok(())
    }

    pub fn adapter(&self) -> String {
        self.device.info().name.clone()
    }

    pub fn device_lost(&self) -> bool {
        self.gpu.lost()
    }

    pub fn loss_signal(&self) -> Promise {
        self.gpu.loss_signal()
    }
}

fn js_error(error: JsValue) -> Error {
    let message = js_sys::Reflect::get(&error, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string());
    Error::Backend(
        message
            .or_else(|| error.as_string())
            .unwrap_or_else(|| "WebGPU operation failed".into()),
    )
}

fn to_js(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}
