pub mod grid;
pub mod recipes;

pub mod cad_surface;
pub mod cad_surface_host;
pub mod canvas_presenter;
pub mod capabilities;
pub mod command_list;
pub mod device;
pub mod editor_compositor;
pub mod frame_scheduler;
pub mod handles;
pub mod mesh;
pub mod pipeline;
pub mod resource_registry;
pub mod ui_surface;

pub use canvas_presenter::CanvasTargetRect;
pub use capabilities::{
    GraphicsAdapterPreference, GraphicsBackendId, GraphicsCapabilities, GraphicsMemoryBudget,
};
pub use editor_compositor::{
    EditorCanvasLayer, EditorComposedFrame, EditorCompositorMetrics, EditorUiLayer,
    NativeEditorCompositor,
};
pub use frame_scheduler::{
    DynamicResolutionController, FrameActivity, FrameInvalidation, FramePacingBudget,
    FramePacingProfile, FramePermit, FrameScheduler, FrameSchedulerMetrics,
};
pub use handles::{
    BufferHandle, GraphicsHandle, MaterialHandle, MeshHandle, PipelineHandle, SamplerHandle,
    SurfaceHandle, TextureHandle,
};
pub use resource_registry::{
    BufferRegistry, MaterialRegistry, MeshRegistry, PipelineRegistry, ResourceAdmission,
    ResourceArena, ResourceArenaMetrics, ResourceBudgetError, SamplerRegistry, TextureRegistry,
};
