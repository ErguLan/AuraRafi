use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorCameraMode {
    Orbit,
    Fly,
    Orthographic2D,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorCameraBookmark {
    pub name: String,
    pub mode: EditorCameraMode,
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub zoom_2d: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorCameraBlock {
    pub name: String,
    pub mode: EditorCameraMode,
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub offset_2d: [f32; 2],
    pub zoom_2d: f32,
    pub fov_degrees: f32,
    pub near_clip: f32,
    pub far_clip: f32,
    pub move_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
    pub bookmarks: Vec<EditorCameraBookmark>,
}

impl Default for EditorCameraBlock {
    fn default() -> Self {
        Self {
            name: "editor_camera".to_string(),
            mode: EditorCameraMode::Orbit,
            target: Vec3::ZERO,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.5,
            distance: 8.0,
            offset_2d: [0.0, 0.0],
            zoom_2d: 1.0,
            fov_degrees: 60.0,
            near_clip: 0.1,
            far_clip: 1000.0,
            move_sensitivity: 3.5,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
            bookmarks: Vec::new(),
        }
    }
}

impl EditorCameraBlock {
    pub fn sanitized(mut self) -> Self {
        self.pitch = self.pitch.clamp(-1.4, 1.4);
        self.distance = self.distance.clamp(0.5, 200.0);
        self.zoom_2d = self.zoom_2d.clamp(0.1, 50.0);
        self.fov_degrees = self.fov_degrees.clamp(20.0, 120.0);
        self.near_clip = self.near_clip.clamp(0.001, 10.0);
        self.far_clip = self.far_clip.max(self.near_clip + 1.0);
        self
    }

    pub fn bookmark(&self, name: impl Into<String>) -> EditorCameraBookmark {
        EditorCameraBookmark {
            name: name.into(),
            mode: self.mode,
            target: self.target,
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            zoom_2d: self.zoom_2d,
        }
    }

    pub fn with_bookmark(mut self, bookmark: EditorCameraBookmark) -> Self {
        self.bookmarks
            .retain(|existing| existing.name != bookmark.name);
        self.bookmarks.push(bookmark);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_block_sanitizes_runtime_values() {
        let block = EditorCameraBlock {
            pitch: 4.0,
            distance: 0.01,
            zoom_2d: 100.0,
            fov_degrees: 180.0,
            near_clip: -4.0,
            far_clip: 0.0,
            ..EditorCameraBlock::default()
        }
        .sanitized();

        assert_eq!(block.pitch, 1.4);
        assert_eq!(block.distance, 0.5);
        assert_eq!(block.zoom_2d, 50.0);
        assert_eq!(block.fov_degrees, 120.0);
        assert!(block.far_clip > block.near_clip);
    }
}
