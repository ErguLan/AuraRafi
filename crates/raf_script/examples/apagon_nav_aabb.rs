use std::path::PathBuf;
use glam::Vec3;
use raf_core::scene::Primitive;
use raf_core::{SceneGraph, SceneNodeId};

#[derive(Clone, Copy)]
struct Aabb { min: Vec3, max: Vec3 }

fn world_pos(scene: &SceneGraph, id: SceneNodeId) -> Vec3 {
    let mut p = Vec3::ZERO;
    let mut cur = Some(id);
    while let Some(c) = cur {
        let n = scene.get(c).unwrap();
        p += n.position;
        cur = n.parent;
    }
    p
}

fn world_scale(scene: &SceneGraph, id: SceneNodeId) -> Vec3 {
    let mut s = Vec3::ONE;
    let mut cur = Some(id);
    while let Some(c) = cur {
        let n = scene.get(c).unwrap();
        s *= n.scale;
        cur = n.parent;
    }
    s
}

fn collect(scene: &SceneGraph, id: SceneNodeId, out: &mut Vec<Aabb>) {
    let Some(node) = scene.get(id) else { return };
    if node.children.is_empty() {
        let p = world_pos(scene, id);
        let s = world_scale(scene, id).abs();
        let half = s * 0.5;
        out.push(Aabb { min: p - half, max: p + half });
    } else {
        for child in node.children.clone() {
            collect(scene, child, out);
        }
    }
}

fn push_one(scene: &SceneGraph, path: &str, out: &mut Vec<Aabb>) {
    if let Some(id) = scene.find_node_by_path(path) {
        let p = world_pos(scene, id);
        let s = world_scale(scene, id).abs();
        let half = s * 0.5;
        out.push(Aabb { min: p - half, max: p + half });
    }
}

fn blocked(ax: f32, az: f32, bx: f32, bz: f32, y: f32, boxes: &[Aabb]) -> bool {
    let dx = bx - ax;
    let dz = bz - az;
    let d = (dx * dx + dz * dz).sqrt();
    if d < 0.05 { return false; }
    for b in boxes {
        if y < b.min.y || y > b.max.y { continue; }
        let mut tmin = 0.0f32;
        let mut tmax: f32 = 1.0;
        for (origin, dir, lo, hi) in [
            (ax, dx, b.min.x, b.max.x),
            (az, dz, b.min.z, b.max.z),
        ] {
            if dir.abs() < 1e-6 {
                if origin < lo || origin > hi { tmin = 1.0; tmax = 0.0; break; }
            } else {
                let mut t1 = (lo - origin) / dir;
                let mut t2 = (hi - origin) / dir;
                if t1 > t2 { std::mem::swap(&mut t1, &mut t2); }
                tmin = tmin.max(t1);
                tmax = tmax.min(t2);
                if tmin > tmax { break; }
            }
        }
        if tmin <= tmax { return true; }
    }
    false
}

fn main() {
    let project = PathBuf::from(r"C:\Users\erick\OneDDrive\Documentos\AuraRafi Projects\XDXXXSADA");
    let scene_path = project.join("sessions").join("3fc5cfca-62ae-4064-86f8-12f91f2a39dd").join("scene.ron");
    let source = std::fs::read_to_string(&scene_path).expect("read scene");
    let scene: SceneGraph = ron::from_str(&source).expect("parse scene");

    let mut boxes = Vec::new();
    if let Some(arch) = scene.find_node_by_path("/World/Arch") { collect(&scene, arch, &mut boxes); }
    if let Some(props) = scene.find_node_by_path("/World/Props") { collect(&scene, props, &mut boxes); }
    push_one(&scene, "/World/Logic/Gate/Gate_Slab", &mut boxes);
    push_one(&scene, "/World/Logic/Breakers/Breaker_Talleres/Console", &mut boxes);
    push_one(&scene, "/World/Logic/Breakers/Breaker_Archivo/Console", &mut boxes);
    push_one(&scene, "/World/Logic/Breakers/Breaker_Conductos/Console", &mut boxes);

    let Some(nav_root) = scene.find_node_by_path("/World/Nav") else { return };
    let mut nav = Vec::new();
    for child in scene.get(nav_root).unwrap().children.clone() {
        let p = world_pos(&scene, child);
        nav.push([p.x, p.z]);
    }
    let n = nav.len();
    println!("AABBS {} NAV {}", boxes.len(), n);
    for (k, b) in boxes.iter().enumerate().take(12) { println!("box {k}: {:?} .. {:?}", b.min, b.max); }
    for (k, p) in nav.iter().enumerate().take(6) { println!("nav {k}: {:?}", p); }
    for (k, b) in boxes.iter().enumerate() {
        if b.min.y <= 1.0 && b.max.y >= 1.0 && ((b.max.x - b.min.x) > 30.0 || (b.max.z - b.min.z) > 30.0) {
            println!("big candidate {k}: {:?} .. {:?}", b.min, b.max);
        }
    }
    let mut edges = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            let a = nav[i];
            let b = nav[j];
            let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if d >= 30.0 { continue; }
            if blocked(a[0], a[1], b[0], b[1], 1.0, &boxes) {
                if i == 0 && j == 2 {
                    for (k, bx) in boxes.iter().enumerate() {
                        if blocked(a[0], a[1], b[0], b[1], 1.0, &[*bx]) { println!("h1 blocker {k}: {:?} .. {:?}", bx.min, bx.max); }
                        if blocked(a[0], a[1], b[0], b[1], 2.8, &[*bx]) { println!("h28 blocker {k}: {:?} .. {:?}", bx.min, bx.max); }
                    }
                }
                continue;
            }
            if blocked(a[0], a[1], b[0], b[1], 2.8, &boxes) { continue; }
            edges.push((i, j));
        }
    }
    println!("EDGES_BEGIN {}", edges.len());
    for chunk in edges.chunks(12) {
        let mut line = String::new();
        for (i, j) in chunk {
            line.push_str(&format!("[{i},{j}],"));
        }
        println!("{line}");
    }
    println!("EDGES_END {}", edges.len());
}
