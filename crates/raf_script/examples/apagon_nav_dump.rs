use std::path::PathBuf;

use raf_core::scene::Primitive;
use raf_core::SceneGraph;
use raf_script::{InputSnapshot, RhaiScriptRuntime, ScriptRuntimeOptions};

fn main() {
    let project = PathBuf::from(r"C:\Users\erick\OneDDrive\Documentos\AuraRafi Projects\XDXXXSADA");
    let scene_path = project
        .join("sessions")
        .join("3fc5cfca-62ae-4064-86f8-12f91f2a39dd")
        .join("scene.ron");
    let assets = project.join("assets");
    let source = std::fs::read_to_string(&scene_path).expect("read scene");
    let mut scene: SceneGraph = ron::from_str(&source).expect("parse scene");

    for id in scene.all_valid_ids() {
        if let Some(node) = scene.get_mut(id) {
            node.scripts.clear();
        }
    }
    let world = scene.find_node_by_path("/World").expect("world");
    scene
        .get_mut(world)
        .unwrap()
        .scripts
        .push("scripts/apagon/world_bootstrap.rhai".into());
    let dump = scene.add_root_with_primitive("__nav_dump__", Primitive::Empty);
    scene
        .get_mut(dump)
        .unwrap()
        .scripts
        .push("scripts/apagon/nav_dump.rhai".into());

    let options = ScriptRuntimeOptions {
        max_time_ms: 60_000,
        max_operations: 100_000_000,
        ..Default::default()
    };
    let (mut runtime, report) = RhaiScriptRuntime::load_from_scene(&scene, Some(&assets), options);
    for line in report.logs { println!("[load] {line}"); }
    for error in report.errors { println!("[load] ERROR: {error}"); }

    let report = runtime.call_start(&mut scene);
    for line in &report.logs { println!("[start] {line}"); }
    for error in &report.errors { println!("[start] ERROR: {error}"); }

    let input = InputSnapshot::default();
    for frame in 0..300 {
        let report = runtime.update(&mut scene, 1.0 / 60.0, &input);
        for line in report.logs { println!("[u{frame}] {line}"); }
        for error in report.errors {
            println!("[u{frame}] ERROR: {error}");
            return;
        }
    }
}
