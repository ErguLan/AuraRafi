use super::physics::PhysicsWorld;
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, Primitive, RigidBodyType, SceneGraph, SceneNodeId};
use raf_script::physics_api::PhysicsEventKind as K;

fn collider(scene: &mut SceneGraph, name: &str, position: Vec3) -> SceneNodeId {
    let id = scene.add_root_with_primitive(name, Primitive::Cube);
    let node = scene.get_mut(id).unwrap();
    node.position = position;
    node.collider.collider_type = ColliderType::Aabb;
    node.collider.aabb = Aabb {
        min: Vec3::splat(-0.5),
        max: Vec3::splat(0.5),
    };
    id
}
#[test]
fn physics_trigger_lifecycle_roster_and_removed_identity() {
    let mut scene = SceneGraph::new();
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    let zone = collider(&mut scene, "Zone", Vec3::ZERO);
    let other = collider(&mut scene, "Other", Vec3::ZERO);
    let body = &mut scene.get_mut(zone).unwrap().rigid_body;
    body.enabled = true;
    body.is_trigger = true;
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    assert_eq!(physics.events().len(), 2);
    assert!(physics.events().iter().all(|e| e.kind == K::TriggerEnter));
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    assert!(physics.events().iter().all(|e| e.kind == K::TriggerStay));
    let uuid = scene.get(other).unwrap().uuid;
    scene.remove_node(other);
    let replacement = collider(&mut scene, "Replacement", Vec3::splat(10.0));
    assert_ne!(uuid, scene.get(replacement).unwrap().uuid);
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    assert_eq!(physics.events().len(), 2);
    assert!(physics.events().iter().all(|e| e.kind == K::TriggerExit));
    assert!(physics.events().iter().any(|e| e.other == uuid));
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    assert!(physics.events().is_empty());
}
#[test]
fn physics_sweep_floor_parented_motion_and_initial_penetration() {
    let mut scene = SceneGraph::new();
    collider(&mut scene, "Floor", Vec3::ZERO);
    let id = collider(&mut scene, "Body", Vec3::Y * 2.0);
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Dynamic;
    body.use_gravity = false;
    body.damping = 0.0;
    body.velocity = -Vec3::Y * 100.0;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 0.05).unwrap();
    assert!((scene.get(id).unwrap().position.y - 1.0).abs() < 1e-5);
    assert_eq!(scene.get(id).unwrap().rigid_body.velocity.y, 0.0);
    assert!(physics.events().iter().any(|e| e.kind == K::CollisionEnter));
    scene.get_mut(id).unwrap().position.y = 0.75;
    physics.step(&mut scene, 0.05).unwrap();
    assert!((scene.get(id).unwrap().position.y - 1.0).abs() < 1e-5);
    let parent = scene.add_root("Parent");
    scene.get_mut(parent).unwrap().scale = Vec3::splat(2.0);
    let moving = scene.add_child(parent, "Kinematic");
    let body = &mut scene.get_mut(moving).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Kinematic;
    body.velocity = Vec3::X * 4.0;
    physics.step(&mut scene, 0.25).unwrap();
    assert!((scene.world_matrix(moving).transform_point3(Vec3::ZERO).x - 1.0).abs() < 1e-5);
}
#[test]
fn physics_contact_flood_is_explicit_and_has_no_partial_events() {
    let mut scene = SceneGraph::new();
    for index in 0..18 {
        let id = collider(&mut scene, &format!("Zone{index}"), Vec3::ZERO);
        let body = &mut scene.get_mut(id).unwrap().rigid_body;
        body.enabled = true;
        body.is_trigger = true;
    }
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    assert!(physics
        .step(&mut scene, 0.02)
        .unwrap_err()
        .contains("contact budget"));
    assert!(physics.events().is_empty());
}
