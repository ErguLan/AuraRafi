//! Backend-neutral viewport grid helpers.

use raf_render::api_graphic_basic::grid::build_2d_grid_points;

pub fn build_grid_points(
    width: f32,
    height: f32,
    offset: [f32; 2],
    zoom: f32,
    spacing: f32,
) -> Vec<[f32; 2]> {
    build_2d_grid_points(width, height, offset, zoom, spacing)
}
