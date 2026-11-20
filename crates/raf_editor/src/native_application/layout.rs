//! Native editor layout requests derived from the project and dock state.

use raf_core::config::RenderQuality;
use raf_core::project::{Project, ProjectType};

use crate::editor_layout::EditorLayoutRequest;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_workbench::NativeGameWorkbench;

pub(super) fn should_queue_eager_hierarchy_refresh(
    quality: RenderQuality,
    selection_changed: bool,
) -> bool {
    selection_changed && quality != RenderQuality::Potato
}

pub(super) fn project_layout_request(
    project_type: ProjectType,
    logical_size: [f32; 2],
    project: Option<&Project>,
) -> EditorLayoutRequest {
    let request = match project_type {
        ProjectType::Game => EditorLayoutRequest::game(logical_size),
        ProjectType::Electronics => EditorLayoutRequest::electronics(logical_size),
    };
    let Some(project) = project else {
        return request;
    };
    EditorLayoutRequest {
        left_visible: project.settings.show_hierarchy_panel,
        right_visible: project.settings.show_properties_panel,
        ..request
    }
}

pub(super) fn synchronize_bottom_dock_layout(
    runtime: &mut NativeEditorRuntime,
    workbench: &NativeGameWorkbench,
) {
    let collapsed = workbench.bottom_dock_collapsed();
    if runtime.bottom_dock_is_collapsed() != collapsed {
        runtime.set_bottom_dock_collapsed(collapsed);
    }

    if !collapsed {
        let requested = workbench.bottom_dock_height();
        if (runtime.requested_bottom_dock_height() - requested).abs() > 0.5 {
            runtime.resize_bottom_dock(requested);
        }
    }
}
