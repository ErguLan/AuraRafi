//! Editor viewport view-mode access for scripts.
//!
//! Runtime Play scripts do not own the authoring camera, so these functions are
//! available only when the editor attaches an explicit view context before
//! execution. The request is one-shot and applied by the editor host after the
//! script returns.

use raf_core::project::EditorViewMode;

use crate::{ScriptContext, ScriptError, ScriptResult};

impl ScriptContext<'_> {
    pub fn viewport_view_mode(&self) -> ScriptResult<EditorViewMode> {
        self.view.editor_view.ok_or_else(|| {
            ScriptError::InvalidArgument(
                "viewport view mode is only available in editor scripts".into(),
            )
        })
    }

    pub fn set_viewport_view_mode(&mut self, mode: EditorViewMode) -> ScriptResult<()> {
        if self.view.editor_view.is_none() {
            return Err(ScriptError::InvalidArgument(
                "viewport view mode is only available in editor scripts".into(),
            ));
        }
        self.view.editor_view = Some(mode);
        self.view.requested_editor_view = Some(mode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::ScriptEventQueue;
    use crate::view::RuntimeViewState;
    use crate::{AudioCommandQueue, InputSnapshot, TimeInfo};
    use raf_core::scene::SceneGraph;

    fn context(scene: &mut SceneGraph, run: impl FnOnce(&mut ScriptContext<'_>)) {
        let mut view = RuntimeViewState::default();
        let input = InputSnapshot::default();
        let mut audio = AudioCommandQueue::default();
        let mut events = ScriptEventQueue::default();
        run(&mut ScriptContext {
            scene,
            view: &mut view,
            input: &input,
            audio: &mut audio,
            events: &mut events,
            time: TimeInfo::default(),
            instance_id: 1,
            owner: None,
            behavior_source: None,
            entity_limit: 16,
            cancellation: None,
            deadline: None,
        });
    }

    #[test]
    fn editor_scripts_read_and_request_the_editor_view() {
        let mut scene = SceneGraph::new();
        context(&mut scene, |ctx| {
            assert!(ctx.viewport_view_mode().is_err());
            ctx.view.set_editor_view_context(EditorViewMode::View3d);
            assert_eq!(ctx.viewport_view_mode().unwrap(), EditorViewMode::View3d);
            ctx.set_viewport_view_mode(EditorViewMode::View2d).unwrap();
            assert_eq!(
                ctx.view.take_editor_view_request(),
                Some(EditorViewMode::View2d)
            );
            assert_eq!(ctx.view.take_editor_view_request(), None);
        });
    }
}
