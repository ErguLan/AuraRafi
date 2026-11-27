//! Select a compiled native executor once while opening a device.

use std::{fmt, str::FromStr};

use crate::{
    product::Layer, AdapterInfo, Backend, Error, Frame, FrameMetrics, FramePlan, Output,
    ResourcePatch, Result, Workload,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeApi {
    Vulkan,
    Dx12,
    Metal,
}

impl fmt::Display for NativeApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Vulkan => "vulkan",
            Self::Dx12 => "dx12",
            Self::Metal => "metal",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeRequest {
    #[default]
    Auto,
    Exact(NativeApi),
}

impl FromStr for NativeRequest {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "auto" => Self::Auto,
            "vulkan" => Self::Exact(NativeApi::Vulkan),
            "dx12" => Self::Exact(NativeApi::Dx12),
            "metal" => Self::Exact(NativeApi::Metal),
            _ => return Err(Error::Invalid("expected auto, vulkan, dx12 or metal")),
        })
    }
}

impl NativeApi {
    pub const fn supports(self, output: Output) -> bool {
        match self {
            Self::Vulkan => cfg!(all(feature = "vulkan", not(target_arch = "wasm32"))),
            Self::Dx12 => cfg!(all(feature = "dx12", windows)),
            Self::Metal => {
                cfg!(all(feature = "metal", target_vendor = "apple"))
                    && (matches!(output, Output::Capture)
                        || cfg!(any(target_os = "macos", target_os = "ios")))
            }
        }
    }
}

const ORDER: [NativeApi; 3] = if cfg!(windows) {
    [NativeApi::Dx12, NativeApi::Vulkan, NativeApi::Metal]
} else if cfg!(target_vendor = "apple") {
    [NativeApi::Metal, NativeApi::Vulkan, NativeApi::Dx12]
} else {
    [NativeApi::Vulkan, NativeApi::Metal, NativeApi::Dx12]
};

macro_rules! native_executors {
    ($($(#[$cfg:meta])* $api:ident: $module:ident),+ $(,)?) => {
        enum Executor {
            $($(#[$cfg])* $api(super::$module::$api),)+
        }

        enum Resident {
            $($(#[$cfg])* $api(super::$module::Resident),)+
        }

        /// A retained native resource. Its representation stays private.
        pub struct NativeResident(Resident);

        /// Created once for a capture target or a window.
        pub struct NativeBackend(Executor);

        impl NativeBackend {
            pub fn api(&self) -> NativeApi {
                match self.0 {
                    $($(#[$cfg])* Executor::$api(_) => NativeApi::$api,)+
                }
            }

            pub fn headless(request: NativeRequest) -> Result<Self> {
                open(request, Output::Capture, |api| match api {
                    $($(#[$cfg])* NativeApi::$api => super::$module::$api::headless().map(Executor::$api),)+
                    #[allow(unreachable_patterns)]
                    _ => Err(Error::Unsupported("native executor missing from this build")),
                }).map(Self)
            }
        }

        impl Backend for NativeBackend {
            type Resident = NativeResident;

            fn info(&self) -> &AdapterInfo {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.info(),)+ }
            }

            fn supports_shadows(&self) -> bool {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.supports_shadows(),)+ }
            }

            fn shadow_bytes(&self) -> u64 {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.shadow_bytes(),)+ }
            }

            fn supports_scene_filter(&self) -> bool {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.supports_scene_filter(),)+ }
            }

            fn supports_layers(&self) -> bool {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.supports_layers(),)+ }
            }

            fn supports_frame_instance_patches(&self) -> bool {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.supports_frame_instance_patches(),)+ }
            }

            fn filter_bytes(&self) -> u64 {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.filter_bytes(),)+ }
            }

            fn configure(&mut self, plan: &FramePlan) -> Result<()> {
                match &mut self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.configure(plan),)+ }
            }

            fn upload(&mut self, workload: &Workload) -> Result<NativeResident> {
                match &mut self.0 {
                    $($(#[$cfg])* Executor::$api(backend) => backend.upload(workload).map(Resident::$api).map(NativeResident),)+
                }
            }

            fn patch_row_alignment(&self) -> u64 {
                match &self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.patch_row_alignment(),)+ }
            }

            unsafe fn patch(&mut self, resident: &mut NativeResident, patch: ResourcePatch<'_>) -> Result<()> {
                match (&mut self.0, &mut resident.0) {
                    $($(#[$cfg])* (Executor::$api(backend), Resident::$api(resource)) => unsafe { backend.patch(resource, patch) },)+
                    #[allow(unreachable_patterns)]
                    _ => Err(Error::Invalid("resident packet belongs to another API")),
                }
            }

            fn draw(&mut self, resident: &NativeResident, plan: &FramePlan, frame: &Frame) -> Result<FrameMetrics> {
                match (&mut self.0, &resident.0) {
                    $($(#[$cfg])* (Executor::$api(backend), Resident::$api(resource)) => backend.draw(resource, plan, frame),)+
                    #[allow(unreachable_patterns)]
                    _ => Err(Error::Invalid("resident packet belongs to another API")),
                }
            }

            fn draw_layers(
                &mut self,
                layers: &[Layer<'_, NativeResident>],
                plan: &FramePlan,
                frame: &Frame,
            ) -> Result<FrameMetrics> {
                match &mut self.0 {
                    $($(#[$cfg])* Executor::$api(backend) => {
                        let mut translated = Vec::with_capacity(layers.len());
                        for layer in layers {
                            match &layer.resident.0 {
                                Resident::$api(resident) => translated.push(Layer {
                                    resident,
                                    scene: layer.scene,
                                }),
                                #[allow(unreachable_patterns)]
                                _ => return Err(Error::Invalid("resident packet belongs to another API")),
                            }
                        }
                        backend.draw_layers(&translated, plan, frame)
                    })+
                }
            }

            fn wait(&mut self) -> Result<()> {
                match &mut self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.wait(),)+ }
            }

            fn capture(&mut self) -> Result<Vec<u8>> {
                match &mut self.0 { $($(#[$cfg])* Executor::$api(backend) => backend.capture(),)+ }
            }
        }
    };
}

native_executors!(
    #[cfg(all(feature = "vulkan", not(target_arch = "wasm32")))]
    Vulkan: vulkan,
    #[cfg(all(feature = "dx12", windows))]
    Dx12: dx12,
    #[cfg(all(feature = "metal", target_vendor = "apple"))]
    Metal: metal,
);

impl NativeBackend {
    #[cfg(any(
        all(feature = "vulkan", not(target_arch = "wasm32")),
        all(feature = "dx12", windows),
        all(feature = "metal", target_vendor = "apple")
    ))]
    /// # Safety
    /// Both raw handles must remain valid until the backend drops.
    pub unsafe fn window(
        request: NativeRequest,
        _display: raw_window_handle::RawDisplayHandle,
        window: raw_window_handle::RawWindowHandle,
    ) -> Result<Self> {
        open(request, Output::Present, |api| match api {
            #[cfg(all(feature = "vulkan", not(target_arch = "wasm32")))]
            NativeApi::Vulkan => {
                unsafe { super::vulkan::Vulkan::window(_display, window) }.map(Executor::Vulkan)
            }
            #[cfg(all(feature = "dx12", windows))]
            NativeApi::Dx12 => unsafe { super::dx12::Dx12::window(window) }.map(Executor::Dx12),
            #[cfg(all(feature = "metal", target_vendor = "apple"))]
            NativeApi::Metal => unsafe { super::metal::Metal::window(window) }.map(Executor::Metal),
            _ => Err(Error::Unsupported(
                "window presentation on the requested native API",
            )),
        })
        .map(Self)
    }
}

fn open<T>(
    request: NativeRequest,
    output: Output,
    mut start: impl FnMut(NativeApi) -> Result<T>,
) -> Result<T> {
    if let NativeRequest::Exact(api) = request {
        if !api.supports(output) {
            return Err(Error::Backend(format!(
                "{api} has no {output:?} executor in this build"
            )));
        }
        return start(api).map_err(|cause| Error::Backend(format!("open {api}: {cause}")));
    }
    let mut failures = Vec::new();
    let order = if cfg!(windows) && output == Output::Present {
        [NativeApi::Vulkan, NativeApi::Dx12, NativeApi::Metal]
    } else {
        ORDER
    };
    for api in order.into_iter().filter(|api| api.supports(output)) {
        match start(api) {
            Ok(executor) => return Ok(executor),
            Err(cause) => failures.push(format!("{api}: {cause}")),
        }
    }
    Err(Error::Backend(if failures.is_empty() {
        format!("no native {output:?} executor in this build")
    } else {
        format!(
            "native {output:?} initialization failed ({})",
            failures.join("; ")
        )
    }))
}
