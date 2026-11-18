use super::physics::PhysicsWorld;
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, Primitive, RigidBodyType, SceneGraph, SceneNodeId};
use raf_script::physics_api::PhysicsEventKind as K;

#[test]
fn physics_dynamic_box_ignores_empty_corner_of_capsule_bounds() {
    let mut scene = SceneGraph::new();
    let character = collider(&mut scene, "Character", Vec3::ZERO);
    let node = scene.get_mut(character).unwrap();
    node.rigid_body.enabled = true;
    node.rigid_body.body_type = RigidBodyType::Dynamic;
    node.rigid_body.is_character = true;
    node.character_controller = Some(raf_core::scene::CharacterController::default());
    let id = collider(&mut scene, "Near miss", Vec3::new(0.44, 0.0, 0.7));
    let node = scene.get_mut(id).unwrap();
    node.scale = Vec3::splat(0.08);
    node.rigid_body.enabled = true;
    node.rigid_body.body_type = RigidBodyType::Dynamic;
    node.rigid_body.damping = 0.0;
    node.rigid_body.velocity = -Vec3::Z * 2.6;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics
        .step_with_gravity(&mut scene, 0.1, Vec3::ZERO)
        .unwrap();
    assert!((scene.get(id).unwrap().position.z - 0.44).abs() < 1e-4);
    assert!(physics.impacts().is_empty());
}

#[test]
fn physics_sleeping_body_falls_when_only_a_wall_remains() {
    let mut scene = SceneGraph::new();
    collider(&mut scene, "Wall", Vec3::X);
    let id = collider(&mut scene, "Body", Vec3::ZERO);
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Dynamic;
    body.sleeping = true;
    body.sleep_gravity = Vec3::new(0.0, -9.81, 0.0);
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    assert!(scene.get(id).unwrap().position.y < 0.0);
}

#[test]
fn physics_capsule_pushes_dynamic_bodies_only_in_advanced_mode() {
    for advanced in [false, true] {
        let mut scene = SceneGraph::new();
        let character = collider(&mut scene, "Character", Vec3::ZERO);
        let node = scene.get_mut(character).unwrap();
        node.rigid_body.enabled = true;
        node.rigid_body.body_type = RigidBodyType::Dynamic;
        node.rigid_body.is_character = true;
        let mut controller = raf_core::scene::CharacterController::default();
        controller.state.desired_velocity = Vec3::X * 6.0;
        node.character_controller = Some(controller);
        let prop = collider(&mut scene, "Pushable", Vec3::X * 1.2);
        let node = scene.get_mut(prop).unwrap();
        node.rigid_body.enabled = true;
        node.rigid_body.body_type = RigidBodyType::Dynamic;
        let mut physics = PhysicsWorld::prepare(&scene).unwrap();
        physics
            .step_with_settings(&mut scene, 0.1, Vec3::ZERO, advanced)
            .unwrap();
        let speed = scene.get(prop).unwrap().rigid_body.velocity.x;
        assert_eq!(speed > 0.0, advanced, "mode={advanced}, push speed={speed}");
    }
}

#[test]
fn physics_material_applies_bounce_friction_and_reports_impact() {
    let mut scene = SceneGraph::new();
    let floor = collider(&mut scene, "Rough floor", Vec3::ZERO);
    let floor = scene.get_mut(floor).unwrap();
    floor.scale = Vec3::new(8.0, 1.0, 8.0);
    floor.rigid_body.friction = 1.0;
    let id = collider(&mut scene, "Bouncing block", Vec3::Y * 1.25);
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Dynamic;
    body.use_gravity = false;
    body.damping = 0.0;
    body.velocity = Vec3::new(2.0, -6.0, 0.0);
    body.friction = 1.0;
    body.restitution = 0.75;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 0.1).unwrap();
    let body = &scene.get(id).unwrap().rigid_body;
    assert!(body.velocity.y > 4.4, "{:?}", body.velocity);
    assert!(body.velocity.x < 1.9, "{:?}", body.velocity);
    assert!(physics
        .impacts()
        .iter()
        .any(|hit| hit.normal.y > 0.99 && hit.normal_impulse > 9.0));
}

#[test]
fn physics_compound_supports_parent_and_applies_angular_impulse() {
    let mut scene = SceneGraph::new();
    let floor = collider(&mut scene, "Floor", Vec3::ZERO);
    scene.get_mut(floor).unwrap().scale = Vec3::new(8.0, 1.0, 8.0);
    let owner = scene.add_root("Mass");
    let uuid = scene.get(owner).unwrap().uuid;
    let body = scene.get_mut(owner).unwrap();
    body.position.y = 4.0;
    body.rigid_body.enabled = true;
    body.rigid_body.body_type = RigidBodyType::Dynamic;
    let child = scene.add_child_with_primitive(owner, "Attached shape", Primitive::Cube);
    let node = scene.get_mut(child).unwrap();
    node.collider.collider_type = ColliderType::Aabb;
    node.collider.aabb = Aabb {
        min: Vec3::splat(-0.5),
        max: Vec3::splat(0.5),
    };
    node.collider.body_owner = Some(uuid);
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    for _ in 0..90 {
        physics.step(&mut scene, 1.0 / 60.0).unwrap();
    }
    assert!((scene.get(owner).unwrap().position.y - 1.0).abs() < 0.001);
    scene.get_mut(owner).unwrap().rigid_body.angular_impulse = Vec3::Y * 0.5;
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    assert!(scene.get(owner).unwrap().rotation.y.abs() > 0.1);
}

#[test]
fn physics_swept_sensor_respects_layers() {
    let mut scene = SceneGraph::new();
    let sensor = collider(&mut scene, "Thin sensor", Vec3::ZERO);
    let runner = collider(&mut scene, "Fast body", -Vec3::X * 5.0);
    let node = scene.get_mut(sensor).unwrap();
    node.scale.x = 0.01;
    node.rigid_body.enabled = true;
    node.rigid_body.is_trigger = true;
    node.collider.collision_layer = 2;
    let node = scene.get_mut(runner).unwrap();
    node.rigid_body.enabled = true;
    node.rigid_body.body_type = RigidBodyType::Dynamic;
    node.rigid_body.use_gravity = false;
    node.rigid_body.damping = 0.0;
    node.rigid_body.velocity = Vec3::X * 100.0;
    node.collider.collision_mask = 1;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 0.1).unwrap();
    assert!(physics.events().is_empty());

    let node = scene.get_mut(runner).unwrap();
    node.position.x = -5.0;
    node.collider.collision_mask = 2;
    physics.step(&mut scene, 0.1).unwrap();
    let events: Vec<_> = physics.events().iter().map(|event| event.kind).collect();
    assert_eq!(
        events,
        vec![
            K::TriggerEnter,
            K::TriggerEnter,
            K::TriggerExit,
            K::TriggerExit
        ]
    );
}

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
fn dynamic_body_passes_through_an_enabled_static_trigger() {
    let mut scene = SceneGraph::new();
    let zone = collider(&mut scene, "Zone", Vec3::ZERO);
    let trigger = &mut scene.get_mut(zone).unwrap().rigid_body;
    trigger.enabled = true;
    trigger.body_type = RigidBodyType::Static;
    trigger.is_trigger = true;

    let id = collider(&mut scene, "Body", Vec3::Y * 3.0);
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Dynamic;
    body.use_gravity = true;
    body.damping = 0.0;

    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    for _ in 0..120 {
        physics.step(&mut scene, 1.0 / 60.0).unwrap();
    }
    assert!(
        scene.get(id).unwrap().position.y < -5.0,
        "a dynamic body must not rest on an enabled static trigger"
    );
}

#[test]
fn physics_many_real_contacts_are_not_limited_to_128() {
    let mut scene = SceneGraph::new();
    for index in 0..18 {
        let id = collider(&mut scene, &format!("Zone{index}"), Vec3::ZERO);
        let body = &mut scene.get_mut(id).unwrap().rigid_body;
        body.enabled = true;
        body.is_trigger = true;
    }
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 0.02).unwrap();
    assert_eq!(physics.events().len(), 18 * 17);
}

#[test]
fn physics_sparse_scene_exceeds_old_pair_product_limit() {
    let mut scene = SceneGraph::new();
    for i in 0..500 {
        collider(
            &mut scene,
            &format!("Wall{i}"),
            Vec3::new(i as f32 * 5.0, -10.0, 0.0),
        );
    }
    for i in 0..10 {
        let id = collider(
            &mut scene,
            &format!("Moving{i}"),
            Vec3::new(i as f32 * 5.0, 5.0, 0.0),
        );
        let body = &mut scene.get_mut(id).unwrap().rigid_body;
        body.enabled = true;
        body.body_type = RigidBodyType::Dynamic;
    }
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..120 {
        physics.step(&mut scene, 1.0 / 60.0).unwrap();
    }
    let metrics = physics.spatial_metrics();
    assert_eq!(metrics.builds, 1, "{metrics:?}");
    assert!(metrics.visited < 500 * 10 * 120, "{metrics:?}");
    eprintln!(
        "500 fixed + 10 moving: 120 ticks in {:?}; {metrics:?}",
        start.elapsed()
    );
}

#[test]
fn physics_falls_inside_rotated_platform_broadphase_without_false_support() {
    let mut scene = SceneGraph::new();
    let platform = collider(&mut scene, "Ramp", Vec3::ZERO);
    let node = scene.get_mut(platform).unwrap();
    node.scale = Vec3::new(8.0, 0.2, 4.0);
    node.rotation.z = 30.0;
    let id = collider(&mut scene, "Player", Vec3::Y * 1.5);
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Dynamic;
    body.use_gravity = false;
    body.damping = 0.0;
    body.velocity = -Vec3::Y * 2.0;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 0.1).unwrap();
    assert!((scene.get(id).unwrap().position.y - 1.3).abs() < 1e-4);
    assert!(physics.events().is_empty());
}

#[test]
fn physics_basic_mode_leaves_objects_fixed_and_simulates_character() {
    let mut scene = SceneGraph::new();
    let ordinary = collider(&mut scene, "Object", Vec3::Y * 10.0);
    let character = collider(&mut scene, "Character", Vec3::new(10.0, 10.0, 0.0));
    for id in [ordinary, character] {
        let body = &mut scene.get_mut(id).unwrap().rigid_body;
        body.enabled = true;
        body.body_type = RigidBodyType::Dynamic;
    }
    scene.get_mut(character).unwrap().rigid_body.is_character = true;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    physics
        .step_with_settings(&mut scene, 0.1, Vec3::new(0.0, -9.81, 0.0), false)
        .unwrap();
    assert_eq!(scene.get(ordinary).unwrap().position.y, 10.0);
    assert!(scene.get(character).unwrap().position.y < 10.0);
}

#[test]
fn physics_character_leaves_edge_and_falls_into_void() {
    let mut scene = SceneGraph::new();
    collider(&mut scene, "Platform", Vec3::ZERO);
    let id = collider(&mut scene, "Character", Vec3::Y);
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    body.enabled = true;
    body.body_type = RigidBodyType::Dynamic;
    body.is_character = true;
    body.use_gravity = true;
    body.damping = 0.0;
    body.velocity = Vec3::X * 3.0;
    let mut physics = PhysicsWorld::prepare(&scene).unwrap();
    for _ in 0..120 {
        physics
            .step_with_settings(&mut scene, 1.0 / 60.0, Vec3::new(0.0, -9.81, 0.0), false)
            .unwrap();
    }
    let node = scene.get(id).unwrap();
    assert!(node.position.x > 5.0, "horizontal movement should continue");
    assert!(
        node.position.y < -3.0,
        "body should fall after leaving the platform"
    );
    assert!(node.rigid_body.velocity.y < 0.0);
}
