//! Native player host. It does not depend on the editor, Hub or Agent.
pub mod application;
mod audio;
pub mod session;
pub mod surface;
pub use application::{run, PlayerLaunch};
pub use session::{PlayerSession, PlayerSource};
