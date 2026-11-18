//! Optional smoke test using the user's saved parkour scene and authored scripts.
use crate::{
    manifest::RuntimeHostSettings, session_loader::SessionSource, RuntimePhase, RuntimeWorld,
};
use raf_core::{config::EngineSettings, project::Project, session::ProjectSessionRegistry};
use raf_script::InputSnapshot;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use uuid::Uuid;

#[test]
#[ignore = "requires RAF_PARKOUR_PROJECT pointing at the saved parkour project"]
fn physics_saved_parkour_falls_off_the_spawn_deck() {
    let root = PathBuf::from(std::env::var_os("RAF_PARKOUR_PROJECT").expect("RAF_PARKOUR_PROJECT"));
    let project = Project::load(&root).unwrap();
    let registry = ProjectSessionRegistry::load_or_legacy(&root, project.project_type);
    let session = registry.active().unwrap().id.0;
    let instance = Uuid::new_v4();
    let source = SessionSource {
        root,
        project_id: project.id,
        project_name: project.name,
        project_settings: project.settings,
        host_settings: RuntimeHostSettings::from_engine(&EngineSettings::default()),
        save_root_grant: None,
        development_slot: 0,
    };
    for advanced in [true, false] {
        let mut manifest = source.load(session, instance).unwrap();
        manifest.project_settings.runtime.advanced_physics = advanced;
        let player = manifest.scene.find_node_by_name("Player").unwrap();
        assert!(manifest.scene.get(player).unwrap().rigid_body.is_character);
        if advanced {
            if let Some(path) = std::env::var_os("RAF_PARKOUR_MANIFEST") {
                manifest.write(&PathBuf::from(path)).unwrap();
            }
        }
        let dt = 1.0 / manifest.project_settings.runtime.fixed_hz as f64;
        let mut prefs = manifest.host_settings.runtime;
        let ui_path = manifest
            .ui_document_file
            .map(|path| manifest.project_root.join(path));
        prefs.hot_reload = false;
        prefs.script_budget_ms = 100;
        let mut world = RuntimeWorld::prepare(
            instance,
            manifest.scene,
            &manifest.project_root,
            manifest.project_settings,
            prefs,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();
        if let Some(graph) = manifest.node_graph {
            world.attach_graph(&graph).unwrap();
        }
        if let Some(path) = ui_path.filter(|path| path.exists()) {
            let mut document: raf_ui::UiDocument =
                ron::de::from_bytes(&std::fs::read(path).unwrap()).unwrap();
            document.migrate().unwrap();
            world.game_ui_mut().mount_document(&document).unwrap();
        }
        world.start();
        for _ in 0..60 {
            world.advance(dt, &InputSnapshot::default());
        }
        let spawn_height = world.scene.world_matrix(player).w_axis.y;
        let input = InputSnapshot {
            keys_held: vec!["d".into()],
            ..Default::default()
        };
        let mut min_height = spawn_height;
        for _ in 0..240 {
            world.advance(dt, &input);
            let current = world.scene.world_matrix(player).w_axis.y;
            min_height = min_height.min(current);
        }
        assert_eq!(
            world.status().phase,
            RuntimePhase::Running,
            "{:?}",
            world.status().last_error
        );
        assert!(
            (0.5..3.0).contains(&spawn_height),
            "invalid spawn: {spawn_height}"
        );
        assert!(
            min_height < spawn_height - 0.8,
            "mode={advanced} spawn={spawn_height} min={min_height}"
        );
    }
}
