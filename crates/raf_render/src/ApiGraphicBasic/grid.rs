use glam::Vec3;

const SCENE_GRID_MARGIN: f32 = 10.0;
const MAX_MINOR_LINES_PER_AXIS: i32 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridLineKind {
    Minor,
    Major,
    Axis,
}

#[derive(Clone, Copy, Debug)]
pub struct GridLine {
    pub start: Vec3,
    pub end: Vec3,
    pub width: f32,
    pub kind: GridLineKind,
}

pub fn build_3d_grid(bounds_min: Vec3, bounds_max: Vec3, spacing: f32) -> Vec<GridLine> {
    let spacing = spacing.max(0.25);
    let min_x_index = ((bounds_min.x - SCENE_GRID_MARGIN) / spacing).floor() as i32;
    let max_x_index = ((bounds_max.x + SCENE_GRID_MARGIN) / spacing).ceil() as i32;
    let min_z_index = ((bounds_min.z - SCENE_GRID_MARGIN) / spacing).floor() as i32;
    let max_z_index = ((bounds_max.z + SCENE_GRID_MARGIN) / spacing).ceil() as i32;
    let min_x = min_x_index as f32 * spacing;
    let max_x = max_x_index as f32 * spacing;
    let min_z = min_z_index as f32 * spacing;
    let max_z = max_z_index as f32 * spacing;

    let density_step_x = (((max_x_index - min_x_index + 1) as f32 / MAX_MINOR_LINES_PER_AXIS as f32).ceil() as i32).max(1);
    let density_step_z = (((max_z_index - min_z_index + 1) as f32 / MAX_MINOR_LINES_PER_AXIS as f32).ceil() as i32).max(1);
    let mut lines = Vec::with_capacity(((max_x_index - min_x_index + max_z_index - min_z_index + 2) * 2) as usize);

    for i in min_x_index..=max_x_index {
        let fi = i as f32 * spacing;
        let kind = if i == 0 {
            GridLineKind::Axis
        } else if i % 5 == 0 {
            GridLineKind::Major
        } else {
            GridLineKind::Minor
        };
        if kind == GridLineKind::Minor && i.rem_euclid(density_step_x) != 0 {
            continue;
        }
        let width = match kind {
            GridLineKind::Axis => 1.4,
            GridLineKind::Major => 0.9,
            GridLineKind::Minor => 0.45,
        };

        lines.push(GridLine {
            start: Vec3::new(fi, 0.0, min_z),
            end: Vec3::new(fi, 0.0, max_z),
            width,
            kind,
        });
    }

    for i in min_z_index..=max_z_index {
        let fi = i as f32 * spacing;
        let kind = if i == 0 {
            GridLineKind::Axis
        } else if i % 5 == 0 {
            GridLineKind::Major
        } else {
            GridLineKind::Minor
        };
        if kind == GridLineKind::Minor && i.rem_euclid(density_step_z) != 0 {
            continue;
        }
        let width = match kind {
            GridLineKind::Axis => 1.4,
            GridLineKind::Major => 0.9,
            GridLineKind::Minor => 0.45,
        };

        lines.push(GridLine {
            start: Vec3::new(min_x, 0.0, fi),
            end: Vec3::new(max_x, 0.0, fi),
            width,
            kind,
        });
    }

    lines
}

pub fn build_2d_grid_points(
    width: f32,
    height: f32,
    offset: [f32; 2],
    zoom: f32,
    grid_spacing: f32,
) -> Vec<[f32; 2]> {
    let spacing = (40.0 * zoom * grid_spacing.max(0.25)).clamp(8.0, 160.0);
    if spacing < 8.0 {
        return Vec::new();
    }

    let mut dots = Vec::new();
    let mut x = offset[0].rem_euclid(spacing);
    while x < width {
        let mut y = offset[1].rem_euclid(spacing);
        while y < height {
            dots.push([x, y]);
            y += spacing;
        }
        x += spacing;
    }

    dots
}