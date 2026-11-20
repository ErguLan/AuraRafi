use super::*;
use super::hierarchy::hierarchy_context_selection_intent;

#[cfg(test)]
mod hierarchy_context_selection_tests {
    use super::*;

    #[test]
    fn context_menu_selects_an_unselected_entity() {
        let target = SceneNodeId(7);

        assert_eq!(
            hierarchy_context_selection_intent(target, &[SceneNodeId(3)]),
            Some(NativeWorkbenchIntent::Command(
                "hierarchy.select:7:replace".to_string()
            ))
        );
    }

    #[test]
    fn context_menu_preserves_a_multi_selection_containing_its_target() {
        let target = SceneNodeId(7);

        assert_eq!(
            hierarchy_context_selection_intent(target, &[SceneNodeId(3), target]),
            None
        );
    }
}
