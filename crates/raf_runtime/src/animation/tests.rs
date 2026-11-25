use super::*;

#[test]
fn animation_action_uses_an_available_variant_and_keeps_held_items() {
    let (mut scene, owner, head) = rig();
    let cup = scene.add_root("Cup");
    let cup = scene.get(cup).unwrap().uuid;
    let controller = scene.add_root("Controller");
    let controller = scene.get(controller).unwrap().uuid;
    let actor = scene.get_mut(owner).unwrap();
    let rig = actor.animation.as_mut().unwrap();
    rig.layers[1].start = 0.0;
    rig.layers[1].fade_in = 0.0;
    rig.layers[1].fade_out = 0.0;
    rig.layers[1].action = Some(serde_json::from_value(serde_json::json!({"request":"switch","free_slots":["left_hand","right_hand"],"priority":2,"interruptible":true})).unwrap());
    let mut elbow = rig.layers[1].clone();
    elbow.name = "Elbow".into();
    elbow.tracks[0].keys[0].value.x = 2.0;
    let action = elbow.action.as_mut().unwrap();
    action.priority = 1;
    action.free_slots.clear();
    rig.layers.push(elbow);
    actor.set_variable(
        "animation_action",
        raf_core::scene::VariableValue::Text("switch".into()),
    );
    actor.set_variable(
        "left_hand",
        raf_core::scene::VariableValue::Entity(Some(cup)),
    );
    actor.set_variable("right_hand", raf_core::scene::VariableValue::Entity(Some(controller)));
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    playback.tick(&mut scene, 0.0, 0.0, &[], None);
    assert_eq!(scene.get(head).unwrap().position.x, 2.0);
    assert_eq!(
        scene.get(owner).unwrap().get_variable("left_hand"),
        Some(&raf_core::scene::VariableValue::Entity(Some(cup)))
    );
    assert_eq!(scene.get(owner).unwrap().get_variable("right_hand"), Some(&raf_core::scene::VariableValue::Entity(Some(controller))));
}

#[test]
fn animation_contact_does_not_cross_a_wall_on_the_first_step() {
    let mut scene = SceneGraph::new();
    let actor = scene.add_root("Actor");
    let upper = scene.add_child(actor, "Upper");
    let mid = scene.add_child(upper, "Mid");
    let tip = scene.add_child(mid, "Tip");
    scene.get_mut(mid).unwrap().position = Vec3::new(0.3, 0.3, 0.0);
    scene.get_mut(tip).unwrap().position = Vec3::new(0.1, -0.2, 0.0);
    let trigger = scene.add_root("Switch");
    scene.get_mut(trigger).unwrap().position = Vec3::X * 0.6;
    let goal = scene.get(trigger).unwrap().uuid;
    let wall = scene.add_root("Wall");
    let wall = scene.get_mut(wall).unwrap();
    wall.collider.collider_type = ColliderType::Aabb;
    wall.collider.aabb = Aabb { min: Vec3::new(0.5,-1.0,-1.0), max: Vec3::new(0.53,1.0,1.0) };
    scene.get_mut(actor).unwrap().animation = Some(serde_json::from_value(serde_json::json!({"layers":[],"contacts":[{
        "chain":[["Upper"],["Upper","Mid"],["Upper","Mid","Tip"]],"goal":{"target":{"entity":goal,"offset":[0,0,0]}},
        "pole":[0,1,0],"radius":0.03
    }]})).unwrap());
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    let mut physics = crate::physics::PhysicsWorld::prepare(&scene).unwrap();
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    playback.solve_contacts(&mut scene, &mut physics, -Vec3::Y);
    let x = scene.world_matrix(tip).w_axis.x;
    assert!((0.46..0.471).contains(&x), "tip crossed obstacle or did not reach it: {x}");
}

#[test]
fn animation_motion_matching_selects_the_requested_planar_direction() {
    let (mut scene, owner, head) = rig();
    let actor = scene.get_mut(owner).unwrap();
    actor.rigid_body.enabled = true;
    actor.rigid_body.body_type = raf_core::scene::RigidBodyType::Kinematic;
    actor.rigid_body.velocity = Vec3::X * 2.0;
    let rig = actor.animation.as_mut().unwrap();
    rig.layers.truncate(1);
    rig.layers[0].looping = true;
    rig.layers[0].motion_speed = Some(2.0);
    let mut strafe = rig.layers[0].clone();
    strafe.name = "Strafe".into();
    strafe.motion_heading_degrees = -90.0;
    strafe.tracks[0].keys[0].value.x = 2.0;
    rig.layers.push(strafe);
    rig.motion_match = Some(
        serde_json::from_value(
            serde_json::json!({"pose_path":["Head"],"lookahead":0.5,"pose_weight":0.0}),
        )
        .unwrap(),
    );
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    playback.tick(&mut scene, 0.0, 0.0, &[], Some(-Vec3::Y));
    assert_eq!(scene.get(head).unwrap().position.x, 2.0);
}

#[test]
fn animation_motion_matching_keeps_phase_when_feature_values_tie() {
    let (mut scene, owner, head) = rig();
    let actor = scene.get_mut(owner).unwrap();
    let rig = actor.animation.as_mut().unwrap();
    rig.layers.truncate(1);
    let layer = &mut rig.layers[0];
    layer.looping = true;
    layer.motion_speed = Some(0.0);
    layer.tracks[0].keys.truncate(1);
    layer.tracks.push(serde_json::from_value(serde_json::json!({"path":["Head"],"channel":"rotation","keys":[
        {"time":0.0,"value":[0,0,0]}, {"time":1.0,"value":[0,0,90]}]})).unwrap());
    rig.motion_match = Some(serde_json::from_value(serde_json::json!({"pose_path":["Head"],"lookahead":0.5,"pose_weight":1.0})).unwrap());
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    playback.tick(&mut scene, 0.0, 0.0, &[], None);
    for tick in 1..=40 { playback.tick(&mut scene, f64::from(tick) / 60.0, 1.0 / 60.0, &[], None); }
    assert!((scene.get(head).unwrap().rotation.z - 60.0).abs() < 1e-3);
}

#[test]
fn animation_ground_contact_places_tip_against_solid_geometry() {
    let mut scene = SceneGraph::new();
    let owner = scene.add_root("Character");
    let leg = scene.add_child(owner, "Leg");
    let knee = scene.add_child(leg, "Knee");
    let foot = scene.add_child(knee, "Foot");
    scene.get_mut(leg).unwrap().position.y = 1.38;
    scene.get_mut(knee).unwrap().position = Vec3::new(0.0, -0.7, 0.3);
    scene.get_mut(foot).unwrap().position = Vec3::new(0.0, -0.5, -0.3);
    let floor = scene.add_root("Floor");
    let floor = scene.get_mut(floor).unwrap();
    floor.collider.collider_type = ColliderType::Aabb;
    floor.collider.aabb = raf_core::scene::Aabb {
        min: Vec3::new(-3.0, -1.0, -3.0),
        max: Vec3::new(3.0, 0.0, 3.0),
    };
    scene.get_mut(owner).unwrap().animation = Some(serde_json::from_value(serde_json::json!({"layers":[],"contacts":[{
        "chain":[["Leg"],["Leg","Knee"],["Leg","Knee","Foot"]],
        "goal":{"ground":{"above":0.1,"below":0.35,"minimum_alignment":0.5}},"pole":[0,0,1],"radius":0.07
    }]})).unwrap());
    let mut physics = crate::physics::PhysicsWorld::prepare(&scene).unwrap();
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    physics.step(&mut scene, 1.0 / 60.0).unwrap();
    playback.solve_contacts(&mut scene, &mut physics, -Vec3::Y);
    assert!((scene.world_matrix(foot).w_axis.y - 0.07).abs() < 0.01);
}

#[test]
fn animation_physics_follow_blends_joint_and_bounds_motor_torque() {
    let (mut scene, owner, head) = rig();
    let body_id = scene.add_root("ExternalBody");
    let body = scene.get_mut(body_id).unwrap();
    body.position.x = 2.0;
    body.rotation.z = 45.0;
    body.rigid_body.enabled = true;
    body.rigid_body.body_type = RigidBodyType::Dynamic;
    let uuid = body.uuid;
    let rig = scene.get_mut(owner).unwrap().animation.as_mut().unwrap();
    rig.physics_follow.push(
        serde_json::from_value(serde_json::json!({"path":["Head"],"body":uuid,
        "weight":0.5,"stiffness":100.0,"damping":0.5,"max_torque":3.0}))
        .unwrap(),
    );
    let mut playback = AnimationPlayback::prepare(&scene).unwrap();
    playback.tick(&mut scene, 0.0, 0.0, &[], Some(-Vec3::Y));
    playback.blend_physics(&mut scene);
    assert!(scene
        .get(head)
        .unwrap()
        .position
        .abs_diff_eq(Vec3::new(1.0, 0.5, 0.0), 1e-5));
    playback.drive_bodies(&mut scene);
    assert!((scene.get(body_id).unwrap().rigid_body.torque.z + 3.0).abs() < 1e-4);
}

#[test]
fn animation_motion_matching_character_scale_fixture() {
    let (template, owner, _) = rig();
    let mut library = template.get(owner).unwrap().animation.clone().unwrap();
    library.layers.truncate(1);
    library.layers[0].looping = true;
    library.layers[0].motion_speed = Some(0.0);
    let mut walk = library.layers[0].clone();
    walk.name = "Walk".into();
    walk.motion_speed = Some(3.0);
    library.layers.push(walk);
    library.motion_match = Some(
        serde_json::from_value(
            serde_json::json!({"pose_path":["Head"],"lookahead":0.5,"pose_weight":1.0}),
        )
        .unwrap(),
    );
    for count in [1, 20, 100] {
        let mut scene = SceneGraph::new();
        for index in 0..count {
            let owner = scene.add_root(&format!("Actor {index}"));
            scene.add_child(owner, "Head");
            scene.get_mut(owner).unwrap().animation = Some(library.clone());
        }
        let mut playback = AnimationPlayback::prepare(&scene).unwrap();
        let mut elapsed = Vec::new();
        for tick in 0..120 {
            let started = std::time::Instant::now();
            playback.tick(
                &mut scene,
                f64::from(tick) / 60.0,
                1.0 / 60.0,
                &[],
                Some(-Vec3::Y),
            );
            elapsed.push(started.elapsed().as_micros());
        }
        elapsed.sort_unstable();
        assert_eq!(playback.sample(0.0, 0.0).unwrap().count(), count);
        eprintln!(
            "motion matching actors={count} p50_us={} p95_us={}",
            elapsed[60], elapsed[113]
        );
    }
}
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
