use glam::Vec3;

/// An edge defined by two 3D points.
pub type Edge = [Vec3; 2];

/// Primitive recipe for a unit cube (1x1x1 centered at origin).
pub fn cube_edges() -> Vec<Edge> {
    let h = 0.5;
    let c = [
        Vec3::new(-h, -h, -h), Vec3::new( h, -h, -h),
        Vec3::new( h,  h, -h), Vec3::new(-h,  h, -h),
        Vec3::new(-h, -h,  h), Vec3::new( h, -h,  h),
        Vec3::new( h,  h,  h), Vec3::new(-h,  h,  h),
    ];
    vec![
        [c[0], c[1]], [c[1], c[2]], [c[2], c[3]], [c[3], c[0]],
        [c[4], c[5]], [c[5], c[6]], [c[6], c[7]], [c[7], c[4]],
        [c[0], c[4]], [c[1], c[5]], [c[2], c[6]], [c[3], c[7]],
    ]
}

pub fn cube_faces() -> Vec<([Vec3; 4], Vec3)> {
    let h = 0.5;
    let c = [
        Vec3::new(-h, -h, -h), Vec3::new( h, -h, -h),
        Vec3::new( h,  h, -h), Vec3::new(-h,  h, -h),
        Vec3::new(-h, -h,  h), Vec3::new( h, -h,  h),
        Vec3::new( h,  h,  h), Vec3::new(-h,  h,  h),
    ];
    vec![
        ([c[4], c[5], c[6], c[7]], Vec3::Z),
        ([c[1], c[0], c[3], c[2]], Vec3::NEG_Z),
        ([c[0], c[4], c[7], c[3]], Vec3::NEG_X),
        ([c[5], c[1], c[2], c[6]], Vec3::X),
        ([c[3], c[7], c[6], c[2]], Vec3::Y),
        ([c[0], c[1], c[5], c[4]], Vec3::NEG_Y),
    ]
}

/// Primitive recipe for a sphere built from cheap orthogonal circles.
pub fn sphere_edges(segments: usize) -> Vec<Edge> {
    let mut edges = Vec::new();
    let r = 0.5;
    let seg = segments.max(8);

    for plane in 0..3 {
        for i in 0..seg {
            let a0 = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / seg as f32) * std::f32::consts::TAU;
            let (p0, p1) = match plane {
                0 => (
                    Vec3::new(r * a0.cos(), r * a0.sin(), 0.0),
                    Vec3::new(r * a1.cos(), r * a1.sin(), 0.0),
                ),
                1 => (
                    Vec3::new(r * a0.cos(), 0.0, r * a0.sin()),
                    Vec3::new(r * a1.cos(), 0.0, r * a1.sin()),
                ),
                _ => (
                    Vec3::new(0.0, r * a0.cos(), r * a0.sin()),
                    Vec3::new(0.0, r * a1.cos(), r * a1.sin()),
                ),
            };
            edges.push([p0, p1]);
        }
    }
    edges
}

pub fn sphere_faces(stacks: usize, slices: usize) -> Vec<([Vec3; 4], Vec3)> {
    let r = 0.5;
    let st = stacks.max(3);
    let sl = slices.max(4);
    let mut faces = Vec::with_capacity(st * sl);

    for i in 0..st {
        let phi0 = std::f32::consts::PI * (i as f32 / st as f32);
        let phi1 = std::f32::consts::PI * ((i + 1) as f32 / st as f32);
        for j in 0..sl {
            let theta0 = std::f32::consts::TAU * (j as f32 / sl as f32);
            let theta1 = std::f32::consts::TAU * ((j + 1) as f32 / sl as f32);

            let p00 = Vec3::new(
                r * phi0.sin() * theta0.cos(),
                r * phi0.cos(),
                r * phi0.sin() * theta0.sin(),
            );
            let p10 = Vec3::new(
                r * phi1.sin() * theta0.cos(),
                r * phi1.cos(),
                r * phi1.sin() * theta0.sin(),
            );
            let p11 = Vec3::new(
                r * phi1.sin() * theta1.cos(),
                r * phi1.cos(),
                r * phi1.sin() * theta1.sin(),
            );
            let p01 = Vec3::new(
                r * phi0.sin() * theta1.cos(),
                r * phi0.cos(),
                r * phi0.sin() * theta1.sin(),
            );

            let center = (p00 + p10 + p11 + p01) * 0.25;
            let normal = center.normalize_or_zero();
            faces.push(([p00, p10, p11, p01], normal));
        }
    }
    faces
}

/// Primitive recipe for a unit plane (XZ, at Y=0).
pub fn plane_edges() -> Vec<Edge> {
    let h = 0.5;
    let c = [
        Vec3::new(-h, 0.0, -h), Vec3::new( h, 0.0, -h),
        Vec3::new( h, 0.0,  h), Vec3::new(-h, 0.0,  h),
    ];
    vec![
        [c[0], c[1]], [c[1], c[2]], [c[2], c[3]], [c[3], c[0]],
        [c[0], c[2]],
    ]
}

pub fn plane_faces() -> Vec<([Vec3; 4], Vec3)> {
    let h = 0.5;
    vec![(
        [
            Vec3::new(-h, 0.0, -h),
            Vec3::new( h, 0.0, -h),
            Vec3::new( h, 0.0,  h),
            Vec3::new(-h, 0.0,  h),
        ],
        Vec3::Y,
    )]
}

/// Primitive recipe for a cylinder along the Y axis.
pub fn cylinder_edges(segments: usize) -> Vec<Edge> {
    let mut edges = Vec::new();
    let r = 0.5;
    let h = 0.5;
    let seg = segments.max(8);

    for i in 0..seg {
        let a0 = (i as f32 / seg as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / seg as f32) * std::f32::consts::TAU;

        let top0 = Vec3::new(r * a0.cos(), h, r * a0.sin());
        let top1 = Vec3::new(r * a1.cos(), h, r * a1.sin());
        let bot0 = Vec3::new(r * a0.cos(), -h, r * a0.sin());
        let bot1 = Vec3::new(r * a1.cos(), -h, r * a1.sin());

        edges.push([top0, top1]);
        edges.push([bot0, bot1]);
        if i % (seg / 4).max(1) == 0 {
            edges.push([bot0, top0]);
        }
    }
    edges
}

pub fn cylinder_faces(segments: usize) -> Vec<([Vec3; 4], Vec3)> {
    let r = 0.5;
    let h = 0.5;
    let seg = segments.max(6);
    let mut faces = Vec::with_capacity(seg * 3);

    for i in 0..seg {
        let a0 = std::f32::consts::TAU * (i as f32 / seg as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / seg as f32);

        let c0 = a0.cos();
        let s0 = a0.sin();
        let c1 = a1.cos();
        let s1 = a1.sin();

        let top0 = Vec3::new(r * c0, h, r * s0);
        let top1 = Vec3::new(r * c1, h, r * s1);
        let bot0 = Vec3::new(r * c0, -h, r * s0);
        let bot1 = Vec3::new(r * c1, -h, r * s1);

        let mid_angle = (a0 + a1) * 0.5;
        let side_normal = Vec3::new(mid_angle.cos(), 0.0, mid_angle.sin());

        faces.push(([bot0, bot1, top1, top0], side_normal));

        let top_center = Vec3::new(0.0, h, 0.0);
        faces.push(([top_center, top0, top1, top1], Vec3::Y));

        let bot_center = Vec3::new(0.0, -h, 0.0);
        faces.push(([bot_center, bot1, bot0, bot0], Vec3::NEG_Y));
    }
    faces
}