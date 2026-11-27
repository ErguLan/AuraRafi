use glam::Vec2;
use raf_agb::{Batch, DrawMode, Instance, Texture, Vertex, Workload};
use raf_ui::{UiLinearGradient, UiRect};

use super::super::{effects::normalized_corners, UiRoundedClip};

const CORNER_STEPS: usize = 8;

#[derive(Clone, Copy)]
struct PaintVertex {
    point: Vec2,
    uv: Vec2,
    rgba: [f32; 4],
}

impl PaintVertex {
    fn lerp(self, end: Self, t: f32) -> Self {
        Self {
            point: self.point.lerp(end.point, t),
            uv: self.uv.lerp(end.uv, t),
            rgba: std::array::from_fn(|index| {
                self.rgba[index] + (end.rgba[index] - self.rgba[index]) * t
            }),
        }
    }
}

pub(super) struct Painter<'a> {
    workload: &'a mut Workload,
    viewport: UiRect,
    output: [f32; 2],
    instance: u32,
    polygon: Vec<PaintVertex>,
    scratch: Vec<PaintVertex>,
}

impl<'a> Painter<'a> {
    pub fn new(workload: &'a mut Workload, viewport: UiRect, output: [u32; 2]) -> Self {
        if workload.textures.is_empty() {
            workload.textures.push(Texture {
                size: [1, 1],
                rgba: vec![255; 4],
                mipmaps: false,
            });
        }
        let instance = workload
            .batches
            .last()
            .filter(|batch| batch.mode == DrawMode::Overlay)
            .map(|batch| batch.instances.start)
            .unwrap_or_else(|| {
                let index = workload.instances.len() as u32;
                workload
                    .instances
                    .push(Instance::new(glam::Mat4::IDENTITY, [1.; 4]));
                index
            });
        Self {
            workload,
            viewport,
            output: output.map(|v| v as f32),
            instance,
            polygon: Vec::with_capacity(64),
            scratch: Vec::with_capacity(64),
        }
    }

    pub fn texture(&mut self, texture: Texture) -> u32 {
        let index = self.workload.textures.len() as u32;
        self.workload.textures.push(texture);
        index
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        &mut self,
        area: UiRect,
        clip: UiRect,
        uv: UiRect,
        radii: [f32; 4],
        color: [f32; 4],
        gradient: Option<UiLinearGradient>,
        clips: &[UiRoundedClip],
        texture: u32,
    ) {
        if area.is_empty() || clip.is_empty() || (color[3] <= 0. && gradient.is_none()) {
            return;
        }
        self.polygon.clear();
        self.polygon
            .extend(rounded_rect(area, radii, false).map(|point| {
                let local = Vec2::new(
                    (point.x - area.x) / area.width,
                    (point.y - area.y) / area.height,
                );
                let rgba = gradient.map_or(color, |gradient| {
                    let dir = Vec2::from_array(gradient.direction);
                    let span = dir.x.abs() + dir.y.abs();
                    let t = if span > 0. {
                        ((local - Vec2::splat(0.5)).dot(dir) / span + 0.5).clamp(0., 1.)
                    } else {
                        0.
                    };
                    std::array::from_fn(|i| {
                        f32::from(gradient.start[i]) / 255.0 * (1. - t)
                            + f32::from(gradient.end[i]) / 255.0 * t
                    })
                });
                PaintVertex {
                    point,
                    uv: Vec2::new(uv.x + local.x * uv.width, uv.y + local.y * uv.height),
                    rgba,
                }
            }));
        self.emit(clip, clips, texture);
    }

    pub fn ring(
        &mut self,
        outer: UiRect,
        outer_radii: [f32; 4],
        inner: UiRect,
        inner_radii: [f32; 4],
        colors: [[f32; 4]; 2],
        clip: UiRect,
        clips: &[UiRoundedClip],
    ) {
        let mut points =
            rounded_rect(outer, outer_radii, true).zip(rounded_rect(inner, inner_radii, true));
        let Some(first) = points.next() else {
            return;
        };
        let vertex = |point, rgba| PaintVertex {
            point,
            uv: Vec2::splat(0.5),
            rgba,
        };
        let mut previous = first;
        for next in points.chain(std::iter::once(first)) {
            self.polygon.clear();
            self.polygon.extend([
                vertex(previous.0, colors[0]),
                vertex(next.0, colors[0]),
                vertex(next.1, colors[1]),
                vertex(previous.1, colors[1]),
            ]);
            self.emit(clip, clips, 0);
            previous = next;
        }
    }

    pub fn stroke(
        &mut self,
        start: Vec2,
        end: Vec2,
        width: f32,
        clip: UiRect,
        clips: &[UiRoundedClip],
        color: [u8; 4],
    ) {
        let normal = (end - start).perp().normalize_or_zero() * width * 0.5;
        self.polygon.clear();
        self.polygon.extend(
            [start - normal, end - normal, end + normal, start + normal].map(|point| PaintVertex {
                point,
                uv: Vec2::splat(0.5),
                rgba: rgba(color),
            }),
        );
        self.emit(clip, clips, 0);
    }

    pub fn solid_polygon(&mut self, points: &[Vec2], color: [u8; 4], clip: UiRect) {
        self.polygon.clear();
        self.polygon.extend(points.iter().map(|&point| PaintVertex {
            point,
            uv: Vec2::splat(0.5),
            rgba: rgba(color),
        }));
        self.emit(clip, &[], 0);
    }

    fn emit(&mut self, clip: UiRect, clips: &[UiRoundedClip], texture: u32) {
        let clip = clip.intersection(UiRect::new(
            0.,
            0.,
            self.viewport.width,
            self.viewport.height,
        ));
        if clip.is_empty() {
            return;
        }
        clip_polygon(
            &mut self.polygon,
            &mut self.scratch,
            rounded_rect(clip, [0.; 4], false),
        );
        for clip in clips {
            clip_polygon(
                &mut self.polygon,
                &mut self.scratch,
                rounded_rect(clip.rect, clip.corners, false),
            );
        }
        if self.polygon.len() < 3 {
            return;
        }
        let first = self.workload.vertices.len() as u32;
        let to_vertex = |source: PaintVertex| Vertex {
            position: [
                2. * (source.point.x + self.viewport.x) / self.output[0] - 1.,
                1. - 2. * (source.point.y + self.viewport.y) / self.output[1],
                0.,
            ],
            uv: source.uv.to_array(),
            rgba: source.rgba,
            normal: [0.; 3],
        };
        let anchor = to_vertex(self.polygon[0]);
        for edge in self.polygon[1..].windows(2) {
            self.workload
                .vertices
                .extend([anchor, to_vertex(edge[0]), to_vertex(edge[1])]);
        }
        let end = self.workload.vertices.len() as u32;
        if let Some(batch) = self.workload.batches.last_mut().filter(|batch| {
            batch.mode == DrawMode::Overlay
                && batch.texture == texture
                && batch.vertices.end == first
                && batch.instances.start == self.instance
        }) {
            batch.vertices.end = end;
        } else {
            self.workload.batches.push(Batch {
                indices: None,
                mode: DrawMode::Overlay,
                texture,
                vertices: first..end,
                instances: self.instance..self.instance + 1,
            });
        }
    }
}

fn rounded_rect(
    rect: UiRect,
    radii: [f32; 4],
    keep_steps: bool,
) -> impl Clone + Iterator<Item = Vec2> {
    let radii = normalized_corners(rect, radii);
    let centers = [
        Vec2::new(rect.x + radii[0], rect.y + radii[0]),
        Vec2::new(rect.right() - radii[1], rect.y + radii[1]),
        Vec2::new(rect.right() - radii[2], rect.bottom() - radii[2]),
        Vec2::new(rect.x + radii[3], rect.bottom() - radii[3]),
    ];
    centers
        .into_iter()
        .zip(radii)
        .enumerate()
        .flat_map(move |(index, (center, radius))| {
            let steps = if radius > 0. || keep_steps {
                CORNER_STEPS
            } else {
                0
            };
            (0..=steps).map(move |step| {
                let phase = (index as f32 + 2. + step as f32 / CORNER_STEPS as f32)
                    * std::f32::consts::FRAC_PI_2;
                center + Vec2::new(phase.cos(), phase.sin()) * radius
            })
        })
}

fn clip_polygon(
    polygon: &mut Vec<PaintVertex>,
    scratch: &mut Vec<PaintVertex>,
    mut mask: impl Clone + Iterator<Item = Vec2>,
) {
    let Some(mut start) = mask.clone().last() else {
        return;
    };
    for end in &mut mask {
        let distance = |point: Vec2| (end - start).perp_dot(point - start);
        let Some(&last) = polygon.last() else {
            break;
        };
        let mut prev = last;
        let mut prev_distance = distance(prev.point);
        scratch.clear();
        for &next in polygon.iter() {
            let next_distance = distance(next.point);
            if (prev_distance >= 0.) != (next_distance >= 0.) {
                scratch.push(prev.lerp(next, prev_distance / (prev_distance - next_distance)));
            }
            if next_distance >= 0. {
                scratch.push(next);
            }
            prev = next;
            prev_distance = next_distance;
        }
        std::mem::swap(polygon, scratch);
        start = end;
    }
}

pub(super) fn rgba(color: [u8; 4]) -> [f32; 4] {
    color.map(|c| f32::from(c) / 255.)
}
