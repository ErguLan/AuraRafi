//! Local presentation state. Never persisted or shared between players.
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScriptRole {
    #[default]
    Local,
    Client,
    Server,
}

#[derive(Debug, Default)]
pub struct RuntimeViewState {
    pub active_camera: Option<Uuid>,
    pub role: ScriptRole,
    pub(crate) environment_overrides: serde_json::Map<String, serde_json::Value>,
    pub(crate) environment_revision: u64,
    pub(crate) environment_cache: std::cell::RefCell<
        Option<(
            raf_core::environment::EnvironmentSettings,
            u64,
            raf_core::environment::EnvironmentSettings,
        )>,
    >,
}
impl RuntimeViewState {
    pub fn environment_revision(&self) -> u64 {
        self.environment_revision
    }
    pub fn environment(
        &self,
        world: raf_core::environment::EnvironmentSettings,
    ) -> raf_core::environment::EnvironmentSettings {
        if self.environment_overrides.is_empty() {
            return world;
        }
        if let Some((source, revision, resolved)) = *self.environment_cache.borrow() {
            if source == world && revision == self.environment_revision {
                return resolved;
            }
        }
        // Bounded to the core property catalog; unchanged views reuse the cached value.
        let resolved = world
            .with_view_overrides(&self.environment_overrides)
            .expect("Validated bounded view overrides");
        *self.environment_cache.borrow_mut() = Some((world, self.environment_revision, resolved));
        resolved
    }
}
