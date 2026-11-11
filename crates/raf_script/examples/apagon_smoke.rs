use std::path::PathBuf;
use std::time::Instant;

use raf_core::SceneGraph;
use raf_script::{InputSnapshot, RhaiScriptRuntime, ScriptRuntimeOptions, ScriptRuntimeReport};

fn dump(label: &str, report: &ScriptRuntimeReport) -> bool {
    for line in &report.logs {
        println!("[{label}] {line}");
    }
    for error in &report.errors {
        println!("[{label}] ERROR: {error}");
    }
    !report.errors.is_empty()
}

fn main() {
    let project = PathBuf::from(r"C:\Users\erick\OneDDrive\Documentos\AuraRafi Projects\XDXXXSADA");
    let scene_path = project
        .join("sessions")
        .join("3fc5cfca-62ae-4064-86f8-12f91f2a39dd")
        .join("scene.ron");
    let assets = project.join("assets");
    let source = std::fs::read_to_string(&scene_path).expect("read scene");
    let mut scene: SceneGraph = ron::from_str(&source).expect("parse scene");
    println!("scene entities: {}", scene.len());

    let options = ScriptRuntimeOptions {
        max_time_ms: 50,
        max_operations: 100_000,
        ..Default::default()
    };
    let (mut runtime, report) = RhaiScriptRuntime::load_from_scene(&scene, Some(&assets), options);
    if dump("load", &report) {
        return;
    }

    let report = runtime.call_start(&mut scene);
    let mut failed = dump("start", &report);

    let input = InputSnapshot::default();
    for frame in 0..240 {
        if failed { break; }
        let started = Instant::now();
        let report = runtime.update(&mut scene, 1.0 / 60.0, &input);
        let update_ms = started.elapsed().as_millis();
        failed |= dump(&format!("update{frame}"), &report);
        if failed { break; }
        let started = Instant::now();
        let report = runtime.fixed_update(&mut scene, 1.0 / 60.0, &input);
        let fixed_ms = started.elapsed().as_millis();
        if update_ms > 20 || fixed_ms > 20 {
            println!("[timing] frame {frame}: update {update_ms} ms, fixed {fixed_ms} ms");
        }
        failed |= dump(&format!("fixed{frame}"), &report);
    }

    let report = runtime.call_destroy(&mut scene);
    let _ = dump("destroy", &report);
    if !failed {
        println!("SMOKE OK");
    }
}
