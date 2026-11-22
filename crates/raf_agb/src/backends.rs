pub mod recorder;

#[cfg(any(
    all(feature = "vulkan", not(target_arch = "wasm32")),
    all(feature = "dx12", windows),
    all(feature = "metal", target_vendor = "apple"),
))]
pub mod native;

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "vulkan", feature = "dx12", feature = "metal")
))]
mod shaders;

#[cfg(all(feature = "dx12", windows))]
pub mod dx12;
#[cfg(all(feature = "metal", target_vendor = "apple"))]
pub mod metal;
#[cfg(all(feature = "vulkan", not(target_arch = "wasm32")))]
pub mod vulkan;
#[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
pub mod webgpu;
