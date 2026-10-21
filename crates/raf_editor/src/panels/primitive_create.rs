//! Shared primitive choices used by the editor creation affordances.

use raf_core::scene::Primitive;

pub use raf_core::scene::geometry::CREATABLE_PRIMITIVES as CREATEABLE_PRIMITIVES;

pub fn primitive_key(primitive: Primitive) -> &'static str {
    primitive.descriptor().label_key
}
pub fn primitive_slug(primitive: Primitive) -> &'static str {
    primitive.descriptor().slug
}
pub fn parse_primitive_slug(slug: &str) -> Option<Primitive> {
    Primitive::from_slug(slug).filter(|primitive| *primitive != Primitive::Empty)
}

pub fn primitive_name(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Cube => "Cube",
        Primitive::Sphere => "Sphere",
        Primitive::Cylinder => "Cylinder",
        Primitive::Capsule => "Capsule",
        Primitive::Plane => "Plane",
        Primitive::Empty => "Entity",
    }
}
