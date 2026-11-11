//! Compression preserves authored minima; an impossible row overflows rather
//! than silently producing undersized hit targets.
pub(super) fn compress_tracks(preferred: &[f32], floors: &[f32], available: f32) -> Vec<f32> {
    let minimum_total = floors.iter().sum::<f32>();
    if minimum_total >= available {
        return floors.to_vec();
    }
    let flexible = preferred
        .iter()
        .zip(floors)
        .map(|(size, minimum)| (size - minimum).max(0.0))
        .sum::<f32>();
    if flexible <= f32::EPSILON {
        return preferred.to_vec();
    }
    let fraction = ((available - minimum_total) / flexible).clamp(0.0, 1.0);
    preferred
        .iter()
        .zip(floors)
        .map(|(size, minimum)| minimum + (size - minimum).max(0.0) * fraction)
        .collect()
}
