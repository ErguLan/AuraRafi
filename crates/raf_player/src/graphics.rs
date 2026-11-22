//! Graphics policy shared by the windowed, headless, and streaming hosts.
//!
//! One mapping keeps the AGB product device, the retained UI hosts, and the
//! scene device on the same execution policy and memory budget. The budget is
//! raised to cover a whole physical target so a large capture can upload and
//! retain its composed frame without hitting the default staging ceiling.

use raf_core::config::RenderExecutionPolicy;
use raf_render::api_graphic_basic::capabilities::GraphicsMemoryBudget;
use raf_render::api_graphic_basic::device::BasicDeviceConfig;
use raf_render::api_graphic_basic::ui_surface::NativeUiWindowConfig;

/// Full RGBA targets the device budget must be able to retain at once: the
/// presented composition, its prepared replacement, the retained scene
/// handoff, and the capture readback.
const RETAINED_TARGETS: u64 = 4;

/// Bytes of one tightly packed RGBA8 target of `size`.
fn target_bytes(size: [u32; 2]) -> u64 {
    u64::from(size[0])
        .saturating_mul(u64::from(size[1]))
        .saturating_mul(4)
}

/// Policy budget with a floor that covers the physical target actually drawn.
///
/// The potato and desktop profiles are tuned for a typical window. A headless
/// capture or a high-DPI present can exceed their staging and frame-upload
/// ceilings with a single composed frame, so the target size raises those
/// floors instead of failing the upload after the device opened.
pub fn memory_budget(policy: RenderExecutionPolicy, size: [u32; 2]) -> GraphicsMemoryBudget {
    let mut budget = BasicDeviceConfig::from_render_policy(policy).memory_budget;
    let target = target_bytes(size);
    budget.gpu_bytes = budget
        .gpu_bytes
        .max(target.saturating_mul(RETAINED_TARGETS));
    budget.staging_bytes = budget.staging_bytes.max(target);
    budget.frame_upload_bytes = budget.frame_upload_bytes.max(target);
    budget
}

/// Host configuration for one native AGB target of `size`.
pub fn window_config(policy: RenderExecutionPolicy, size: [u32; 2]) -> NativeUiWindowConfig {
    let device = BasicDeviceConfig::from_render_policy(policy);
    NativeUiWindowConfig {
        adapter_preference: device.adapter_preference,
        memory_budget: memory_budget(policy, size),
        ..NativeUiWindowConfig::default()
    }
}

/// Scene device configuration for the same policy and physical target.
///
/// `CpuOnly` resolves to `CpuSoftware` with `allow_gpu = false`, which is the
/// recovery compositor path; native policies retain the scene frame for the
/// host compositor instead of opening a second device.
pub fn device_config(policy: RenderExecutionPolicy, size: [u32; 2]) -> BasicDeviceConfig {
    let mut config = BasicDeviceConfig::from_render_policy(policy);
    config.memory_budget = memory_budget(policy, size);
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_targets_raise_the_upload_floor() {
        let base = BasicDeviceConfig::from_render_policy(RenderExecutionPolicy::Auto).memory_budget;
        let small = memory_budget(RenderExecutionPolicy::Auto, [640, 360]);
        let large = memory_budget(RenderExecutionPolicy::Auto, [3840, 2160]);
        let target = 3840 * 2160 * 4;
        assert_eq!(small.gpu_bytes, base.gpu_bytes);
        assert_eq!(small.staging_bytes, base.staging_bytes);
        assert!(large.staging_bytes >= target);
        assert!(large.frame_upload_bytes >= target);
        assert!(large.gpu_bytes >= target * RETAINED_TARGETS);
    }

    #[test]
    fn cpu_only_resolves_to_the_recovery_device() {
        let config = device_config(RenderExecutionPolicy::CpuOnly, [1280, 720]);
        assert!(!config.allow_gpu);
        assert!(config.force_cpu);
    }
}
