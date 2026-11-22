use glam::Mat4;

use crate::{Batch, DrawMode, Error, Instance, Result, Vertex};

use super::{validate_color, Encoder};
use crate::product::{Grid, Line};

const LINE_KIND: u32 = 1;
const GRID_KIND: u32 = 2;
const GRID_RADIUS: i32 = 24;
const MAJOR_STEP: i32 = 4;

impl Encoder<'_> {
    pub(super) fn line(&mut self, line: Line) -> Result<()> {
        validate_color(line.rgba)?;
        if !line.start.into_iter().chain(line.end).all(f32::is_finite)
            || !line.width_px.is_finite()
            || line.width_px <= 0.
            || !line.depth_bias.is_finite()
        {
            return Err(Error::Invalid("invalid world line"));
        }
        if line.start == line.end || line.rgba[3] == 0. {
            return Ok(());
        }
        let instance = match self.line_instance {
            Some(instance) => instance,
            None => {
                let instance = self.raster_instance(LINE_KIND, [1.; 4])?;
                self.line_instance = Some(instance);
                instance
            }
        };
        self.line_batch(&line_vertices(line), instance, line.no_depth_test)
    }

    pub(super) fn grid(&mut self, grid: Grid) -> Result<()> {
        let radius = GRID_RADIUS as f32;
        if !grid.y.is_finite()
            || !grid.spacing.is_finite()
            || grid.spacing <= 0.
            || !(radius * grid.spacing * 2.).is_finite()
        {
            return Err(Error::Invalid("invalid world grid"));
        }
        let instance = self.raster_instance(
            GRID_KIND,
            [grid.spacing, (radius - 2.) * grid.spacing, 0., 0.],
        )?;
        for i in -GRID_RADIUS..=GRID_RADIUS {
            let offset = i as f32 * grid.spacing;
            let limit = radius * grid.spacing;
            let major = i.rem_euclid(MAJOR_STEP) == 0;
            let gray = if major { 0.46 } else { 0.55 };
            let rgba = [gray, gray, gray, if major { 0.44 } else { 0.25 }];
            for (start, end) in [
                ([-limit, grid.y, offset], [limit, grid.y, offset]),
                ([offset, grid.y, -limit], [offset, grid.y, limit]),
            ] {
                self.line_batch(
                    &line_vertices(Line {
                        start,
                        end,
                        rgba,
                        width_px: 1.,
                        depth_bias: 0.0001,
                        no_depth_test: grid.no_depth_test,
                    }),
                    instance,
                    grid.no_depth_test,
                )?;
            }
        }
        Ok(())
    }

    fn line_batch(&mut self, vertices: &[Vertex], instance: u32, no_depth: bool) -> Result<()> {
        if self.overlay {
            return Err(Error::Unsupported(
                "world primitives must precede retained UI composition",
            ));
        }
        let vertices = self.add_vertices(vertices)?;
        self.batch(Batch {
            mode: if no_depth {
                DrawMode::SceneNoDepth
            } else {
                DrawMode::DepthReadOnly
            },
            vertices,
            instances: instance..instance + 1,
            indices: None,
            texture: self.white,
        })
    }

    pub(super) fn raster_instance(&mut self, kind: u32, params: [f32; 4]) -> Result<u32> {
        self.reserve(size_of::<Instance>() as u64)?;
        let index = super::stream_range(self.workload.instances.len(), 1)?.start;
        let mut instance = Instance::new(Mat4::IDENTITY, params);
        instance.raster_kind = kind;
        self.workload.instances.push(instance);
        Ok(index)
    }
}

fn line_vertices(line: Line) -> [Vertex; 6] {
    let vertex = |position, endpoint, side| Vertex {
        position,
        normal: endpoint,
        uv: [side * line.width_px, line.depth_bias],
        rgba: line.rgba,
    };
    let a = vertex(line.start, line.end, -1.);
    let b = vertex(line.end, line.start, 1.);
    let c = vertex(line.end, line.start, -1.);
    let d = vertex(line.start, line.end, 1.);
    [a, b, c, a, c, d]
}
