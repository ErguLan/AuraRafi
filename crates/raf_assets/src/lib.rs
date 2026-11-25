//! # raf_assets
//!
//! Asset management for AuraRafi: importing, hot-reloading, and browsing.

pub mod animation_import;
pub mod skeletal_model;
pub mod animation_asset;
pub mod asset_paths;
pub mod audio_asset;
pub mod browser;
pub mod authoring_catalog;
pub mod image_asset;
pub mod importer;
pub mod ingress;
pub mod model_asset;
pub mod primitive_manifest;
pub mod primitives;

pub use browser::AssetBrowser;
pub use image_asset::{decode_image, image_output_dimensions, DecodedImage};
pub use importer::{
    import_model_package, import_model_package_with_destination, AssetImporter, AssetType,
    ImportReport, ModelImportMetadata,
};
pub use model_asset::{
    load_static_model, ImportedMesh, ImportedModel, ModelBounds, ModelDependency,
    ModelDependencyKind, ModelMaterial, MAX_MODEL_FILE_BYTES, MAX_MODEL_MATERIALS,
    MAX_MODEL_NODE_DEPTH, MAX_MODEL_TEXTURES, MAX_MODEL_TRIANGLES,
};
pub use ingress::{
    decode_texture, thumbnail, AssetIngressError, DecodedTexture, TextureDecodeLimits,
    MAX_THUMBNAIL_DIMENSION,
};
pub use primitive_manifest::{
    builtin_primitive_model_kinds, PrimitiveManifestError, PrimitiveModelManifest,
    PrimitiveModelPart,
};
pub use primitives::{Primitive3D, PrimitiveShape};
