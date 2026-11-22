//! Deterministic geometry and camera shared by backend comparisons.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::{Batch, DrawMode, Frame, Instance, Texture, Vertex, Workload};

pub fn camera(extent: [u32; 2], orbit: f32) -> Frame {
    let aspect = extent[0] as f32 / extent[1].max(1) as f32;
    let center = Vec3::new(0.0, 0.4, 0.0);
    let eye = Vec3::new(6.5 * orbit.cos(), 4.4, 6.5 * orbit.sin());
    let proj = Mat4::perspective_rh(48.0_f32.to_radians(), aspect, 0.1, 70.0);
    Frame {
        view_projection: (proj * Mat4::look_at_rh(eye, center, Vec3::Y)).to_cols_array_2d(),
        clear: [0.054, 0.071, 0.10, 1.0],
        eye: eye.to_array(),
        light: [0.0, 1.0, 0.0, 1.0],
        light_color: [1.0; 4],
        environment: [1.; 4],
        viewport: [0., 0., 1., 1.],
        shadow: None,
        scene_filter: false,
    }
}

pub fn scene(shadows: bool, repeat: u32) -> Workload {
    let mut workload = Workload::default();
    workload.textures.push(Texture {
        size: [1, 1],
        rgba: vec![255; 4],
    });
    let mut checker = Vec::new();
    for y in 0..32 {
        for x in 0..32 {
            let color = if (x / 4 + y / 4) % 2 == 0 {
                [228, 231, 237, 255]
            } else {
                [156, 169, 186, 255]
            };
            checker.extend_from_slice(&color);
        }
    }
    workload.textures.push(Texture {
        size: [32, 32],
        rgba: checker,
    });
    let ground = quad(
        &mut workload,
        [
            [-18., 0., 18.],
            [18., 0., 18.],
            [18., 0., -18.],
            [-18., 0., -18.],
        ],
        [[0., 0.], [12., 0.], [12., 12.], [0., 12.]],
        [0.22, 0.27, 0.34, 1.],
    );
    workload
        .instances
        .push(Instance::new(Mat4::IDENTITY, [1.; 4]));
    workload.batches.push(Batch {
        mode: DrawMode::Scene,
        vertices: ground,
        indices: None,
        instances: 0..1,
        texture: 1,
    });
    let cube_start = workload.vertices.len() as u32;
    let corners = [
        [-0.5, 0., -0.5],
        [0.5, 0., -0.5],
        [0.5, 0., 0.5],
        [-0.5, 0., 0.5],
        [-0.5, 1., -0.5],
        [0.5, 1., -0.5],
        [0.5, 1., 0.5],
        [-0.5, 1., 0.5],
    ];
    let faces = [
        ([0, 1, 5, 4], 0.64),
        ([1, 2, 6, 5], 0.82),
        ([2, 3, 7, 6], 0.95),
        ([3, 0, 4, 7], 0.54),
        ([4, 5, 6, 7], 1.0),
        ([3, 2, 1, 0], 0.3),
    ];
    for (face, light) in faces {
        quad(
            &mut workload,
            face.map(|index| corners[index]),
            [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            [light, light, light, 1.],
        );
    }
    let cube = cube_start..workload.vertices.len() as u32;
    let repeat = repeat.clamp(1, 80);
    let transforms = (0..repeat)
        .flat_map(|y| (0..repeat).map(move |x| (x, y)))
        .map(|(x, y)| {
            let offset = (repeat - 1) as f32 * 0.69;
            let height = 0.75 + ((x * 19 + y * 31) % 7) as f32 * 0.11;
            let transform = Mat4::from_scale_rotation_translation(
                Vec3::new(1.05, height, 1.05),
                Quat::from_rotation_y(0.17 * ((x * 7 + y) % 4) as f32),
                Vec3::new(x as f32 * 1.38 - offset, 0.002, y as f32 * 1.38 - offset),
            );
            let tint = [
                [0.98, 0.61, 0.31, 1.],
                [0.35, 0.73, 0.93, 1.],
                [0.78, 0.88, 0.89, 1.],
            ];
            (transform, tint[(x as usize + 2 * y as usize) % tint.len()])
        });
    if shadows {
        let projection = Mat4::from_cols(
            Vec4::X,
            Vec4::new(-0.55, 0., -0.38, 0.),
            Vec4::Z,
            Vec4::new(0., 0.001, 0., 1.),
        );
        let start = workload.instances.len() as u32;
        workload.instances.extend(
            transforms.clone().map(|(transform, _)| {
                Instance::new(projection * transform, [0.05, 0.07, 0.10, 0.65])
            }),
        );
        workload.batches.push(Batch {
            mode: DrawMode::Scene,
            vertices: cube.clone(),
            indices: None,
            instances: start..workload.instances.len() as u32,
            texture: 0,
        });
    }
    let start = workload.instances.len() as u32;
    workload
        .instances
        .extend(transforms.map(|(transform, tint)| Instance::new(transform, tint)));
    workload.batches.push(Batch {
        mode: DrawMode::Scene,
        vertices: cube,
        indices: None,
        instances: start..workload.instances.len() as u32,
        texture: 1,
    });
    workload
}

pub fn quad(
    workload: &mut Workload,
    positions: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    rgba: [f32; 4],
) -> std::ops::Range<u32> {
    let first = workload.vertices.len() as u32;
    workload.vertices.extend([0, 1, 2, 0, 2, 3].map(|i| Vertex {
        position: positions[i],
        uv: uv[i],
        rgba,
        normal: [0.; 3],
    }));
    first..workload.vertices.len() as u32
}
