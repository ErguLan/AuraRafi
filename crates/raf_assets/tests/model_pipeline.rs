//! Integration coverage for the static model pipeline.

use raf_assets::asset_paths::{
    copy_imported_file, normalize_reference, resolve_under_project, ImportBudget,
};
use raf_assets::authoring_catalog::AuthoringAsset;
use raf_assets::importer::{import_model_package, ModelImportMetadata};
use raf_assets::model_asset::load_static_model;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "raf-assets-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn triangle_bin() -> Vec<u8> {
    let mut bin = Vec::with_capacity(44);
    for value in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0u16, 1, 2, 0] {
        bin.extend_from_slice(&index.to_le_bytes());
    }
    assert_eq!(bin.len(), 44);
    bin
}

fn triangle_document(buffer: serde_json::Value, textured: bool) -> serde_json::Value {
    let mut document = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "buffers": [buffer],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962},
            {"buffer": 0, "byteOffset": 36, "byteLength": 8, "target": 34963}
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0]
            },
            {
                "bufferView": 1,
                "componentType": 5123,
                "count": 3,
                "type": "SCALAR",
                "min": [0],
                "max": [2]
            }
        ],
        "meshes": [{"name": "Triangle", "primitives": [{
            "attributes": {"POSITION": 0},
            "indices": 1
        }]}]
    });
    if textured {
        let object = document.as_object_mut().unwrap();
        object.insert(
            "materials".into(),
            json!([{
                "name": "Brick",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.5, 0.25, 0.125, 1.0],
                    "baseColorTexture": {"index": 0}
                }
            }]),
        );
        object.insert("textures".into(), json!([{"source": 0}]));
        object.insert(
            "images".into(),
            json!([{"uri": "textures/base.png"}]),
        );
        object["meshes"][0]["primitives"][0]["material"] = json!(0);
    }
    document
}

fn glb_bytes(document: &serde_json::Value, bin: &[u8]) -> Vec<u8> {
    let mut json_chunk = serde_json::to_vec(document).expect("serialize gltf json");
    while json_chunk.len() % 4 != 0 {
        json_chunk.push(b' ');
    }
    let mut bin_chunk = bin.to_vec();
    while bin_chunk.len() % 4 != 0 {
        bin_chunk.push(0);
    }
    let total = 12 + 8 + json_chunk.len() + 8 + bin_chunk.len();
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(b"glTF");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(total as u32).to_le_bytes());
    bytes.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    bytes.extend_from_slice(&json_chunk);
    bytes.extend_from_slice(&(bin_chunk.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    bytes.extend_from_slice(&bin_chunk);
    bytes
}

fn write_triangle_glb(path: &Path) -> Vec<u8> {
    let bin = triangle_bin();
    let document = triangle_document(json!({"byteLength": 44}), false);
    let bytes = glb_bytes(&document, &bin);
    fs::write(path, &bytes).expect("write glb");
    bytes
}

#[test]
fn references_are_normalized_and_traversal_is_rejected() {
    assert_eq!(
        normalize_reference("assets/models/store.glb").unwrap(),
        "models/store.glb"
    );
    assert_eq!(
        normalize_reference("./models/store.glb").unwrap(),
        "models/store.glb"
    );
    for invalid in [
        "",
        "../escape.glb",
        "models/../../escape.glb",
        "/absolute/escape.glb",
        "C:/escape.glb",
        "models//empty.glb",
        "https://example.com/model.glb",
    ] {
        assert!(
            normalize_reference(invalid).is_err(),
            "{invalid} must be rejected"
        );
    }

    let temp = TempDir::new("paths");
    fs::create_dir_all(temp.path().join("assets/models")).unwrap();
    let resolved = resolve_under_project(temp.path(), "models/store.glb").unwrap();
    assert!(resolved.starts_with(temp.path().join("assets").canonicalize().unwrap()));
    assert_eq!(
        resolve_under_project(temp.path(), "assets/models/store.glb").unwrap(),
        resolved
    );
    assert!(resolve_under_project(temp.path(), "../escape.glb").is_err());
}

#[test]
fn copy_budget_adds_unique_names_and_enforces_limits() {
    let temp = TempDir::new("copy");
    let source = temp.path().join("mesh.glb");
    fs::write(&source, b"mesh").unwrap();
    let destination = temp.path().join("out");
    let mut budget = ImportBudget::default();
    let first = copy_imported_file(&source, &destination, &mut budget).unwrap();
    let second = copy_imported_file(&source, &destination, &mut budget).unwrap();
    assert_eq!(first.path.file_name().unwrap(), "mesh.glb");
    assert_eq!(second.path.file_name().unwrap(), "mesh_1.glb");
    assert_eq!(budget.asset_count(), 2);
    assert_eq!(budget.used_bytes(), 8);

    let huge = temp.path().join("huge.glb");
    let file = fs::File::create(&huge).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    let mut budget = ImportBudget::default();
    assert!(copy_imported_file(&huge, &destination, &mut budget).is_err());
}

#[test]
fn glb_loads_positions_indices_and_generated_normals() {
    let temp = TempDir::new("glb");
    let path = temp.path().join("triangle.glb");
    write_triangle_glb(&path);

    let model = load_static_model(&path).unwrap();
    assert_eq!(model.triangles, 1);
    assert_eq!(model.meshes.len(), 1);
    assert_eq!(model.vertex_count(), 3);
    assert_eq!(model.meshes[0].indices, vec![0, 1, 2]);
    assert_eq!(model.bounds.min, [0.0, 0.0, 0.0]);
    assert_eq!(model.bounds.max, [1.0, 1.0, 0.0]);
    for normal in &model.meshes[0].normals {
        assert!((normal[0]).abs() < 1.0e-5);
        assert!((normal[1]).abs() < 1.0e-5);
        assert!((normal[2] - 1.0).abs() < 1.0e-5);
    }
    assert!(model.dependencies.is_empty());
}

#[test]
fn gltf_external_buffers_and_textures_are_tracked_and_confined() {
    let temp = TempDir::new("gltf");
    fs::create_dir_all(temp.path().join("textures")).unwrap();
    fs::write(temp.path().join("model.bin"), triangle_bin()).unwrap();
    fs::write(temp.path().join("textures/base.png"), b"not-a-real-png").unwrap();
    let document = triangle_document(
        json!({"uri": "model.bin", "byteLength": 44}),
        true,
    );
    let path = temp.path().join("triangle.gltf");
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();

    let model = load_static_model(&path).unwrap();
    assert_eq!(model.triangles, 1);
    assert_eq!(model.materials.len(), 1);
    assert_eq!(model.materials[0].name, "Brick");
    assert_eq!(model.materials[0].base_color, [0.5, 0.25, 0.125, 1.0]);
    assert_eq!(
        model.materials[0].base_color_texture.as_deref(),
        Some("textures/base.png")
    );
    let paths: Vec<&str> = model
        .dependencies
        .iter()
        .map(|dependency| dependency.path.as_str())
        .collect();
    assert!(paths.contains(&"model.bin"));
    assert!(paths.contains(&"textures/base.png"));

    let mut escaped = triangle_document(
        json!({"uri": "../outside.bin", "byteLength": 44}),
        false,
    );
    escaped["scene"] = json!(0);
    let escaped_path = temp.path().join("escaped.gltf");
    fs::write(&escaped_path, serde_json::to_vec(&escaped).unwrap()).unwrap();
    assert!(load_static_model(&escaped_path).is_err());
}

#[test]
fn package_import_copies_dependencies_and_never_overwrites() {
    let temp = TempDir::new("import");
    let assets = temp.path().join("project/assets");
    fs::create_dir_all(assets.join("textures")).unwrap();
    fs::write(assets.join("model.bin"), triangle_bin()).unwrap();
    fs::write(assets.join("textures/base.png"), b"not-a-real-png").unwrap();
    let document = triangle_document(
        json!({"uri": "model.bin", "byteLength": 44}),
        true,
    );
    let source = assets.join("source.gltf");
    fs::write(&source, serde_json::to_vec(&document).unwrap()).unwrap();

    let report = import_model_package(&source, &assets).unwrap();
    assert!(report.mesh_asset.path.starts_with("models/source/"));
    assert_eq!(report.metadata.triangles, 1);
    assert_eq!(report.metadata.materials.len(), 1);
    assert_eq!(
        report.metadata.materials[0].base_color_texture.as_deref(),
        Some(format!("{}/textures/base.png", report.package).as_str())
    );
    let model_file = assets.join(&report.mesh_asset.path);
    assert!(model_file.is_file());
    assert!(assets
        .join(&report.package)
        .join("model.bin")
        .is_file());
    assert!(assets
        .join(&report.package)
        .join("textures/base.png")
        .is_file());
    let sidecar = ModelImportMetadata::sidecar_path(&model_file);
    assert!(sidecar.is_file());
    let cached = ModelImportMetadata::read_for_asset_file(&model_file).unwrap();
    assert_eq!(cached.path, report.mesh_asset.path);
    assert_eq!(cached.content_hash, report.metadata.content_hash);

    let second = import_model_package(&source, &assets).unwrap();
    assert_ne!(second.mesh_asset.path, report.mesh_asset.path);
    assert!(assets.join(&second.mesh_asset.path).is_file());
    assert!(assets.join(&report.mesh_asset.path).is_file());
}

#[test]
fn catalog_marks_only_existing_glb_gltf_models_insertable() {
    let temp = TempDir::new("catalog");
    let assets = temp.path().join("project/assets");
    fs::create_dir_all(&assets).unwrap();
    let source = temp.path().join("store.glb");
    write_triangle_glb(&source);
    let report = import_model_package(&source, &assets).unwrap();

    let entry = AuthoringAsset::from_path_with_root(&report.mesh_asset.path, Some(&assets));
    assert!(entry.exists());
    assert!(entry.insertable());
    assert_eq!(entry.dimensions, Some([1.0, 1.0, 0.0]));
    assert_eq!(entry.triangles(), Some(1));
    let metadata = entry.model_metadata().expect("cached metadata");
    assert_eq!(metadata.triangles, 1);

    let missing = AuthoringAsset::from_path_with_root("models/missing.glb", Some(&assets));
    assert!(!missing.exists());
    assert!(!missing.insertable());

    let legacy = assets.join("models/legacy.obj");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, b"obj").unwrap();
    let legacy = AuthoringAsset::from_path_with_root("models/legacy.obj", Some(&assets));
    assert!(legacy.exists());
    assert!(!legacy.insertable());
}
