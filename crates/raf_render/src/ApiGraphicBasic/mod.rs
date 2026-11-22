pub mod grid;
pub mod recipes;

mod cad_board_surface;
pub mod asset_ingress;
pub mod cad_surface;
pub mod cad_surface_host;
pub mod canvas_presenter;
pub mod command_list;
pub mod device;
#[cfg(not(target_arch = "wasm32"))]
pub mod editor_compositor;
pub mod line_projection;
pub mod mesh;
pub mod pipeline;
pub mod product_composition;
pub mod ui_surface;

pub use raf_agb::{capabilities, frame_graph, frame_scheduler, handles, memory_ledger, resource_registry};

pub use asset_ingress::{
    TextureIngress, TextureIngressEntry, TextureIngressKey, TextureIngressMetrics,
    DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES, TEXTURE_THUMBNAIL_MAX_DIMENSION,
};
pub use canvas_presenter::CanvasTargetRect;
pub use capabilities::{
    GraphicsAdapterPreference, GraphicsBackendId, GraphicsBudgetError, GraphicsCapabilities,
    GraphicsMemoryBudget,
};
pub use device::{SceneFrameCapture, SceneFrameMetrics};
#[cfg(not(target_arch = "wasm32"))]
pub use editor_compositor::{
    EditorCanvasLayer, EditorComposedFrame, EditorCompositorMetrics, EditorUiLayer,
    NativeEditorCompositor,
};
pub use frame_graph::{
    AliasSlot, CompiledFrameGraph, FrameGraph, FrameGraphError, GraphBarrier, GraphTextureDesc,
    GraphTextureFormat, PassDesc, PassKind, ResourceAccess, ResourceState, TextureUsage,
};
pub use frame_scheduler::{
    DynamicResolutionController, FrameActivity, FrameInvalidation, FramePacingBudget,
    FramePacingProfile, FramePermit, FrameScheduler, FrameSchedulerMetrics,
};
pub use handles::{
    BufferHandle, GraphicsHandle, MaterialHandle, MeshHandle, PipelineHandle, SamplerHandle,
    SurfaceHandle, TextureHandle,
};
pub use memory_ledger::{
    GraphicsFamilyLedger, GraphicsFamilySnapshot, GraphicsLedgerSnapshot, GraphicsResourceFamily,
    GraphicsResourceLedger, LedgerAdmission, LedgerError, ProcessMemoryLedger,
};
pub use product_composition::{
    prepare_product_scene, scene_frame_uniforms, ProductFrameBuilder, PRODUCT_SCENE_PACKET_KEY,
};
pub use resource_registry::{
    BufferRegistry, MaterialRegistry, MeshRegistry, PipelineRegistry, ResourceAdmission,
    ResourceArena, ResourceArenaMetrics, ResourceBudgetError, SamplerRegistry, TextureRegistry,
};
