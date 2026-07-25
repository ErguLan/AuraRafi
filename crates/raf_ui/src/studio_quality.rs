//! Density and DPI contracts used by RafUI Studio.

use serde::{Deserialize, Serialize};

use crate::{UiColorMode, UiDensityContract, UiEnvironment};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiStudioDpiCase {
    pub scale_factor: f32,
    pub logical_size: [u32; 2],
    pub color_mode: UiColorMode,
}

impl UiStudioDpiCase {
    pub const fn new(scale_factor: f32, logical_size: [u32; 2], color_mode: UiColorMode) -> Self {
        Self {
            scale_factor,
            logical_size,
            color_mode,
        }
    }

    pub fn label(self) -> String {
        let mode = match self.color_mode {
            UiColorMode::System => "system",
            UiColorMode::Dark => "dark",
            UiColorMode::Light => "light",
        };
        format!("{}%/{mode}", (self.scale_factor * 100.0).round() as u32)
    }

    pub fn environment(self) -> UiEnvironment {
        UiEnvironment {
            viewport_size: [
                self.logical_size[0].max(1) as f32,
                self.logical_size[1].max(1) as f32,
            ],
            scale_factor: self.scale_factor,
            color_mode: self.color_mode,
            ..UiEnvironment::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStudioDpiMatrix {
    pub logical_size: [u32; 2],
    pub cases: Vec<UiStudioDpiCase>,
}

impl UiStudioDpiMatrix {
    pub fn new(logical_size: [u32; 2]) -> Self {
        let mut cases = Vec::with_capacity(8);
        for color_mode in [UiColorMode::Dark, UiColorMode::Light] {
            for scale_factor in [1.0, 1.25, 1.5, 2.0] {
                cases.push(UiStudioDpiCase::new(scale_factor, logical_size, color_mode));
            }
        }
        Self {
            logical_size,
            cases,
        }
    }

    pub fn reports(&self) -> Vec<UiStudioDpiReport> {
        self.cases
            .iter()
            .copied()
            .map(UiStudioDpiReport::from_case)
            .collect()
    }
}

impl Default for UiStudioDpiMatrix {
    fn default() -> Self {
        Self::new([1280, 720])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiStudioDpiReport {
    pub case: UiStudioDpiCase,
    pub physical_size: [u32; 2],
    pub density: UiDensityContract,
}

impl UiStudioDpiReport {
    pub fn from_case(case: UiStudioDpiCase) -> Self {
        let environment = case.environment();
        Self {
            case,
            physical_size: environment.physical_size(),
            density: environment.density_contract(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiStudioDensityReport {
    pub environment: UiEnvironment,
    pub contract: UiDensityContract,
    pub geometry_isolated: bool,
    pub text_isolated: bool,
    pub icons_isolated: bool,
}

impl UiStudioDensityReport {
    pub fn from_environment(environment: UiEnvironment) -> Self {
        let contract = environment.density_contract();
        Self {
            environment,
            contract,
            geometry_isolated: contract.geometry_scale != contract.text_scale,
            text_isolated: contract.text_scale >= contract.geometry_scale,
            icons_isolated: (contract.icon_scale - contract.text_scale).abs() > f32::EPSILON
                || contract.icon_scale == contract.geometry_scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiGeometrySnap, UiSamplingMode};

    #[test]
    fn matrix_covers_two_themes_and_four_display_densities() {
        let matrix = UiStudioDpiMatrix::new([800, 600]);
        assert_eq!(matrix.cases.len(), 8);
        assert_eq!(matrix.reports()[2].physical_size, [1200, 900]);
    }

    #[test]
    fn density_contract_does_not_apply_text_floor_to_geometry_or_icons() {
        let report = UiStudioDensityReport::from_environment(UiEnvironment {
            scale_factor: 1.0,
            ..UiEnvironment::default()
        });
        assert_eq!(report.contract.geometry_scale, 1.0);
        assert_eq!(report.contract.text_scale, 1.25);
        assert_eq!(report.contract.icon_scale, 1.0);
        assert_eq!(report.contract.geometry_snap, UiGeometrySnap::PhysicalPixel);
        assert_eq!(report.contract.icon_sampling, UiSamplingMode::Nearest);
    }
}
