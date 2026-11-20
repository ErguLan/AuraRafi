//! World Program compiler, plan journal and persistent world brief.
//!
//! Kept as a child module of commands::game so the command kernel stays
//! readable without widening the public surface.

mod brief;
mod cancel;
mod commit;
mod compile;
mod kits;
mod plan;
mod script;
#[cfg(test)]
mod tests;

pub(crate) use brief::{execute_world_brief, execute_world_recipe};
pub(crate) use cancel::execute_cancel;
pub(crate) use commit::{apply_plan_to_scene, execute_commit};
pub(crate) use plan::{execute_plan, PlannedWorld};
pub(crate) use script::execute_world_script;

use super::*;

thread_local! {
    static ACTIVE_PROJECT_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Bind the current command dispatch to a project root so plan artifacts and
/// the world brief stay inside the active project. The value is thread-local:
/// editor UI dispatch and attached IPC dispatch never exchange it implicitly.
pub fn set_project_root(root: Option<PathBuf>) {
    ACTIVE_PROJECT_ROOT.with(|slot| *slot.borrow_mut() = root);
}

pub fn clear_project_root() {
    ACTIVE_PROJECT_ROOT.with(|slot| *slot.borrow_mut() = None);
}

/// Scoped project binding for plan artifacts and the world brief.
pub struct ProjectRootGuard {
    previous: Option<PathBuf>,
}

impl Drop for ProjectRootGuard {
    fn drop(&mut self) {
        ACTIVE_PROJECT_ROOT.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}

pub fn bind_project_root(root: Option<PathBuf>) -> ProjectRootGuard {
    let previous = ACTIVE_PROJECT_ROOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        std::mem::replace(&mut *slot, root)
    });
    ProjectRootGuard { previous }
}

pub(crate) fn active_project_root() -> Option<PathBuf> {
    ACTIVE_PROJECT_ROOT.with(|slot| slot.borrow().clone())
}
