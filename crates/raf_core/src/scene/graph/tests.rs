use super::*;

use glam::Vec3;

#[test]
fn hierarchy_basics() {
    let mut graph = SceneGraph::new();
    let root = graph.add_root("Root");
    let child = graph.add_child(root, "Child");

    assert_eq!(graph.len(), 2);
    assert_eq!(graph.roots().len(), 1);

    let root_node = graph.get(root).unwrap();
    assert_eq!(root_node.children.len(), 1);

    let child_node = graph.get(child).unwrap();
    assert_eq!(child_node.parent, Some(root));
}

#[test]
fn world_matrix_propagation() {
    let mut graph = SceneGraph::new();
    let root = graph.add_root("Root");
    graph.get_mut(root).unwrap().position = Vec3::new(10.0, 0.0, 0.0);

    let child = graph.add_child(root, "Child");
    graph.get_mut(child).unwrap().position = Vec3::new(0.0, 5.0, 0.0);

    let world = graph.world_matrix(child);
    let translation = world.col(3);
    assert!((translation.x - 10.0).abs() < 0.001);
    assert!((translation.y - 5.0).abs() < 0.001);
}

#[test]
fn reparent_preserves_world_transform() {
    let mut graph = SceneGraph::new();
    let source = graph.add_root("Source");
    graph.get_mut(source).unwrap().position = Vec3::new(10.0, 2.0, -4.0);
    let target = graph.add_root_folder("Target");
    graph.get_mut(target).unwrap().position = Vec3::new(-3.0, 5.0, 7.0);
    let child = graph.add_child_with_primitive(source, "Child", Primitive::Cube);
    graph.get_mut(child).unwrap().position = Vec3::new(2.0, 1.0, 3.0);

    let before = graph.world_matrix(child).to_cols_array();
    assert!(graph.reparent_node_preserve_world_transform(child, Some(target)));
    let after = graph.world_matrix(child).to_cols_array();

    assert_eq!(graph.get(child).unwrap().parent, Some(target));
    for (before, after) in before.iter().zip(after) {
        assert!((*before - after).abs() < 0.001);
    }
}

#[test]
fn render_fingerprint_tracks_visual_changes_only() {
    let mut graph = SceneGraph::new();
    let node = graph.add_root_with_primitive("Cube", Primitive::Cube);
    let initial = graph.render_fingerprint();

    graph
        .get_mut(node)
        .unwrap()
        .scripts
        .push("logic.rhai".to_string());
    assert_eq!(graph.render_fingerprint(), initial);

    graph.get_mut(node).unwrap().position.x = 4.0;
    assert_ne!(graph.render_fingerprint(), initial);
}

#[test]
fn hierarchy_fingerprint_ignores_animation_but_tracks_authoring_changes() {
    let mut graph = SceneGraph::new();
    let node = graph.add_root_with_primitive("Car", Primitive::Cube);
    let initial = graph.hierarchy_fingerprint();
    let render_initial = graph.node_render_fingerprint(node);

    {
        let car = graph.get_mut(node).unwrap();
        car.position.x = 12.0;
        car.rotation.y = 45.0;
        car.color.r = 20;
    }
    assert_eq!(graph.hierarchy_fingerprint(), initial);
    assert_ne!(graph.node_render_fingerprint(node), render_initial);

    graph.get_mut(node).unwrap().name = "Race car".to_string();
    assert_ne!(graph.hierarchy_fingerprint(), initial);

    let renamed = graph.hierarchy_fingerprint();
    graph.get_mut(node).unwrap().visible = false;
    assert_ne!(graph.hierarchy_fingerprint(), renamed);
}

#[test]
fn legacy_sprite_primitive_deserializes_as_plane() {
    let primitive: Primitive = ron::from_str("Sprite2D").expect("legacy primitive alias");
    assert_eq!(primitive, Primitive::Plane);
}

#[test]
fn reparent_before_preserves_sibling_order_and_rejects_cycles() {
    let mut graph = SceneGraph::new();
    let root = graph.add_root("Root");
    let first = graph.add_child(root, "First");
    let second = graph.add_child(root, "Second");
    let third = graph.add_child(root, "Third");

    assert!(graph.reparent_node_before(third, Some(root), Some(first)));
    assert_eq!(
        graph.get(root).unwrap().children,
        vec![third, first, second]
    );
    assert!(!graph.reparent_node_before(root, Some(third), None));
}

#[test]
fn multi_reparent_preserves_source_order() {
    let mut graph = SceneGraph::new();
    let root = graph.add_root("Root");
    let first = graph.add_child(root, "First");
    let second = graph.add_child(root, "Second");
    let third = graph.add_child(root, "Third");
    let target = graph.add_root("Target");
    let before = graph.add_child(target, "Before");

    assert!(graph.reparent_nodes_before(&[first, second], Some(target), Some(before)));
    assert_eq!(
        graph.get(target).unwrap().children,
        vec![first, second, before]
    );
    assert_eq!(graph.get(root).unwrap().children, vec![third]);
}

#[test]
fn multi_reparent_rejects_ancestor_and_descendant_selection() {
    let mut graph = SceneGraph::new();
    let root = graph.add_root("Root");
    let child = graph.add_child(root, "Child");
    let target = graph.add_root("Target");

    assert!(!graph.reparent_nodes_before(&[root, child], Some(target), None));
    assert_eq!(graph.roots(), &[root, target]);
    assert_eq!(graph.get(root).unwrap().children, vec![child]);
}

#[test]
fn duplicate_into_preserves_subtree_under_requested_parent() {
    let mut graph = SceneGraph::new();
    let source = graph.add_root_folder("Source");
    graph.add_child(source, "Nested");
    let target = graph.add_root_folder("Target");

    let duplicate = graph.duplicate_node_into(source, Some(target)).unwrap();

    assert_eq!(graph.get(duplicate).unwrap().parent, Some(target));
    assert_eq!(graph.get(target).unwrap().children, vec![duplicate]);
    assert_eq!(graph.get(duplicate).unwrap().children.len(), 1);
    assert_ne!(
        graph.get(source).unwrap().uuid,
        graph.get(duplicate).unwrap().uuid
    );
}

#[test]
fn render_fingerprint_tracks_child_order() {
    let mut graph = SceneGraph::new();
    let root = graph.add_root("Root");
    let first = graph.add_child(root, "First");
    let second = graph.add_child(root, "Second");
    let before = graph.render_fingerprint();

    assert!(graph.reparent_node_before(second, Some(root), Some(first)));
    assert_ne!(graph.render_fingerprint(), before);
}

#[test]
fn all_live_ids_excludes_soft_removed_slots_but_keeps_hidden_nodes() {
    let mut graph = SceneGraph::new();
    let hidden = graph.add_root("Hidden");
    graph.get_mut(hidden).unwrap().visible = false;
    let removed = graph.add_root("Removed");
    assert!(graph.remove_node(removed));

    assert_eq!(graph.all_live_ids(), vec![hidden]);
    assert_eq!(graph.all_valid_ids(), Vec::<SceneNodeId>::new());
}

#[test]
fn document_revision_advances_without_hashing_the_graph() {
    let mut graph = SceneGraph::new();
    let initial = graph.document_revision();
    let node = graph.add_root("Root");
    let added = graph.document_revision();
    assert_ne!(added, initial);

    graph.get_mut(node).unwrap().name = "Renamed".to_string();
    assert_ne!(graph.document_revision(), added);
}
