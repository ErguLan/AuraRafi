//! Conservative scene visibility for the shared Scene viewport.
//!
//! This module owns only the decision to submit an object. Quality and frame
//! budgets may reduce optional work or output resolution, but they must never
//! make an otherwise visible editor object disappear.

use glam::{Mat4, Vec3};

use crate::math::frustum::Frustum;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneObjectBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl SceneObjectBounds {
    /// Builds a conservative world AABB without transforming every mesh
    /// vertex. The absolute model basis projects the local half extents onto
    /// the world axes, so rotation, non-uniform scale, and parent transforms
    /// are all included.
    pub fn from_local_aabb(local_min: Vec3, local_max: Vec3, model: Mat4) -> Self {
        let local_center = (local_min + local_max) * 0.5;
        let local_half_extent = (local_max - local_min).abs() * 0.5;
        let center = model.transform_point3(local_center);
        let extent = model.x_axis.truncate().abs() * local_half_extent.x
            + model.y_axis.truncate().abs() * local_half_extent.y
            + model.z_axis.truncate().abs() * local_half_extent.z;

        Self {
            min: center - extent,
            max: center + extent,
        }
    }

    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldStreamVisibility {
    pub enabled: bool,
    pub region_size: f32,
    pub load_radius: u32,
}

impl Default for WorldStreamVisibility {
    fn default() -> Self {
        Self {
            enabled: false,
            region_size: 128.0,
            load_radius: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneVisibility {
    Visible,
    OutsideFrustum,
    OutsideStream,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SceneVisibilityPolicy {
    pub world_stream: WorldStreamVisibility,
}

impl SceneVisibilityPolicy {
    pub fn classify(
        self,
        frustum: &Frustum,
        camera_position: Vec3,
        bounds: SceneObjectBounds,
    ) -> SceneVisibility {
        if !self.intersects_loaded_regions(camera_position, bounds) {
            return SceneVisibility::OutsideStream;
        }
        if !frustum.intersects_aabb(bounds.min, bounds.max) {
            return SceneVisibility::OutsideFrustum;
        }
        SceneVisibility::Visible
    }

    fn intersects_loaded_regions(self, camera_position: Vec3, bounds: SceneObjectBounds) -> bool {
        if !self.world_stream.enabled {
            return true;
        }

        let region_size = self.world_stream.region_size.clamp(16.0, 1024.0);
        let radius = self.world_stream.load_radius.max(1) as i32;
        let camera_region_x = (camera_position.x / region_size).floor() as i32;
        let camera_region_z = (camera_position.z / region_size).floor() as i32;
        let loaded_min_x = (camera_region_x - radius) as f32 * region_size;
        let loaded_max_x = (camera_region_x + radius + 1) as f32 * region_size;
        let loaded_min_z = (camera_region_z - radius) as f32 * region_size;
        let loaded_max_z = (camera_region_z + radius + 1) as f32 * region_size;

        bounds.max.x >= loaded_min_x
            && bounds.min.x <= loaded_max_x
            && bounds.max.z >= loaded_min_z
            && bounds.min.z <= loaded_max_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    #[test]
    fn transformed_bounds_contain_every_rotated_scaled_corner() {
        let local_min = Vec3::new(-1.0, -2.0, -0.5);
        let local_max = Vec3::new(1.0, 2.0, 0.5);
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(4.0, 0.5, 3.0),
            Quat::from_rotation_y(0.7),
            Vec3::new(8.0, -1.0, 3.0),
        );
        let bounds = SceneObjectBounds::from_local_aabb(local_min, local_max, model);

        for x in [local_min.x, local_max.x] {
            for y in [local_min.y, local_max.y] {
                for z in [local_min.z, local_max.z] {
                    let point = model.transform_point3(Vec3::new(x, y, z));
                    assert!(point.cmpge(bounds.min - Vec3::splat(1.0e-5)).all());
                    assert!(point.cmple(bounds.max + Vec3::splat(1.0e-5)).all());
                }
            }
        }
    }

    #[test]
    fn streaming_keeps_a_large_object_that_crosses_the_loaded_area() {
        let policy = SceneVisibilityPolicy {
            world_stream: WorldStreamVisibility {
                enabled: true,
                region_size: 100.0,
                load_radius: 1,
            },
        };
        let crossing = SceneObjectBounds {
            min: Vec3::new(150.0, -1.0, -5.0),
            max: Vec3::new(260.0, 1.0, 5.0),
        };
        let distant = SceneObjectBounds {
            min: Vec3::new(250.0, -1.0, -5.0),
            max: Vec3::new(260.0, 1.0, 5.0),
        };

        assert!(policy.intersects_loaded_regions(Vec3::ZERO, crossing));
        assert!(!policy.intersects_loaded_regions(Vec3::ZERO, distant));
    }
}
