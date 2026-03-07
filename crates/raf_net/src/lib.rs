//! # raf_net
//!
//! Networking stubs for future multiplayer support in AuraRafi.
//! Currently provides protocol definitions and client/server interfaces.

pub mod protocol;

pub use protocol::{NetMessage, NetMessageType};
