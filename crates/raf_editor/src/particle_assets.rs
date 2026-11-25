//! Built-in particle templates and textures embedded in the editor binary.
//!
//! Creating a particle asset never depends on the editor's install directory:
//! the templates and the starter textures ship inside the executable and are
//! copied into the current project on demand.

use std::path::Path;

pub struct ParticleTemplate {
    pub slug: &'static str,
    pub name: &'static str,
    pub json: &'static str,
}

macro_rules! template {
    ($slug:literal, $name:literal, $path:literal) => {
        ParticleTemplate {
            slug: $slug,
            name: $name,
            json: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/particles/templates/",
                $path
            )),
        }
    };
}

const BLANK_TEMPLATE: &str = r#"{
  "version": 1,
  "name": "Particle System",
  "budget": { "max_particles": 2000, "max_sprites": 2000 },
  "emitters": [
    {
      "name": "emitter",
      "texture": "particles/textures/soft.png",
      "enabled": true,
      "max_particles": 1000,
      "rate": 40.0,
      "burst": 16,
      "lifetime": [0.8, 1.6],
      "speed": [0.2, 0.6],
      "spread": [30.0, 30.0],
      "direction": [0.0, 1.0, 0.0],
      "gravity": [0.0, 0.1, 0.0],
      "drag": 0.2,
      "size": {
        "keys": [
          { "t": 0.0, "value": 0.25 },
          { "t": 1.0, "value": 0.08 }
        ]
      },
      "opacity": {
        "keys": [
          { "t": 0.0, "value": 0.9 },
          { "t": 0.7, "value": 0.5 },
          { "t": 1.0, "value": 0.0 }
        ]
      },
      "color": {
        "keys": [
          { "t": 0.0, "color": [255, 255, 255, 255] },
          { "t": 1.0, "color": [180, 200, 255, 0] }
        ]
      },
      "shape": { "type": "point" },
      "flipbook": null,
      "collision": null,
      "wind_affects_drag": true
    }
  ]
}
"#;

pub const TEMPLATES: &[ParticleTemplate] = &[
    ParticleTemplate {
        slug: "blank",
        name: "Blank",
        json: BLANK_TEMPLATE,
    },
    template!("fire", "Fire", "fire.rafparticle"),
    template!("smoke", "Smoke", "smoke.rafparticle"),
    template!("sparks", "Sparks", "sparks.rafparticle"),
    template!("rain", "Rain", "rain.rafparticle"),
    template!("dust", "Dust", "dust.rafparticle"),
    template!("magic", "Magic", "magic.rafparticle"),
];

/// Starter textures copied next to the asset on creation. Existing files are
/// never overwritten, so authored atlases survive later asset creation.
const TEXTURES: &[(&str, &[u8])] = &[
    (
        "soft.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/soft.png"
        )),
    ),
    (
        "smoke.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/smoke.png"
        )),
    ),
    (
        "spark.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/spark.png"
        )),
    ),
    (
        "flame.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/flame.png"
        )),
    ),
    (
        "fire_atlas.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/fire_atlas.png"
        )),
    ),
    (
        "smoke_atlas.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/smoke_atlas.png"
        )),
    ),
    (
        "magic_atlas.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/particles/textures/magic_atlas.png"
        )),
    ),
];

pub fn template(slug: &str) -> Option<&'static ParticleTemplate> {
    TEMPLATES.iter().find(|template| template.slug == slug)
}

/// Writes `assets/particles/<stem>.rafparticle` from an embedded template plus
/// the starter textures. Returns the project-relative asset path.
pub fn create_particle_asset(
    project_root: &Path,
    template_slug: &str,
    stem: &str,
) -> Result<String, String> {
    let template = template(template_slug)
        .ok_or_else(|| format!("unknown particle template '{template_slug}'"))?;
    let mut asset = raf_particles::parse_asset(template.json)?;
    let stem = sanitize_stem(stem)?;
    asset.name = stem.clone();
    write_particle_asset(project_root, &stem, &asset)
}

/// Validates and writes a caller-authored particle definition. Existing assets
/// are never overwritten, and the starter textures are copied on first use.
pub fn write_particle_asset(
    project_root: &Path,
    stem: &str,
    asset: &raf_particles::ParticleSystemAsset,
) -> Result<String, String> {
    let stem = sanitize_stem(stem)?;
    if asset.emitters.len() > 64 {
        return Err("particle asset has more than 64 emitters".to_string());
    }
    let mut asset = asset.clone();
    asset.name = stem.clone();
    asset.validate();
    let assets = project_root.join("assets");
    let particles = assets.join("particles");
    let textures = particles.join("textures");
    std::fs::create_dir_all(&textures).map_err(|error| error.to_string())?;
    for (name, bytes) in TEXTURES {
        let target = textures.join(name);
        if !target.exists() {
            std::fs::write(&target, bytes).map_err(|error| error.to_string())?;
        }
    }

    let path = particles.join(format!("{stem}.rafparticle"));
    if path.exists() {
        return Err(format!("particle asset already exists: {}", path.display()));
    }
    let json = serde_json::to_string_pretty(&asset).map_err(|error| error.to_string())?;
    std::fs::write(&path, json).map_err(|error| error.to_string())?;
    Ok(format!("particles/{stem}.rafparticle"))
}

fn sanitize_stem(value: &str) -> Result<String, String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('.')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ' '))
        && !value.chars().all(char::is_whitespace);
    if !valid {
        return Err("invalid particle asset name".to_string());
    }
    Ok(value.replace(' ', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_template_is_a_valid_particle_asset() {
        for template in TEMPLATES {
            let asset = raf_particles::parse_asset(template.json)
                .unwrap_or_else(|error| panic!("template {}: {error}", template.slug));
            assert!(
                !asset.emitters.is_empty(),
                "template {} has no emitters",
                template.slug
            );
            assert_eq!(asset.version, raf_particles::PARTICLE_ASSET_VERSION);
        }
    }

    #[test]
    fn creation_writes_asset_and_textures_once() {
        let root =
            std::env::temp_dir().join(format!("aurafi-particle-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let row = create_particle_asset(&root, "fire", "Test Fire").unwrap();
        assert_eq!(row, "particles/Test_Fire.rafparticle");
        assert!(root.join("assets").join(&row).is_file());
        assert!(root
            .join("assets/particles/textures/fire_atlas.png")
            .is_file());
        assert!(create_particle_asset(&root, "fire", "Test Fire").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
