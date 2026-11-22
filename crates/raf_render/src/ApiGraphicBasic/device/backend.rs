//! Backend selection for the scene device.
//!
//! The device has exactly two execution owners: the CPU recovery rasterizer and
//! the AGB compositor. Selecting the native owner never opens a graphics API;
//! the compositor already owns the device and consumes the retained frame.

#[cfg(not(target_arch = "wasm32"))]
use raf_agb::backends::native::NativeApi;
use raf_agb::Output;
use raf_core::config::RenderExecutionPolicy;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeApi {
    Vulkan,
    Dx12,
    Metal,
}

#[cfg(target_arch = "wasm32")]
impl NativeApi {
    /// The browser host opens WebGPU directly; the scene device never owns a
    /// native surface on wasm and therefore prefers no native API.
    pub const fn supports(self, _output: Output) -> bool {
        false
    }
}

#[cfg(target_arch = "wasm32")]
impl std::fmt::Display for NativeApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Vulkan => "vulkan",
            Self::Dx12 => "dx12",
            Self::Metal => "metal",
        })
    }
}

use crate::api_graphic_basic::capabilities::{
    GraphicsAdapterPreference, GraphicsBackendId, GraphicsCapabilities, GraphicsMemoryBudget,
};

/// Conservative texture ceiling promised by the scene contract itself.
///
/// The compositor reports the limits of the adapter it actually opened; the
/// scene device allocates no texture and cannot measure hardware.
const NATIVE_CONTRACT_MAX_TEXTURE_DIMENSION: u32 = 16_384;
/// The scene device allocates no buffer, so it reports no measured ceiling.
const NATIVE_CONTRACT_MAX_BUFFER_SIZE: u64 = 0;

/// Native API this build prefers for presentation, or `None` when the build
/// compiles no native executor that can present.
pub fn preferred_native_api() -> Option<NativeApi> {
    native_presentation_order()
        .into_iter()
        .find(|api| api.supports(Output::Present))
}

const fn native_presentation_order() -> [NativeApi; 3] {
    if cfg!(windows) {
        [NativeApi::Vulkan, NativeApi::Dx12, NativeApi::Metal]
    } else if cfg!(target_vendor = "apple") {
        [NativeApi::Metal, NativeApi::Vulkan, NativeApi::Dx12]
    } else {
        [NativeApi::Vulkan, NativeApi::Metal, NativeApi::Dx12]
    }
}

/// Supported execution backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicBackendType {
    /// CPU recovery/software rendering.
    CpuSoftware,
    /// Retained scene handoff for the AGB compositor.
    ///
    /// The device performs no graphics work in this mode: no device, target,
    /// upload or draw belongs to it. The compositor owns all of those.
    Native(NativeApi),
}

impl BasicBackendType {
    pub const fn id(self) -> GraphicsBackendId {
        match self {
            Self::CpuSoftware => GraphicsBackendId::CpuSoftware,
            Self::Native(NativeApi::Vulkan) => GraphicsBackendId::Vulkan,
            Self::Native(NativeApi::Dx12) => GraphicsBackendId::DirectX12,
            Self::Native(NativeApi::Metal) => GraphicsBackendId::Metal,
        }
    }

    /// Whether a graphics API draws this backend's frames.
    pub const fn is_native(self) -> bool {
        matches!(self, Self::Native(_))
    }

    pub const fn is_cpu(self) -> bool {
        matches!(self, Self::CpuSoftware)
    }

    /// Editor-facing alias kept for the existing diagnostics.
    pub const fn is_gpu_active(self) -> bool {
        self.is_native()
    }

    /// Stable label used by diagnostics and serialized snapshots.
    pub const fn label(self) -> &'static str {
        match self {
            Self::CpuSoftware => "cpu_software",
            Self::Native(api) => native_api_label(api),
        }
    }

    /// Capabilities this device can promise without owning a queue.
    ///
    /// A native handoff reports the API identity and hardware presence only.
    /// Compute, indirect and timestamp support stay unset until the compositor
    /// queries the adapter it opened.
    pub const fn capabilities(self) -> GraphicsCapabilities {
        match self {
            Self::CpuSoftware => GraphicsCapabilities::cpu(),
            Self::Native(_) => GraphicsCapabilities {
                backend: self.id(),
                gpu_hardware: true,
                compute: false,
                indirect_draw: false,
                timestamp_queries: false,
                max_texture_dimension: NATIVE_CONTRACT_MAX_TEXTURE_DIMENSION,
                max_buffer_size: NATIVE_CONTRACT_MAX_BUFFER_SIZE,
            },
        }
    }
}

const fn native_api_label(api: NativeApi) -> &'static str {
    match api {
        NativeApi::Vulkan => "vulkan",
        NativeApi::Dx12 => "dx12",
        NativeApi::Metal => "metal",
    }
}

impl Serialize for BasicBackendType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.label())
    }
}

impl<'de> Deserialize<'de> for BasicBackendType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let label = String::deserialize(deserializer)?;
        match label.as_str() {
            "cpu_software" => Ok(Self::CpuSoftware),
            "vulkan" => Ok(Self::Native(NativeApi::Vulkan)),
            "dx12" => Ok(Self::Native(NativeApi::Dx12)),
            "metal" => Ok(Self::Native(NativeApi::Metal)),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["cpu_software", "vulkan", "dx12", "metal"],
            )),
        }
    }
}

/// Configuration settings for device initialization.
#[derive(Debug, Clone)]
pub struct BasicDeviceConfig {
    /// Allow the retained native handoff instead of CPU software rendering.
    pub allow_gpu: bool,
    /// Force CPU software rendering regardless of the available native APIs.
    pub force_cpu: bool,
    /// Backend-neutral memory and frame budget.
    pub memory_budget: GraphicsMemoryBudget,
    /// Adapter preference forwarded to the compositor that opens the device.
    pub adapter_preference: GraphicsAdapterPreference,
}

impl Default for BasicDeviceConfig {
    fn default() -> Self {
        Self {
            allow_gpu: true,
            force_cpu: false,
            memory_budget: GraphicsMemoryBudget::default(),
            adapter_preference: GraphicsAdapterPreference::default(),
        }
    }
}

impl BasicDeviceConfig {
    /// Build a device config from the engine render execution policy.
    pub fn from_render_policy(policy: RenderExecutionPolicy) -> Self {
        match policy {
            RenderExecutionPolicy::Auto | RenderExecutionPolicy::GpuPreferred => Self {
                allow_gpu: true,
                force_cpu: false,
                memory_budget: if matches!(policy, RenderExecutionPolicy::GpuPreferred) {
                    GraphicsMemoryBudget::desktop()
                } else {
                    GraphicsMemoryBudget::potato()
                },
                adapter_preference: if matches!(policy, RenderExecutionPolicy::GpuPreferred) {
                    GraphicsAdapterPreference::HighPerformance
                } else {
                    GraphicsAdapterPreference::LowPower
                },
            },
            RenderExecutionPolicy::CpuOnly => Self {
                allow_gpu: false,
                force_cpu: true,
                memory_budget: GraphicsMemoryBudget::potato(),
                adapter_preference: GraphicsAdapterPreference::LowPower,
            },
        }
    }

    /// Backend this configuration resolves to without opening a device.
    ///
    /// The native API is the preferred one compiled for this target. The
    /// compositor may open a different compiled API; it reports its own
    /// `raf_agb` adapter identity.
    pub fn resolved_backend(&self) -> BasicBackendType {
        if self.force_cpu || !self.allow_gpu {
            return BasicBackendType::CpuSoftware;
        }
        preferred_native_api()
            .map(BasicBackendType::Native)
            .unwrap_or(BasicBackendType::CpuSoftware)
    }
}
