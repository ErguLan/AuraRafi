use std::sync::Arc;

use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use glam::{Mat4, Vec3};

/// Backend-neutral line instance recorded by a scene or CAD surface.
///
/// `width` is expressed in target pixels. The active GPU and CPU executors
/// consume the same data so debug geometry does not change appearance when a
/// device falls back to software rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasicLine {
    pub start: Vec3,
    pub end: Vec3,
    pub color: [u8; 4],
    pub width: f32,
    pub depth_bias: f32,
}

/// A filled screen-space triangle used by renderer-owned overlays such as
/// transform gizmo arrowheads.
///
/// Coordinates are target pixels with a top-left origin. Keeping this
/// primitive in the backend-neutral command list lets the GPU path and the
/// CPU recovery path render the same billboard geometry without exposing
/// WGPU details to the editor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasicScreenTriangle {
    pub points: [[f32; 2]; 3],
    pub color: [u8; 4],
}

/// Per-instance transform and tint for a shared mesh batch.
///
/// The command list owns the small instance records, while the GPU executor
/// reuses a bounded vertex buffer slot instead of creating one buffer per
/// object or per frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasicMeshInstance {
    pub transform: Mat4,
    pub color: [u8; 4],
}

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
    /// Draw several transforms of one registered mesh in a single instanced
    /// call when the backend supports it.
    DrawMeshBatch {
        mesh_id: usize,
        instances: Vec<BasicMeshInstance>,
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
    /// A contiguous batch of lines that shares the depth-test mode.
    DrawLineBatch {
        lines: Vec<BasicLine>,
        no_depth_test: bool,
    },
    /// Draw filled screen-space overlay triangles.
    DrawScreenTriangleBatch { triangles: Vec<BasicScreenTriangle> },
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
    mesh_ids: std::collections::HashMap<usize, usize>,
    mesh_cacheable: Vec<bool>,
    current_pipeline: BasicPipelineKind,
}

impl BasicCommandList {
    /// Create an empty command list.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            meshes: Vec::new(),
            mesh_ids: std::collections::HashMap::new(),
            mesh_cacheable: Vec::new(),
            current_pipeline: BasicPipelineKind::FlatColor,
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
        let key = Arc::as_ptr(&mesh) as usize;
        if let Some(&id) = self.mesh_ids.get(&key) {
            return id;
        }

        let id = self.meshes.len();
        self.meshes.push(mesh);
        self.mesh_ids.insert(key, id);
        self.mesh_cacheable.push(true);
        id
    }

    /// Register topology that is expected to change frequently, such as an
    /// editor mesh override. It is uploaded for this frame and never enters
    /// the persistent GPU mesh cache.
    pub fn register_transient_mesh(&mut self, mesh: Arc<BasicMesh>) -> usize {
        let id = self.meshes.len();
        self.meshes.push(mesh);
        self.mesh_cacheable.push(false);
        id
    }

    /// Add a pipeline binding command.
    pub fn set_pipeline(&mut self, pipeline: BasicPipelineKind) {
        if self.current_pipeline == pipeline {
            return;
        }
        self.current_pipeline = pipeline;
        self.commands.push(GraphicCommand::SetPipeline(pipeline));
    }

    /// Add a mesh drawing command.
    pub fn draw_mesh(&mut self, mesh_id: usize, transform: Mat4, color: [u8; 4]) {
        let instance = BasicMeshInstance { transform, color };
        let can_extend_batch = matches!(
            self.commands.last(),
            Some(GraphicCommand::DrawMeshBatch {
                mesh_id: batch_mesh_id,
                instances,
            }) if *batch_mesh_id == mesh_id
                && color[3] == u8::MAX
                && instances.iter().all(|instance| instance.color[3] == u8::MAX)
        );
        if can_extend_batch {
            if let Some(GraphicCommand::DrawMeshBatch { instances, .. }) = self.commands.last_mut()
            {
                instances.push(instance);
            }
            return;
        }

        let can_promote_previous = matches!(
            self.commands.last(),
            Some(GraphicCommand::DrawMesh {
                mesh_id: previous_mesh_id,
                color: previous_color,
                ..
            }) if *previous_mesh_id == mesh_id
                && color[3] == u8::MAX
                && previous_color[3] == u8::MAX
        );
        if can_promote_previous {
            let previous = self.commands.pop().expect("mesh command was just checked");
            if let GraphicCommand::DrawMesh {
                transform, color, ..
            } = previous
            {
                self.commands.push(GraphicCommand::DrawMeshBatch {
                    mesh_id,
                    instances: vec![BasicMeshInstance { transform, color }, instance],
                });
                return;
            }
        }

        self.commands.push(GraphicCommand::DrawMesh {
            mesh_id,
            transform,
            color,
        });
    }

    /// Record a batch explicitly when a producer already grouped identical
    /// mesh/material work. Singletons stay as the regular draw command so the
    /// backend does not pay an instancing setup cost for one object.
    pub fn draw_mesh_batch<I>(&mut self, mesh_id: usize, instances: I)
    where
        I: IntoIterator<Item = BasicMeshInstance>,
    {
        let instances = instances.into_iter().collect::<Vec<_>>();
        if instances
            .iter()
            .any(|instance| instance.color[3] != u8::MAX)
        {
            for instance in instances {
                self.draw_mesh(mesh_id, instance.transform, instance.color);
            }
            return;
        }
        match instances.len() {
            0 => {}
            1 => {
                let instance = instances[0];
                self.draw_mesh(mesh_id, instance.transform, instance.color);
            }
            _ => self
                .commands
                .push(GraphicCommand::DrawMeshBatch { mesh_id, instances }),
        }
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
        let line = BasicLine {
            start,
            end,
            color,
            width: width.max(1.0),
            depth_bias,
        };

        if let Some(GraphicCommand::DrawLineBatch {
            lines: batch_lines,
            no_depth_test: batch_no_depth_test,
        }) = self.commands.last_mut()
        {
            if *batch_no_depth_test == no_depth_test {
                batch_lines.push(line);
                return;
            }
        }

        self.commands.push(GraphicCommand::DrawLineBatch {
            lines: vec![line],
            no_depth_test,
        });
    }

    /// Add a screen-space overlay triangle. Adjacent triangles are grouped so
    /// a gizmo can submit its outline and fill with one backend draw call.
    pub fn draw_screen_triangle(&mut self, triangle: BasicScreenTriangle) {
        if let Some(GraphicCommand::DrawScreenTriangleBatch { triangles }) =
            self.commands.last_mut()
        {
            triangles.push(triangle);
            return;
        }

        self.commands.push(GraphicCommand::DrawScreenTriangleBatch {
            triangles: vec![triangle],
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

    pub fn mesh_cacheable(&self, mesh_id: usize) -> bool {
        self.mesh_cacheable.get(mesh_id).copied().unwrap_or(false)
    }

    /// Clear the command list for the next frame.
    pub fn clear_commands(&mut self) {
        self.commands.clear();
        self.meshes.clear();
        self.mesh_ids.clear();
        self.mesh_cacheable.clear();
        self.current_pipeline = BasicPipelineKind::FlatColor;
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

    #[test]
    fn adjacent_lines_merge_without_crossing_depth_modes() {
        let mut commands = BasicCommandList::new();
        commands.draw_line(Vec3::ZERO, Vec3::X, [255, 0, 0, 255], 2.0, false, 0.0);
        commands.draw_line(Vec3::Y, Vec3::ONE, [0, 255, 0, 255], 1.0, false, 0.0);
        commands.draw_line(Vec3::Z, Vec3::ONE, [0, 0, 255, 255], 1.0, true, 0.0);

        assert_eq!(commands.commands().len(), 2);
        assert!(matches!(
            &commands.commands()[0],
            GraphicCommand::DrawLineBatch { lines, no_depth_test: false }
                if lines.len() == 2 && lines[0].width == 2.0
        ));
        assert!(matches!(
            &commands.commands()[1],
            GraphicCommand::DrawLineBatch { lines, no_depth_test: true }
                if lines.len() == 1
        ));
    }

    #[test]
    fn adjacent_meshes_promote_to_an_instance_batch() {
        let mut commands = BasicCommandList::new();
        let mesh = Arc::new(BasicMesh::new(
            vec![BasicVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: [0.0, 0.0],
            }],
            vec![0],
        ));
        let mesh_id = commands.register_mesh(mesh);
        commands.draw_mesh(mesh_id, Mat4::IDENTITY, [255, 0, 0, 255]);
        commands.draw_mesh(mesh_id, Mat4::from_translation(Vec3::X), [0, 255, 0, 255]);

        assert!(matches!(
            &commands.commands()[0],
            GraphicCommand::DrawMeshBatch { mesh_id: id, instances }
                if *id == mesh_id && instances.len() == 2
        ));
    }
}
