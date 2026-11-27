//! ApiGraphicBasic's platform-independent resource and frame contract.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod capabilities;
pub mod frame_graph;
pub mod frame_scheduler;
pub mod handles;
pub mod memory_ledger;
pub mod resource_registry;

pub mod backends;
mod device;
pub mod fixtures;
pub mod product;
mod workload;

pub use device::{
    AdapterInfo, Backend, Device, Error, Frame, FrameMetrics, FramePlan, Output, RasterUniforms,
    Result, ShadowFrame,
};
pub use workload::{
    Batch, DrawMode, Instance, ResourcePatch, Texture, TextureUpdate, Vertex, Workload,
};

pub const SHADER: &str = include_str!("shaders/draw.wgsl");
pub const SCENE_FILTER_SHADER: &str = include_str!("shaders/scene_filter.wgsl");
