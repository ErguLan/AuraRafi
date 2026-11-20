//! Native workbench input routing.
//!
//! Input dispatch is kept separate from workbench state and retained-surface
//! composition so the main workbench remains a coordinator rather than a
//! monolithic event handler.

use super::*;

#[path = "native_workbench_input/assets.rs"]
mod assets;
#[path = "native_workbench_input/chrome.rs"]
mod chrome;
#[path = "native_workbench_input/electronics.rs"]
mod electronics;
#[path = "native_workbench_input/hierarchy.rs"]
mod hierarchy;
#[path = "native_workbench_input/layout.rs"]
mod layout;
#[path = "native_workbench_input/nodes.rs"]
mod nodes;
#[path = "native_workbench_input/project.rs"]
mod project;
#[path = "native_workbench_input/routing.rs"]
mod routing;
#[path = "native_workbench_input/text_fields.rs"]
mod text_fields;

pub(crate) use routing::parse_agent_action;

impl NativeGameWorkbench {
    pub fn process_input<F>(
        &mut self,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        project: Option<&Project>,
        resolve: F,
    ) -> (Vec<NativeWorkbenchIntent>, Vec<UiDispatchedAction>)
    where
        F: FnMut(&str) -> String,
    {
        self.process_input_routed(native_input, router, scene, selected, project, resolve)
    }
}

#[cfg(test)]
#[path = "native_workbench_input/tests.rs"]
mod tests;
