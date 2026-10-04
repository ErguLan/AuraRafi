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
}
