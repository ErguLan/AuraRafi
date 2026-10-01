//! Cross-family process memory ledger for optimization observability.
//!
//! Owners report residency into this aggregate so potato/desktop budgets and
//! OC-03 audits can compare meshes, UI images, atlas, and staging in one place.
//! Enforcement stays with each cache; this type only records what is resident.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessMemoryLedger {
    pub mesh_resident_bytes: u64,
    pub ui_gpu_image_bytes: u64,
    pub ui_cpu_image_bytes: u64,
    pub text_atlas_bytes: u64,
    pub staging_bytes: u64,
}

impl ProcessMemoryLedger {
    pub const fn total_resident_bytes(&self) -> u64 {
        self.mesh_resident_bytes
            .saturating_add(self.ui_gpu_image_bytes)
            .saturating_add(self.ui_cpu_image_bytes)
            .saturating_add(self.text_atlas_bytes)
            .saturating_add(self.staging_bytes)
    }

    pub const fn with_mesh_resident_bytes(mut self, bytes: u64) -> Self {
        self.mesh_resident_bytes = bytes;
        self
    }

    pub const fn with_ui_gpu_image_bytes(mut self, bytes: u64) -> Self {
        self.ui_gpu_image_bytes = bytes;
        self
    }

    pub const fn with_ui_cpu_image_bytes(mut self, bytes: u64) -> Self {
        self.ui_cpu_image_bytes = bytes;
        self
    }

    pub const fn with_text_atlas_bytes(mut self, bytes: u64) -> Self {
        self.text_atlas_bytes = bytes;
        self
    }

    pub const fn with_staging_bytes(mut self, bytes: u64) -> Self {
        self.staging_bytes = bytes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_saturate_across_resource_families() {
        let ledger = ProcessMemoryLedger::default()
            .with_mesh_resident_bytes(1024)
            .with_ui_gpu_image_bytes(2048)
            .with_ui_cpu_image_bytes(4096)
            .with_text_atlas_bytes(512)
            .with_staging_bytes(128);
        assert_eq!(
            ledger.total_resident_bytes(),
            1024 + 2048 + 4096 + 512 + 128
        );
    }

    #[test]
    fn total_saturates_instead_of_wrapping() {
        let ledger = ProcessMemoryLedger {
            mesh_resident_bytes: u64::MAX,
            ui_gpu_image_bytes: u64::MAX,
            ui_cpu_image_bytes: 0,
            text_atlas_bytes: 0,
            staging_bytes: 0,
        };
        assert_eq!(ledger.total_resident_bytes(), u64::MAX);
    }
}
