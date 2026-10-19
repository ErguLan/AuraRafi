//! Static directional stars: one integer hash per sky cell, no geometry, textures or clock.
use glam::{Vec2, Vec3};

fn hash(mut n: u32) -> f32 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846ca68b);
    n ^= n >> 16;
    (n & 0x00ff_ffff) as f32 / 16777216.0
}

pub(crate) fn brightness(ray: Vec3, density: f32) -> f32 {
    if ray.y <= 0.0 || density <= 0.0 {
        return 0.0;
    }
    let a = ray.abs();
    let (uv, face) = if a.x >= a.y && a.x >= a.z {
        (
            Vec2::new(ray.y, ray.z) / a.x,
            if ray.x >= 0.0 { 0 } else { 1 },
        )
    } else if a.y >= a.z {
        (
            Vec2::new(ray.x, ray.z) / a.y,
            if ray.y >= 0.0 { 2 } else { 3 },
        )
    } else {
        (
            Vec2::new(ray.x, ray.y) / a.z,
            if ray.z >= 0.0 { 4 } else { 5 },
        )
    };
    let p = (uv * 0.5 + Vec2::splat(0.5)) * 20.0;
    let cell = p.floor();
    let seed = (cell.x as u32).wrapping_mul(73856093)
        ^ (cell.y as u32).wrapping_mul(19349663)
        ^ (face as u32).wrapping_mul(83492791);
    if hash(seed) > density * 0.5 {
        return 0.0;
    }
    let center =
        Vec2::new(hash(seed ^ 0xa511e9b3), hash(seed ^ 0x63d83595)) * 0.64 + Vec2::splat(0.18);
    let d = (p - cell - center).abs();
    let radius = 0.055 + hash(seed ^ 0x9e3779b9) * 0.05;
    let core = (1.0 - d.length() / radius).clamp(0.0, 1.0);
    let x = (1.0 - d.x / (radius * 0.28)).clamp(0.0, 1.0)
        * (1.0 - d.y / (radius * 1.7)).clamp(0.0, 1.0);
    let y = (1.0 - d.y / (radius * 0.28)).clamp(0.0, 1.0)
        * (1.0 - d.x / (radius * 1.7)).clamp(0.0, 1.0);
    core.max(x.max(y) * 0.7) * (ray.y * 5.0).clamp(0.0, 1.0)
}

pub(crate) const WGSL: &str = r#"
fn star_hash(seed: u32) -> f32 {
    var n = seed;
    n = n ^ (n >> 16u); n = n * 0x7feb352du;
    n = n ^ (n >> 15u); n = n * 0x846ca68bu; n = n ^ (n >> 16u);
    return f32(n & 0x00ffffffu) / 16777216.0;
}
fn star_brightness(ray: vec3<f32>, density: f32) -> f32 {
    if ray.y <= 0.0 || density <= 0.0 { return 0.0; }
    let a = abs(ray); var uv = vec2<f32>(0.0); var face = 0u;
    if a.x >= a.y && a.x >= a.z { uv = ray.yz / a.x; face = select(1u,0u,ray.x >= 0.0); }
    else if a.y >= a.z { uv = ray.xz / a.y; face = select(3u,2u,ray.y >= 0.0); }
    else { uv = ray.xy / a.z; face = select(5u,4u,ray.z >= 0.0); }
    let p = (uv*0.5+vec2<f32>(0.5))*20.0; let cell = floor(p);
    let seed = u32(cell.x)*73856093u ^ u32(cell.y)*19349663u ^ face*83492791u;
    if star_hash(seed) > density*0.5 { return 0.0; }
    let center = vec2<f32>(star_hash(seed ^ 0xa511e9b3u),star_hash(seed ^ 0x63d83595u))*0.64+vec2<f32>(0.18);
    let d = abs(p-cell-center); let radius = 0.055+star_hash(seed ^ 0x9e3779b9u)*0.05;
    let core = clamp(1.0-length(d)/radius,0.0,1.0);
    let x = clamp(1.0-d.x/(radius*0.28),0.0,1.0)*clamp(1.0-d.y/(radius*1.7),0.0,1.0);
    let y = clamp(1.0-d.y/(radius*0.28),0.0,1.0)*clamp(1.0-d.x/(radius*1.7),0.0,1.0);
    return max(core,max(x,y)*0.7)*clamp(ray.y*5.0,0.0,1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stylized_stars_are_static_bounded_and_bypass_disabled_or_ground_rays() {
        let mut visible = 0;
        for x in -100..100 {
            for z in -100..100 {
                let ray = Vec3::new(x as f32 / 100.0, 0.7, z as f32 / 100.0).normalize();
                let value = brightness(ray, 1.0);
                assert!(value.is_finite() && (0.0..=1.0).contains(&value));
                assert_eq!(value, brightness(ray, 1.0));
                assert_eq!(brightness(ray, 0.0), 0.0);
                assert_eq!(brightness(-ray, 1.0), 0.0);
                visible += usize::from(value > 0.0);
            }
        }
        assert!(
            visible > 50,
            "no accidental all-black star field: {visible}"
        );
    }
}
