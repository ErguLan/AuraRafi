//! Backend-neutral capabilities and resource budgets.

use serde::{Deserialize, Serialize};

/// Identifies the implementation behind an ApiGraphicBasic device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsBackendId {
    Wgpu,
    CpuSoftware,
    DirectX12,
    Vulkan,
    Metal,
}

/// Adapter features exposed without leaking a backend API into upper layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsCapabilities {
    pub backend: GraphicsBackendId,
    pub gpu_hardware: bool,
    pub compute: bool,
    pub indirect_draw: bool,
    pub timestamp_queries: bool,
    pub max_texture_dimension: u32,
    pub max_buffer_size: u64,
}

impl GraphicsCapabilities {
    pub const fn cpu() -> Self {
        Self {
            backend: GraphicsBackendId::CpuSoftware,
            gpu_hardware: false,
            compute: false,
            indirect_draw: false,
            timestamp_queries: false,
            max_texture_dimension: 16_384,
            max_buffer_size: 0,
        }
    }

    pub const fn wgpu(max_texture_dimension: u32, max_buffer_size: u64) -> Self {
        Self {
            backend: GraphicsBackendId::Wgpu,
            gpu_hardware: true,
            compute: true,
            indirect_draw: true,
            timestamp_queries: false,
            max_texture_dimension,
            max_buffer_size,
        }
    }
}

/// Explicit memory and frame budgets carried by the graphics device.
///
/// These values are policy metadata in this first foundation. Cache eviction
/// and upload enforcement will consume the same contract in later updates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphicsMemoryBudget {
    pub gpu_bytes: u64,
    pub staging_bytes: u64,
    pub mesh_cache_entries: u32,
    pub texture_cache_entries: u32,
    pub frame_upload_bytes: u64,
    pub target_frame_ms: f32,
}

impl GraphicsMemoryBudget {
    pub const fn potato() -> Self {
        Self {
            gpu_bytes: 256 * 1024 * 1024,
            staging_bytes: 16 * 1024 * 1024,
            mesh_cache_entries: 4_096,
            texture_cache_entries: 512,
            frame_upload_bytes: 8 * 1024 * 1024,
            target_frame_ms: 33.3,
        }
    }

    pub const fn desktop() -> Self {
        Self {
            gpu_bytes: 1 * 1024 * 1024 * 1024,
            staging_bytes: 64 * 1024 * 1024,
            mesh_cache_entries: 16_384,
            texture_cache_entries: 2_048,
            frame_upload_bytes: 32 * 1024 * 1024,
            target_frame_ms: 16.6,
        }
    }
}

impl Default for GraphicsMemoryBudget {
    fn default() -> Self {
        Self::potato()
    }
}

/// Adapter selection policy independent from any one backend API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsAdapterPreference {
    LowPower,
    HighPerformance,
}

impl Default for GraphicsAdapterPreference {
    fn default() -> Self {
        Self::LowPower
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potato_budget_is_bounded_and_gpu_neutral() {
        let budget = GraphicsMemoryBudget::potato();
        assert_eq!(budget.gpu_bytes, 256 * 1024 * 1024);
        assert!(budget.frame_upload_bytes <= budget.staging_bytes);
        assert_eq!(
            GraphicsCapabilities::cpu().backend,
            GraphicsBackendId::CpuSoftware
        );
    }
}
