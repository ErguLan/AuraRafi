//! Machine-level hardware snapshot used by `doctor` and benchmark reports.
//!
//! This intentionally reports only stable, cross-platform values. GPU details
//! are supplied by the attached editor, which owns the live graphics device.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HardwareReport {
    pub os: String,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub arch: String,
    pub cpu_brand: Option<String>,
    pub cpu_cores: usize,
    pub ram_total_mb: Option<u64>,
    pub ram_available_mb: Option<u64>,
}

/// Reads the current machine snapshot. Cheap enough for an explicit doctor
/// call; never call it per frame.
pub fn report() -> HardwareReport {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.refresh_cpu_all();
    let cpu_brand = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|brand| !brand.is_empty());
    let cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or_else(|_| system.cpus().len());
    HardwareReport {
        os: std::env::consts::OS.to_string(),
        os_version: sysinfo::System::os_version(),
        kernel_version: sysinfo::System::kernel_version(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_brand,
        cpu_cores: cores,
        ram_total_mb: Some(bytes_to_mb(system.total_memory())),
        ram_available_mb: Some(bytes_to_mb(system.available_memory())),
    }
}

fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_a_usable_machine_shape() {
        let report = report();
        assert!(!report.os.is_empty());
        assert!(!report.arch.is_empty());
        assert!(report.cpu_cores > 0);
        assert!(report.ram_total_mb.unwrap_or(0) > 0);
    }
}
