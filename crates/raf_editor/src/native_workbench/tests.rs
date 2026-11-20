use super::metrics::{estimated_capacity_fps, ledger_budget_bytes};
use super::*;

#[test]
fn agent_virtual_window_does_not_rebuild_for_every_scroll_pixel() {
    assert_eq!(next_agent_scroll_projection(0.0, 1.0), None);
    assert_eq!(next_agent_scroll_projection(0.0, 95.0), None);
    assert_eq!(next_agent_scroll_projection(0.0, 96.0), Some(96.0));
    assert_eq!(next_agent_scroll_projection(96.0, 40.0), None);
    assert_eq!(next_agent_scroll_projection(96.0, 0.0), Some(0.0));
}

#[test]
fn estimated_capacity_uses_the_slowest_measured_stage() {
    assert_eq!(estimated_capacity_fps(3.1, Some(0.2)), 323);
    assert_eq!(estimated_capacity_fps(1.0, Some(4.0)), 250);
    assert_eq!(estimated_capacity_fps(4.0, None), 250);
}

#[test]
fn estimated_capacity_is_unknown_without_positive_samples() {
    assert_eq!(estimated_capacity_fps(0.0, None), 0);
    assert_eq!(estimated_capacity_fps(f32::NAN, Some(f32::INFINITY)), 0);
}

#[test]
fn ledger_budget_follows_the_active_render_policy() {
    use raf_render::api_graphic_basic::GraphicsMemoryBudget;

    assert_eq!(
        ledger_budget_bytes(RenderExecutionPolicy::Auto),
        GraphicsMemoryBudget::potato().gpu_bytes
    );
    assert_eq!(
        ledger_budget_bytes(RenderExecutionPolicy::CpuOnly),
        GraphicsMemoryBudget::potato().gpu_bytes
    );
    assert_eq!(
        ledger_budget_bytes(RenderExecutionPolicy::GpuPreferred),
        GraphicsMemoryBudget::desktop().gpu_bytes
    );
}
