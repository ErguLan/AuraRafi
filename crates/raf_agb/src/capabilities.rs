//! Backend-neutral capabilities and resource budgets.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::memory_ledger::GraphicsResourceFamily;

/// Identifies the implementation behind an ApiGraphicBasic device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsBackendId {
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
}

/// Explicit memory and frame budgets carried by the graphics device.
///
/// Mesh residency, UI image residency, and frame-upload checks consume this
/// contract. [`GraphicsMemoryBudget::family_budgets`] splits `gpu_bytes` into
/// the shares used by the cross-family graphics memory ledger.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphicsMemoryBudget {
    pub gpu_bytes: u64,
    pub staging_bytes: u64,
    pub mesh_cache_entries: u32,
    pub texture_cache_entries: u32,
    pub frame_upload_bytes: u64,
    pub target_frame_ms: f32,
}

// Potato-safe shares of `gpu_bytes` reserved for each resource family.
//
// Percentages are integers and are applied with 128-bit intermediates, so the
// split cannot overflow. Truncation can only leave unassigned headroom, never
// exceed the total.
const BUFFER_SHARE_PERCENT: u32 = 15;
const TEXTURE_SHARE_PERCENT: u32 = 25;
const SAMPLER_SHARE_PERCENT: u32 = 2;
const PIPELINE_SHARE_PERCENT: u32 = 5;
const MESH_SHARE_PERCENT: u32 = 45;
const MATERIAL_SHARE_PERCENT: u32 = 5;
const STAGING_SHARE_PERCENT: u32 = 2;
const TRANSIENT_SHARE_PERCENT: u32 = 1;

const fn share_bytes(total_bytes: u64, percent: u32) -> u64 {
    ((total_bytes as u128) * (percent as u128) / 100u128) as u64
}

/// A family budget split that cannot be used safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsBudgetError {
    /// The configured total left no usable bytes for this family.
    ZeroFamily {
        family: GraphicsResourceFamily,
        total_bytes: u64,
    },
    /// The family shares add up to more than the configured total.
    SumExceedsTotal { sum_bytes: u64, total_bytes: u64 },
}

impl fmt::Display for GraphicsBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFamily {
                family,
                total_bytes,
            } => write!(
                formatter,
                "graphics budget leaves zero bytes for {} out of {} total",
                family.label(),
                total_bytes
            ),
            Self::SumExceedsTotal {
                sum_bytes,
                total_bytes,
            } => write!(
                formatter,
                "graphics family budgets sum to {} bytes, above the {} byte total",
                sum_bytes, total_bytes
            ),
        }
    }
}

impl std::error::Error for GraphicsBudgetError {}

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

    /// Split `gpu_bytes` into one hard budget per resource family.
    ///
    /// The order matches [`GraphicsResourceFamily::ALL`] so callers can index
    /// the result by [`GraphicsResourceFamily::index`].
    pub const fn family_budgets(&self) -> [(GraphicsResourceFamily, u64); 8] {
        [
            (
                GraphicsResourceFamily::Buffer,
                share_bytes(self.gpu_bytes, BUFFER_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Texture,
                share_bytes(self.gpu_bytes, TEXTURE_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Sampler,
                share_bytes(self.gpu_bytes, SAMPLER_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Pipeline,
                share_bytes(self.gpu_bytes, PIPELINE_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Mesh,
                share_bytes(self.gpu_bytes, MESH_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Material,
                share_bytes(self.gpu_bytes, MATERIAL_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Staging,
                share_bytes(self.gpu_bytes, STAGING_SHARE_PERCENT),
            ),
            (
                GraphicsResourceFamily::Transient,
                share_bytes(self.gpu_bytes, TRANSIENT_SHARE_PERCENT),
            ),
        ]
    }

    /// Confirm that every family can admit resources and that the split stays
    /// inside `gpu_bytes`. Tiny custom budgets can fail this on purpose.
    pub fn validate(&self) -> Result<(), GraphicsBudgetError> {
        let mut sum_bytes = 0_u64;
        for (family, bytes) in self.family_budgets() {
            if bytes == 0 {
                return Err(GraphicsBudgetError::ZeroFamily {
                    family,
                    total_bytes: self.gpu_bytes,
                });
            }
            sum_bytes = sum_bytes.saturating_add(bytes);
        }
        if sum_bytes > self.gpu_bytes {
            return Err(GraphicsBudgetError::SumExceedsTotal {
                sum_bytes,
                total_bytes: self.gpu_bytes,
            });
        }
        Ok(())
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

    #[test]
    fn family_budgets_sum_within_total_for_potato_and_desktop() {
        for budget in [
            GraphicsMemoryBudget::potato(),
            GraphicsMemoryBudget::desktop(),
        ] {
            let total: u64 = budget
                .family_budgets()
                .into_iter()
                .map(|(_, bytes)| bytes)
                .sum();
            assert!(
                total <= budget.gpu_bytes,
                "family split {total} exceeds {}",
                budget.gpu_bytes
            );
            assert!(budget.validate().is_ok());
        }
    }

    #[test]
    fn family_budgets_keep_the_declared_shares() {
        let budget = GraphicsMemoryBudget::potato();
        let families = budget.family_budgets();

        assert_eq!(
            families[GraphicsResourceFamily::Mesh.index()],
            (
                GraphicsResourceFamily::Mesh,
                budget.gpu_bytes * MESH_SHARE_PERCENT as u64 / 100
            )
        );
        assert_eq!(
            families[GraphicsResourceFamily::Texture.index()],
            (
                GraphicsResourceFamily::Texture,
                budget.gpu_bytes * TEXTURE_SHARE_PERCENT as u64 / 100
            )
        );
        assert_eq!(
            families[GraphicsResourceFamily::Buffer.index()],
            (
                GraphicsResourceFamily::Buffer,
                budget.gpu_bytes * BUFFER_SHARE_PERCENT as u64 / 100
            )
        );
    }

    #[test]
    fn zero_total_budget_reports_the_empty_family() {
        let budget = GraphicsMemoryBudget {
            gpu_bytes: 0,
            ..GraphicsMemoryBudget::potato()
        };
        assert_eq!(
            budget.validate(),
            Err(GraphicsBudgetError::ZeroFamily {
                family: GraphicsResourceFamily::Buffer,
                total_bytes: 0,
            })
        );
    }
}
