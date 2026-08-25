//! Shared primitive choices used by the editor creation affordances.

use raf_core::scene::Primitive;

pub const CREATEABLE_PRIMITIVES: [Primitive; 4] = [
    Primitive::Cube,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Plane,
];

pub fn primitive_key(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Cube => "app.primitive_cube",
        Primitive::Sphere => "app.primitive_sphere",
        Primitive::Cylinder => "app.primitive_cylinder",
        Primitive::Plane => "app.primitive_plane",
        Primitive::Empty => "app.primitive_empty",
    }
}

pub fn primitive_slug(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Cube => "cube",
        Primitive::Sphere => "sphere",
        Primitive::Cylinder => "cylinder",
        Primitive::Plane => "plane",
        Primitive::Empty => "empty",
    }
}

pub fn parse_primitive_slug(slug: &str) -> Option<Primitive> {
    match slug {
        "cube" => Some(Primitive::Cube),
        "sphere" => Some(Primitive::Sphere),
        "cylinder" => Some(Primitive::Cylinder),
        "plane" => Some(Primitive::Plane),
        _ => None,
    }
}

pub fn primitive_name(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Cube => "Cube",
        Primitive::Sphere => "Sphere",
        Primitive::Cylinder => "Cylinder",
        Primitive::Plane => "Plane",
        Primitive::Empty => "Entity",
    }
}
