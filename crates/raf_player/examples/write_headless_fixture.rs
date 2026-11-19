//! Writes a minimal runtime manifest for the headless smoke test.
//!
//! The fixture is intentionally tiny and deterministic: one cube, one authored
//! camera and no scripts or textures. It exercises the headless host, the
//! runtime fixed step, the scene renderer and the PNG capture path without
//! depending on a user project.
//!
//! Usage:
//!   cargo run -p raf_player --example write_headless_fixture
//!   cargo run -p raf_player --example write_headless_fixture -- <output.ron>

use raf_core::config::EngineSettings;
use raf_core::project::ProjectSettings;
use raf_core::runtime_config::GameCamera;
use raf_core::scene::{Primitive, SceneGraph};
use raf_runtime::manifest::{RuntimeHostSettings, RuntimeManifest};
use std::path::PathBuf;
use uuid::Uuid;

fn main() -> Result<(), String> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/headless/project/runtime.ron"));

    let mut scene = SceneGraph::new();
    let world = scene.add_root("World");
    let cube = scene.add_child_with_primitive(world, "Cube", Primitive::Cube);
    scene.get_mut(cube).unwrap().position = glam::Vec3::new(0.0, 1.0, 0.0);
    let camera = scene.add_child(world, "Camera");
    {
        let node = scene.get_mut(camera).unwrap();
        node.position = glam::Vec3::new(0.0, 2.5, 7.5);
        node.game_camera = Some(GameCamera::default());
    }
    let camera_uuid = scene.get(camera).unwrap().uuid;

    let mut project_settings = ProjectSettings::default();
    project_settings.runtime.active_camera = Some(camera_uuid);

    let manifest = RuntimeManifest {
        project_id: Uuid::new_v4(),
        session_id: None,
        development_slot: 0,
        save_root_grant: None,
        node_graph: None,
        protocol: 1,
        instance: Uuid::new_v4(),
        // CI and local smoke runs execute from the repository root.
        project_root: PathBuf::from("."),
        project_name: "Headless Fixture".into(),
        ui_document_file: None,
        scene,
        project_settings,
        host_settings: RuntimeHostSettings::from_engine(&EngineSettings::default()),
    };

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("fixture directory {}: {error}", parent.display()))?;
    }
    manifest.write(&output)?;
    println!("{}", output.display());
    Ok(())
}
