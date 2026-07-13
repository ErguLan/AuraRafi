use serde::{Deserialize, Serialize};

use crate::geometry::{UiRect, UiSpacing};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiFlow {
    None,
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiPositionMode {
    Flow,
    Absolute,
    Docked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiLayout {
    pub rect: Option<UiRect>,
    pub flow: UiFlow,
    pub padding: UiSpacing,
    pub gap: f32,
    pub basis: [f32; 2],
    pub grow: f32,
    #[serde(default)]
    pub position_mode: UiPositionMode,
    #[serde(default)]
    pub z_index: i16,
    #[serde(default)]
    pub min_size: [f32; 2],
    #[serde(default)]
    pub max_size: [f32; 2],
}

impl Default for UiPositionMode {
    fn default() -> Self {
        Self::Flow
    }
}

impl Default for UiLayout {
    fn default() -> Self {
        Self {
            rect: None,
            flow: UiFlow::None,
            padding: UiSpacing::ZERO,
            gap: 0.0,
            basis: [0.0, 0.0],
            grow: 0.0,
            position_mode: UiPositionMode::Flow,
            z_index: 0,
            min_size: [0.0, 0.0],
            max_size: [0.0, 0.0],
        }
    }
}

impl UiLayout {
    pub fn fill(flow: UiFlow) -> Self {
        Self {
            grow: 1.0,
            flow,
            ..Self::default()
        }
    }

    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            basis: [width.max(0.0), height.max(0.0)],
            ..Self::default()
        }
    }

    pub fn absolute(rect: UiRect) -> Self {
        Self {
            rect: Some(rect),
            basis: [rect.width, rect.height],
            position_mode: UiPositionMode::Absolute,
            ..Self::default()
        }
    }

    pub fn with_z_index(mut self, z_index: i16) -> Self {
        self.z_index = z_index;
        self
    }
}
