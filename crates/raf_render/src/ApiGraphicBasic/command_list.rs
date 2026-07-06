use std::sync::Arc;

use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use glam::{Mat4, Vec3};

/// Individual drawing and configuration commands.
#[derive(Debug, Clone)]
pub enum GraphicCommand {
    /// Clear the framebuffer or screen target with a specified color.
    Clear { r: u8, g: u8, b: u8, a: u8 },
    /// Bind a specific rendering pipeline.
    SetPipeline(BasicPipelineKind),
    /// Draw a 3D indexed mesh.
    DrawMesh {
        /// ID of the registered mesh to draw.
        mesh_id: usize,
        /// Model transformation matrix.
        transform: Mat4,
        /// Color tint (RGBA).
        color: [u8; 4],
    },
    /// Draw a 3D line.
    DrawLine {
        /// Starting point.
        start: Vec3,
        /// Ending point.
        end: Vec3,
        /// Color (RGBA).
        color: [u8; 4],
        /// Line thickness.
        width: f32,
        /// Bypass the depth test (renders on top of everything).
        no_depth_test: bool,
        /// Additional depth bias applied after projection.
        depth_bias: f32,
    },
    /// Draw the coordinate grid.
    DrawGrid {
        /// Height of the grid on the Y axis.
        grid_y: f32,
        /// Spacing between major grid lines.
        spacing: f32,
        /// Bypass the depth test.
        no_depth_test: bool,
    },
}

/// Accumulator of drawing commands that represents a single frame's rendering pipeline instructions.
#[derive(Debug, Clone, Default)]
pub struct BasicCommandList {
    commands: Vec<GraphicCommand>,
    meshes: Vec<Arc<BasicMesh>>,
}

impl BasicCommandList {
    /// Create an empty command list.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            meshes: Vec::new(),
        }
    }

    /// Add a clear command.
    pub fn clear(&mut self, color: [u8; 4]) {
        self.commands.push(GraphicCommand::Clear {
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        });
    }

    /// Register a mesh for this frame and return its command-local ID.
    pub fn register_mesh(&mut self, mesh: Arc<BasicMesh>) -> usize {
        let id = self.meshes.len();
        self.meshes.push(mesh);
        id
    }

    /// Add a pipeline binding command.
    pub fn set_pipeline(&mut self, pipeline: BasicPipelineKind) {
        self.commands.push(GraphicCommand::SetPipeline(pipeline));
    }

    /// Add a mesh drawing command.
    pub fn draw_mesh(&mut self, mesh_id: usize, transform: Mat4, color: [u8; 4]) {
        self.commands.push(GraphicCommand::DrawMesh {
            mesh_id,
            transform,
            color,
        });
    }

    /// Add a line drawing command.
    pub fn draw_line(
        &mut self,
        start: Vec3,
        end: Vec3,
        color: [u8; 4],
        width: f32,
        no_depth_test: bool,
        depth_bias: f32,
    ) {
        self.commands.push(GraphicCommand::DrawLine {
            start,
            end,
            color,
            width,
            no_depth_test,
            depth_bias,
        });
    }

    /// Add a grid drawing command.
    pub fn draw_grid(&mut self, grid_y: f32, spacing: f32, no_depth_test: bool) {
        self.commands.push(GraphicCommand::DrawGrid {
            grid_y,
            spacing,
            no_depth_test,
        });
    }

    /// Get a reference to the recorded commands.
    pub fn commands(&self) -> &[GraphicCommand] {
        &self.commands
    }

    /// Resolve a mesh by its frame-local ID.
    pub fn mesh(&self, mesh_id: usize) -> Option<&BasicMesh> {
        self.meshes.get(mesh_id).map(|mesh| mesh.as_ref())
    }

    /// Resolve the underlying shared mesh handle by its frame-local ID.
    pub fn mesh_arc(&self, mesh_id: usize) -> Option<&Arc<BasicMesh>> {
        self.meshes.get(mesh_id)
    }

    /// Clear the command list for the next frame.
    pub fn clear_commands(&mut self) {
        self.commands.clear();
        self.meshes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::mesh::{BasicMesh, BasicVertex};

    #[test]
    fn register_mesh_and_lookup() {
        let mut commands = BasicCommandList::new();
        let mesh_id = commands.register_mesh(Arc::new(BasicMesh::new(
            vec![BasicVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: [0.0, 0.0],
            }],
            vec![0],
        )));

        assert!(commands.mesh(mesh_id).is_some());
    }
}
