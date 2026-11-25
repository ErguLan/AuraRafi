//! Scene node identity, primitives, colors, and node-local data.

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::scene::{AudioSource, Collider, RigidBody, SceneVariable, VariableValue};

/// Unique identifier for a scene node (index into the graph's node vec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneNodeId(pub usize);

/// Primitive shape for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primitive {
    /// No visible geometry (group/empty node).
    Empty,
    /// Unit cube centered at origin.
    Cube,
    /// Unit sphere.
    Sphere,
    /// Flat plane on XZ.
    #[serde(alias = "Sprite2D", alias = "sprite2d")]
    Plane,
    /// Cylinder along Y axis.
    Cylinder,
    /// Rounded Y-axis solid, diameter 1 m and total height 2 m.
    Capsule,
    /// Inclined wedge/ramp, 1x1x1 bounds.
    Wedge,
    /// Torus on the XZ plane with unit outer diameter.
    Torus,
    /// Regular prism along Y with a configurable side count.
    Prism,
    /// Camera-facing shaded disk that reads as a sphere while remaining flat.
    Sphere2D,
}

impl Default for Primitive {
    fn default() -> Self {
        Self::Empty
    }
}

impl Primitive {
    /// Display name for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::Cube => "Cube",
            Self::Sphere => "Sphere",
            Self::Plane => "Plane",
            Self::Cylinder => "Cylinder",
            Self::Capsule => "Capsule",
            Self::Wedge => "Wedge",
            Self::Torus => "Torus",
            Self::Prism => "Prism",
            Self::Sphere2D => "Sphere 2D",
        }
    }

    /// Display name in Spanish.
    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Empty => "Vac\u{00ed}o",
            Self::Cube => "Cubo",
            Self::Sphere => "Esfera",
            Self::Plane => "Plano",
            Self::Cylinder => "Cilindro",
            Self::Capsule => "Cápsula",
            Self::Wedge => "Cuña",
            Self::Torus => "Toro",
            Self::Prism => "Prisma",
            Self::Sphere2D => "Esfera 2D",
        }
    }
}

/// RGBA color for an entity (0-255 per channel).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl NodeColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Default entity colors by primitive type.
    pub fn for_primitive(prim: Primitive) -> Self {
        match prim {
            Primitive::Empty => Self::rgb(120, 120, 120),
            Primitive::Cube => Self::rgb(224, 116, 24),
            Primitive::Sphere => Self::rgb(236, 236, 236),
            Primitive::Plane => Self::rgb(150, 150, 150),
            Primitive::Cylinder => Self::rgb(142, 148, 156),
            Primitive::Capsule => Self::rgb(170, 180, 192),
            Primitive::Wedge => Self::rgb(198, 160, 112),
            Primitive::Torus => Self::rgb(150, 166, 182),
            Primitive::Prism => Self::rgb(176, 186, 198),
            Primitive::Sphere2D => Self::rgb(236, 236, 236),
        }
    }
}

impl Default for NodeColor {
    fn default() -> Self {
        Self::rgb(180, 180, 180)
    }
}

/// A single node in the scene hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneNode {
    /// Stable UUID for serialization.
    pub uuid: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Local position relative to parent.
    pub position: Vec3,
    /// Local rotation (Euler degrees for editor display).
    pub rotation: Vec3,
    /// Local scale.
    pub scale: Vec3,
    /// Visual primitive shape.
    pub primitive: Primitive,
    /// Display color.
    pub color: NodeColor,
    /// Project-relative base-color image under the project's `assets` folder.
    #[serde(default)]
    pub base_color_texture: Option<String>,
    /// Parent index, `None` for root nodes.
    pub parent: Option<SceneNodeId>,
    /// Child indices.
    pub children: Vec<SceneNodeId>,
    /// Whether this node is visible.
    pub visible: bool,
    #[serde(default = "default_shadow_participation")]
    pub casts_shadows: bool,
    #[serde(default = "default_shadow_participation")]
    pub receives_shadows: bool,
    /// Prevents viewport transforms while keeping the node selectable.
    #[serde(default)]
    pub locked: bool,
    /// Associated ECS entity handle (optional, for linking with hecs).
    pub entity_index: Option<u32>,
    /// External script files attached to this entity (e.g. VS Code edited logic).
    pub scripts: Vec<String>,
    /// Script-facing custom variables for this entity.
    #[serde(default)]
    pub variables: Vec<SceneVariable>,
    /// Optional parameters scoped to an attached source, separate from shared entity state.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub behavior_parameters: std::collections::BTreeMap<String, Vec<SceneVariable>>,
    /// Runtime audio source settings.
    #[serde(default)]
    pub audio_source: AudioSource,
    /// Collider configuration used by the runtime.
    #[serde(default)]
    pub collider: Collider,
    /// Physics body state for runtime simulation.
    #[serde(default)]
    pub rigid_body: RigidBody,
    /// Optional explicit capsule character controller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_controller: Option<crate::scene::character::CharacterController>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub physics_joints: Vec<crate::scene::joint::PhysicsJoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<crate::scene::animation::AnimationRig>,
    /// Project-relative file under assets, resolved when preparing playback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation_asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<crate::scene::mesh_asset::SkinBinding>,
    /// Explicit game camera component. Ordinary objects never become cameras implicitly.
    #[serde(default)]
    pub game_camera: Option<crate::runtime_config::GameCamera>,
    /// Optional local point/spot light authored on this entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<crate::scene::SceneLight>,
    /// Optional data-driven particle system instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_system: Option<crate::scene::ParticleSystemComponent>,
    /// Optional static imported mesh geometry source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_asset: Option<crate::scene::MeshAssetRef>,
    /// Optional persistent editable topology authored through Tab Mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editable_mesh: Option<crate::scene::EditableMeshData>,
    #[serde(default)]
    pub surface: crate::scene::SurfaceStyle,
    #[serde(default)]
    pub pivot: crate::scene::PivotMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_marker: Option<crate::scene::SpawnMarker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_assembly: Option<crate::scene::LinkedAssembly>,
    /// Organizational folder/group node inside the hierarchy.
    #[serde(default)]
    pub is_folder: bool,
    /// Optional asset/manifest identifier used to create this node.
    #[serde(default)]
    pub source_asset: Option<String>,
    /// Schema version of the source manifest, when this node came from one.
    #[serde(default)]
    pub source_schema_version: Option<u32>,
    /// Optional semantic role assigned by an authoring tool (for example
    /// `store.shelf`). It is metadata only and never changes rendering.
    #[serde(default)]
    pub semantic_role: Option<String>,
    /// Stable authoring key used by reconciliation workflows.
    #[serde(default)]
    pub stable_key: Option<String>,
    /// Searchable semantic tags for Agent perception and batch operations.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Task or tool that created the node, when known.
    #[serde(default)]
    pub agent_origin: Option<String>,
}

impl SceneNode {
    /// Create a new node with default transform.
    pub fn new(name: &str) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            name: name.to_string(),
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            primitive: Primitive::Empty,
            color: NodeColor::default(),
            base_color_texture: None,
            parent: None,
            children: Vec::new(),
            visible: true,
            casts_shadows: true,
            receives_shadows: true,
            locked: false,
            entity_index: None,
            scripts: Vec::new(),
            variables: Vec::new(),
            behavior_parameters: Default::default(),
            audio_source: AudioSource::default(),
            collider: Collider::default(),
            rigid_body: RigidBody::default(),
            character_controller: None,
            physics_joints: Vec::new(),
            animation: None,
            animation_asset: None,
            skin: None,
            game_camera: None,
            light: None,
            particle_system: None,
            mesh_asset: None,
            editable_mesh: None,
            surface: Default::default(),
            pivot: Default::default(),
            spawn_marker: None,
            linked_assembly: None,
            is_folder: false,
            source_asset: None,
            source_schema_version: None,
            semantic_role: None,
            stable_key: None,
            tags: Vec::new(),
            agent_origin: None,
        }
    }

    /// Create a node with a specific primitive.
    pub fn with_primitive(name: &str, primitive: Primitive) -> Self {
        let mut node = Self {
            uuid: Uuid::new_v4(),
            name: name.to_string(),
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            primitive,
            color: NodeColor::for_primitive(primitive),
            base_color_texture: None,
            parent: None,
            children: Vec::new(),
            visible: true,
            casts_shadows: true,
            receives_shadows: true,
            locked: false,
            entity_index: None,
            scripts: Vec::new(),
            variables: Vec::new(),
            behavior_parameters: Default::default(),
            audio_source: AudioSource::default(),
            collider: Collider::default(),
            rigid_body: RigidBody::default(),
            character_controller: None,
            physics_joints: Vec::new(),
            animation: None,
            animation_asset: None,
            skin: None,
            game_camera: None,
            light: None,
            particle_system: None,
            mesh_asset: None,
            editable_mesh: None,
            surface: Default::default(),
            pivot: Default::default(),
            spawn_marker: None,
            linked_assembly: None,
            is_folder: false,
            source_asset: None,
            source_schema_version: None,
            semantic_role: None,
            stable_key: None,
            tags: Vec::new(),
            agent_origin: None,
        };
        if primitive == Primitive::Sphere2D {
            // The billboard disk uses spherical normals on flat depth. It must
            // not sample the directional shadow map against itself.
            node.casts_shadows = false;
            node.receives_shadows = false;
        }
        node
    }

    /// Create a folder/group node. Uses Empty primitive but is semantically distinct.
    pub fn folder(name: &str) -> Self {
        let mut node = Self::new(name);
        node.is_folder = true;
        node.locked = true;
        node
    }

    /// Compute the local-to-parent transformation matrix.
    pub fn local_matrix(&self) -> Mat4 {
        let rotation_quat = Quat::from_euler(
            glam::EulerRot::YXZ,
            self.rotation.y.to_radians(),
            self.rotation.x.to_radians(),
            self.rotation.z.to_radians(),
        );
        Mat4::from_scale_rotation_translation(self.scale, rotation_quat, self.position)
    }

    pub fn get_variable(&self, name: &str) -> Option<&VariableValue> {
        self.variables
            .iter()
            .find(|variable| variable.name == name)
            .map(|variable| &variable.value)
    }

    pub fn set_variable(&mut self, name: &str, value: VariableValue) {
        if let Some(variable) = self
            .variables
            .iter_mut()
            .find(|variable| variable.name == name)
        {
            variable.value = value;
            return;
        }

        self.variables.push(SceneVariable {
            name: name.to_string(),
            value,
        });
    }
}

fn default_shadow_participation() -> bool {
    true
}
