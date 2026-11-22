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
    brightness_filtered(ray, density, 0.0)
}
pub(crate) fn brightness_filtered(ray: Vec3, density: f32, footprint: f32) -> f32 {
    let Some((p, cell, seed)) = cell_seed(ray, density) else {
        return 0.0;
    };
    let center =
        Vec2::new(hash(seed ^ 0xa511e9b3), hash(seed ^ 0x63d83595)) * 0.64 + Vec2::splat(0.18);
    let d = (p - cell - center).abs();
    let base_radius = 0.055 + hash(seed ^ 0x9e3779b9) * 0.05;
    let radius = base_radius + footprint.max(0.0) * 0.5;
    let core = (1.0 - d.length() / radius).clamp(0.0, 1.0);
    let x = (1.0 - d.x / (radius * 0.28)).clamp(0.0, 1.0)
        * (1.0 - d.y / (radius * 1.7)).clamp(0.0, 1.0);
    let y = (1.0 - d.y / (radius * 0.28)).clamp(0.0, 1.0)
        * (1.0 - d.x / (radius * 1.7)).clamp(0.0, 1.0);
    core.max(x.max(y) * 0.7) * (ray.y * 5.0).clamp(0.0, 1.0) * (base_radius / radius).powi(2)
}
/// Per-star chroma: `warm -> cool` spread that keeps the field neutral on
/// average but removes the single flat tone. Same cell hash as brightness, so
/// the tint stays static and deterministic.
pub(crate) fn star_tint(ray: Vec3, density: f32) -> Vec3 {
    let Some((_, _, seed)) = cell_seed(ray, density) else {
        return Vec3::ZERO;
    };
    let t = hash(seed ^ 0x25cc5f23);
    Vec3::new(1.0, 0.88, 0.72).lerp(Vec3::new(0.68, 0.80, 1.0), t)
}
fn cell_seed(ray: Vec3, density: f32) -> Option<(Vec2, Vec2, u32)> {
    if ray.y <= 0.0 || density <= 0.0 {
        return None;
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
        return None;
    }
    Some((p, cell, seed))
}

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
    #[test]
    fn star_tint_matches_star_cells_stays_warm_to_cool_and_deterministic() {
        let mut warm = 0;
        let mut cool = 0;
        for x in -100..100 {
            for z in -100..100 {
                let ray = Vec3::new(x as f32 / 100.0, 0.9, z as f32 / 100.0).normalize();
                let tint = star_tint(ray, 1.0);
                let value = brightness(ray, 1.0);
                if value > 0.0 {
                    assert!(
                        tint.min_element() >= 0.68 && tint.max_element() <= 1.0,
                        "bounded warm-to-cool chroma: {tint}"
                    );
                    assert_eq!(tint, star_tint(ray, 1.0));
                    if tint.x - 0.68 < 0.17 {
                        warm += 1;
                    } else {
                        cool += 1;
                    }
                } else {
                    assert_eq!(tint.is_finite(), true);
                }
            }
        }
        assert!(
            warm > 0 && cool > 0,
            "tint must spread over both ends {warm} {cool}"
        );
    }
}
