//! Allocation-free improved light response. Current technology bypasses all of this.
use glam::Vec3;

pub(crate) fn direct(shade: f32, steps: f32, softness: f32) -> f32 {
    if steps < 2.0 {
        return shade;
    }
    let x = shade * steps;
    let band = x.floor();
    let half = softness.clamp(0.0, 1.0) * 0.5;
    if half <= 0.0 {
        return (x + 0.5).floor() / steps;
    }
    let t = ((x - band - 0.5 + half) / (2.0 * half)).clamp(0.0, 1.0);
    ((band + t * t * (3.0 - 2.0 * t)) / steps).clamp(0.0, 1.0)
}
pub(crate) fn ambient(color: Vec3, normal_y: f32) -> Vec3 {
    let upper = normal_y.clamp(-1.0, 1.0) * 0.5 + 0.5;
    (color * Vec3::new(0.45, 0.50, 0.62)).lerp(color, upper)
}
/// A continuous highlight shoulder; no HDR framebuffer or extra tone-map pass.
pub(crate) fn expose(color: Vec3, exposure: f32) -> Vec3 {
    (color * exposure).map(|v| {
        if v <= 0.8 {
            v.max(0.0)
        } else {
            0.8 + 0.2 * (v - 0.8) / (v - 0.6)
        }
    })
}

#[derive(Clone, Copy)]
pub struct SurfaceLighting {
    pub ambient: Vec3,
    pub sun: Vec3,
    pub steps: f32,
    pub softness: f32,
    pub exposure: f32,
}
impl SurfaceLighting {
    pub fn current(ambient: Vec3, sun: Vec3, steps: f32) -> Self {
        Self {
            ambient,
            sun,
            steps,
            softness: 0.0,
            exposure: 0.0,
        }
    }
}

pub(crate) const WGSL: &str = r#"
fn improved_direct(shade: f32, steps: f32, softness: f32) -> f32 {
    if steps < 2.0 { return shade; }
    let x = shade*steps; let band = floor(x); let half_width = clamp(softness,0.0,1.0)*0.5;
    if half_width <= 0.0 { return floor(x+0.5)/steps; }
    let t = clamp((x-band-0.5+half_width)/(2.0*half_width),0.0,1.0);
    return clamp((band+t*t*(3.0-2.0*t))/steps,0.0,1.0);
}
fn improved_ambient(color: vec3<f32>, normal_y: f32) -> vec3<f32> {
    return mix(color*vec3<f32>(0.45,0.50,0.62),color,clamp(normal_y,-1.0,1.0)*0.5+0.5);
}
fn improved_expose(color: vec3<f32>, exposure: f32) -> vec3<f32> {
    let v = max(color*exposure,vec3<f32>(0.0));
    return select(vec3<f32>(0.8)+0.2*(v-0.8)/max(v-0.6,vec3<f32>(0.00001)),v,v<=vec3<f32>(0.8));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimized_light_response_is_bounded_monotonic_and_preserves_midtones() {
        assert_eq!(ambient(Vec3::ONE, 1.0), Vec3::ONE);
        assert_eq!(ambient(Vec3::ONE, -1.0), Vec3::new(0.45, 0.50, 0.62));
        assert_eq!(expose(Vec3::splat(0.5), 1.0), Vec3::splat(0.5));
        let mut previous = Vec3::ZERO;
        for i in 0..1000 {
            let next = expose(Vec3::splat(i as f32 * 0.01), 4.0);
            assert!(
                next.is_finite()
                    && next.min_element() >= previous.min_element()
                    && next.max_element() <= 1.0
            );
            previous = next;
        }
        for steps in 2..=6 {
            let steps = steps as f32;
            assert_eq!(direct(0.0, steps, 0.35), 0.0);
            assert_eq!(direct(1.0, steps, 0.35), 1.0);
            for i in 0..steps as usize {
                let boundary = (i as f32 + 0.5) / steps;
                assert!(
                    (direct(boundary - 1e-5, steps, 0.35) - direct(boundary + 1e-5, steps, 0.35))
                        .abs()
                        < 0.001
                );
            }
            for i in 0..=100 {
                let s = i as f32 / 100.0;
                assert_eq!(direct(s, steps, 0.0), (s * steps + 0.5).floor() / steps);
            }
        }
    }
}
