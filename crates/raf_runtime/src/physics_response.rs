//! Translation-only depenetration; not a general impulse or angular solver.
use glam::Vec3;
use raf_core::scene::Aabb;

pub(super) fn depenetrate(
    mut body: Aabb,
    obstacles: &[Aabb],
    velocity: &mut Vec3,
) -> Result<(Aabb, Vec3), String> {
    let mut correction = Vec3::ZERO;
    for _ in 0..4 {
        let mut changed = false;
        for obstacle in obstacles {
            if !(0..3).all(|a| body.min[a] < obstacle.max[a] && body.max[a] > obstacle.min[a]) {
                continue;
            }
            let mut best_axis = 0;
            let mut best = f32::INFINITY;
            for axis in 0..3 {
                let negative = obstacle.min[axis] - body.max[axis];
                let positive = obstacle.max[axis] - body.min[axis];
                let shift = if negative.abs() <= positive.abs() {
                    negative
                } else {
                    positive
                };
                if shift.abs() < best.abs() {
                    best = shift;
                    best_axis = axis;
                }
            }
            if !best.is_finite() {
                return Err("Non-finite collider depenetration".into());
            }
            body.min[best_axis] += best;
            body.max[best_axis] += best;
            correction[best_axis] += best;
            velocity[best_axis] = 0.0;
            changed = true;
        }
        if !changed {
            return Ok((body, correction));
        }
    }
    if obstacles
        .iter()
        .any(|o| (0..3).all(|a| body.min[a] < o.max[a] - 1e-5 && body.max[a] > o.min[a] + 1e-5))
    {
        return Err("Initial overlap cannot be resolved within four translation passes".into());
    }
    Ok((body, correction))
}
