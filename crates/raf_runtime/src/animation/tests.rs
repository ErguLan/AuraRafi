use super::*;
use raf_core::config::ScriptExecutionMode;
use raf_core::project::ProjectSettings;
use raf_core::runtime_config::RuntimePreferences;
use raf_core::scene::{Aabb, RigidBodyType};
use std::sync::{atomic::AtomicBool, Arc};

fn rig() -> (SceneGraph, SceneNodeId, SceneNodeId) {
    let mut scene = SceneGraph::new();
    let owner = scene.add_root("Puppet");
    let head = scene.add_child(owner, "Head");
    scene.get_mut(owner).unwrap().animation = Some(serde_json::from_value(serde_json::json!({
        "layers": [
            {"name":"Base","duration":1,"tracks":[{"path":["Head"],"channel":"position","stepped":true,
                "keys":[{"time":0,"value":[0,1,0]},{"time":0.5,"value":[0,2,0]}]}]},
            {"name":"Gesture","duration":0.5,"start":0.25,"fade_in":0.25,"fade_out":0.25,"additive":true,
                "tracks":[{"path":["Head"],"channel":"position","keys":[{"time":0,"value":[1,0,0]}]}]}
        ],
        "impact":{"path":["Head"],"duration":0.25,"meters_per_impulse":0.5,"max_offset":0.2}
    })).unwrap());
    (scene, owner, head)
}

#[test]
fn animation_round_trip_composites_stepped_pose_and_timed_gesture() {
    let (scene, _, head) = rig();
    let data = ron::to_string(&scene).unwrap();
    let loaded: SceneGraph = ron::from_str(&data).unwrap();
    let mut playback = AnimationPlayback::prepare(&loaded).unwrap();
    for (time, position) in [
        (0.125, Vec3::Y),
        (0.375, Vec3::new(0.5, 1.0, 0.0)),
        (0.5, Vec3::new(1.0, 2.0, 0.0)),
        (0.75, 2.0 * Vec3::Y),
    ] {
        let pose = playback
            .sample(time, 0.0)
            .unwrap()
            .find(|pose| pose.node == head)
            .unwrap();
        assert_eq!(pose.transform.position, position);
    }
    assert!(playback.sample(f64::NAN, 0.0).is_err());
    assert_eq!(loaded.get(head).unwrap().position, Vec3::ZERO);
}

#[test]
fn animation_impact_resolves_local_direction_and_restores_the_clip() {
    let (mut scene, owner, head) = rig();
    scene.get_mut(owner).unwrap().rotation.y = 90.0;
    let owner_uuid = scene.get(owner).unwrap().uuid;
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    let impact = PhysicsImpact {
        body: Uuid::new_v4(),
        other: owner_uuid,
        point: Vec3::ZERO,
        normal: Vec3::X,
        normal_impulse: 4.0,
    };
    playback.tick(&mut scene, 0.1, 1.0 / 60.0, &[impact], Some(-Vec3::Y));
    assert!(scene
        .get(head)
        .unwrap()
        .position
        .abs_diff_eq(Vec3::new(0.0, 1.0, -0.2), 1e-5));
    playback.tick(&mut scene, 0.35, 0.25, &[], Some(-Vec3::Y));
    assert!(scene
        .get(head)
        .unwrap()
        .position
        .abs_diff_eq(Vec3::new(0.4, 1.0, 0.0), 1e-5));
}

#[test]
fn animation_blends_locomotion_from_planar_velocity() {
    let (mut scene, owner, head) = rig();
    let node = scene.get_mut(owner).unwrap();
    node.rigid_body.enabled = true;
    node.rigid_body.body_type = RigidBodyType::Kinematic;
    node.rigid_body.velocity = Vec3::new(2.0, 10.0, 0.0);
    let rig = node.animation.as_mut().unwrap();
    rig.layers.truncate(1);
    rig.layers[0].looping = true;
    rig.layers[0].motion_speed = Some(0.0);
    let mut run = rig.layers[0].clone();
    run.name = "Run".into();
    run.motion_speed = Some(4.0);
    run.tracks[0].keys[0].value.x = 1.0;
    rig.layers.push(run);
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    playback.tick(&mut scene, 0.1, 0.1, &[], Some(-Vec3::Y));
    assert_eq!(scene.get(head).unwrap().position, Vec3::new(0.5, 1.0, 0.0));
}

#[test]
fn animation_rejects_ambiguous_bindings_and_conflicting_physics() {
    let (mut scene, owner, _) = rig();
    let extra = scene.add_child(owner, "Head");
    assert!(AnimationPlayback::prepare(&scene)
        .err()
        .unwrap()
        .contains("ambiguous"));
    scene.get_mut(extra).unwrap().name = "Other".into();
    let head = resolve_path(&scene, owner, &["Head".into()]).unwrap();
    let collider = scene.add_child(head, "Collider");
    let node = scene.get_mut(collider).unwrap();
    node.collider.collider_type = ColliderType::Aabb;
    node.collider.aabb = Aabb {
        min: Vec3::splat(-0.5),
        max: Vec3::splat(0.5),
    };
    node.rigid_body.body_type = RigidBodyType::Static;
    assert!(AnimationPlayback::prepare(&scene)
        .err()
        .unwrap()
        .contains("physics body or collider"));
}

#[test]
fn animation_uses_the_runtime_clock_without_physics_and_pauses() {
    let (scene, _, head) = rig();
    let settings = ProjectSettings {
        enable_physics: false,
        script_execution_mode: ScriptExecutionMode::Disabled,
        ..ProjectSettings::default()
    };
    let mut world = crate::RuntimeWorld::prepare(
        Uuid::new_v4(),
        scene,
        std::path::Path::new("."),
        settings,
        RuntimePreferences::default(),
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    world.start();
    assert_eq!(world.scene.get(head).unwrap().position, Vec3::Y);
    world.control(crate::RuntimeControl::Pause);
    for _ in 0..30 {
        world.control(crate::RuntimeControl::Step);
    }
    assert_eq!(world.status().fixed_ticks, 30);
    assert_eq!(
        world.scene.get(head).unwrap().position,
        Vec3::new(1.0, 2.0, 0.0)
    );
}

#[test]
fn animation_and_debug_consume_impacts_from_the_fixed_physics_step() {
    let (mut scene, owner, head) = rig();
    let collider = Aabb {
        min: Vec3::splat(-0.5),
        max: Vec3::splat(0.5),
    };
    let puppet = scene.get_mut(owner).unwrap();
    puppet.collider.collider_type = ColliderType::Aabb;
    puppet.collider.aabb = collider;
    puppet.rigid_body.enabled = true;
    let projectile = scene.add_root("Projectile");
    let projectile = scene.get_mut(projectile).unwrap();
    projectile.position.x = 1.5;
    projectile.collider.collider_type = ColliderType::Aabb;
    projectile.collider.aabb = collider;
    projectile.rigid_body.enabled = true;
    projectile.rigid_body.body_type = RigidBodyType::Dynamic;
    projectile.rigid_body.use_gravity = false;
    projectile.rigid_body.velocity.x = -10.0;
    projectile.rigid_body.force.x = -60.0;
    let mut settings = ProjectSettings {
        enable_physics: true,
        script_execution_mode: ScriptExecutionMode::Disabled,
        ..ProjectSettings::default()
    };
    settings.runtime.advanced_physics = true;
    settings.runtime.physics_debug_draw = true;
    let mut world = crate::RuntimeWorld::prepare(
        Uuid::new_v4(),
        scene,
        std::path::Path::new("."),
        settings,
        RuntimePreferences::default(),
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    world.start();
    world.control(crate::RuntimeControl::Pause);
    world.control(crate::RuntimeControl::Step);
    assert_eq!(world.physics_debug_loads()[0].force.x, -60.0);
    assert_eq!(world.animation_bones().len(), 1);
    for _ in 0..8 {
        world.control(crate::RuntimeControl::Step);
        if !world.physics_impacts().is_empty() {
            assert!(world.scene.get(head).unwrap().position.x < -0.1);
            return;
        }
    }
    panic!("expected the projectile to hit the rig's physical body");
}

#[test]
fn animation_asset_links_resolve_the_current_file_for_each_play_snapshot() {
    let (mut scene, owner, head) = rig();
    let root = std::env::temp_dir().join(format!("raf-linked-animation-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let authored = scene.get_mut(owner).unwrap();
    let original = authored.animation.take().unwrap();
    let path = raf_assets::animation_asset::create(&root, "Puppet", &original).unwrap();
    authored.animation_asset = Some(path.clone());
    let persisted = ron::to_string(&scene).unwrap();
    let play = || {
        let scene = ron::from_str(&persisted).unwrap();
        let settings = ProjectSettings {
            enable_physics: false,
            script_execution_mode: ScriptExecutionMode::Disabled,
            ..ProjectSettings::default()
        };
        let mut world = crate::RuntimeWorld::prepare(
            Uuid::new_v4(),
            scene,
            &root,
            settings,
            RuntimePreferences::default(),
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();
        world.start();
        world.scene.get(head).unwrap().position
    };
    assert_eq!(play(), Vec3::Y);
    let mut edited = original.clone();
    edited.layers[0].tracks[0].keys[0].value.y = 3.0;
    raf_assets::animation_asset::save(&root, &path, &original, &edited).unwrap();
    assert!(raf_assets::animation_asset::save(&root, &path, &original, &original).is_err());
    assert_eq!(play(), 3.0 * Vec3::Y);
    assert!(scene.get(owner).unwrap().animation.is_none());
    std::fs::remove_file(root.join("assets").join(&path)).unwrap();
    std::fs::remove_file(root.join("assets").join(format!(".{path}.previous"))).unwrap();
    std::fs::remove_dir(root.join("assets")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
