struct Frame {
    view_projection: mat4x4<f32>,
    light: vec4<f32>,
    light_color: vec4<f32>,
    viewport: vec4<f32>,
    eye: vec4<f32>,
    environment: vec4<f32>,
    // [authored, improved, toon_steps, toon_softness].
    lighting: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var color_texture: texture_2d<f32>;
@group(0) @binding(2) var color_sampler: sampler;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) rgba: vec4<f32>,
    @location(3) model_x: vec4<f32>,
    @location(4) model_y: vec4<f32>,
    @location(5) model_z: vec4<f32>,
    @location(6) model_w: vec4<f32>,
    @location(7) tint: vec4<f32>,
    @location(8) normal: vec3<f32>,
    @location(9) raster_kind: u32,
};

struct Paint {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rgba: vec4<f32>,
    @location(2) ray: vec3<f32>,
    @location(3) @interpolate(flat) zenith: vec4<f32>,
    @location(4) @interpolate(flat) horizon: vec4<f32>,
    @location(5) @interpolate(flat) ground: vec4<f32>,
    @location(6) @interpolate(flat) atmosphere: vec4<f32>,
    @location(7) @interpolate(flat) stars: vec4<f32>,
};

fn normal_paint(position: vec4<f32>, uv: vec2<f32>, rgba: vec4<f32>) -> Paint {
    return Paint(position, uv, rgba, vec3<f32>(0.0), vec4<f32>(0.0),
        vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
}

fn ambient_light(normal_y: f32) -> vec3<f32> {
    // environment.rgb is the ambient tint and light.w the authored fraction.
    // The product guarantees tint * fraction equals the authored ambient.
    let ambient = frame.environment.rgb * frame.light.w;
    if frame.lighting.x < 0.5 || frame.lighting.y < 0.5 { return ambient; }
    let low = vec3<f32>(0.45, 0.50, 0.62);
    return mix(ambient * low, ambient, clamp(normal_y * 0.5 + 0.5, 0.0, 1.0));
}

fn sun_band(diffuse: f32) -> f32 {
    let steps = frame.lighting.z;
    if steps < 2.0 { return diffuse; }
    let value = diffuse * steps;
    let softness = frame.lighting.w;
    if softness <= 0.0 { return floor(value + 0.5) / steps; }
    return (floor(value) + smoothstep(0.5 - softness * 0.5, 0.5 + softness * 0.5, fract(value))) / steps;
}

fn expose_scene(color: vec3<f32>) -> vec3<f32> {
    let v = color * frame.environment.w;
    if frame.lighting.y < 0.5 { return v; }
    return select(vec3<f32>(0.8) + 0.2 * (v - 0.8) / max(v - 0.6, vec3<f32>(0.00001)), v, v <= vec3<f32>(0.8));
}

fn mesh_normal(vertex: Vertex) -> vec3<f32> {
    let x = vertex.model_x.xyz;
    let y = vertex.model_y.xyz;
    let z = vertex.model_z.xyz;
    let yz = cross(y, z);
    let n = (yz * vertex.normal.x + cross(z, x) * vertex.normal.y +
        cross(x, y) * vertex.normal.z) * sign(dot(x, yz));
    return n * inverseSqrt(max(dot(n, n), 1e-20));
}

@vertex
fn scene(vertex: Vertex) -> Paint {
    let base = scene_paint(vertex);
    if dot(vertex.normal, vertex.normal) > 0.0 && (vertex.raster_kind & 255u) == 0u {
        return normal_paint(base.position, base.uv, vec4<f32>(expose_scene(base.rgba.rgb), base.rgba.a));
    }
    return base;
}

fn scene_paint(vertex: Vertex) -> Paint {
    if (vertex.raster_kind & 255u) != 0u {
        return world_line(vertex);
    }
    let model = mat4x4<f32>(vertex.model_x, vertex.model_y, vertex.model_z, vertex.model_w);
    var rgba = vertex.rgba * vertex.tint;
    if dot(vertex.normal, vertex.normal) > 0.0 {
        let n = mesh_normal(vertex);
        let diffuse = max(dot(n, frame.light.xyz), 0.0);
        let solar_weight = select(1.0 - frame.light.w, 1.0, frame.lighting.x > 0.5);
        let shade = ambient_light(n.y) + frame.light_color.xyz * (sun_band(diffuse) * solar_weight);
        rgba = vec4<f32>(rgba.rgb * shade, rgba.a);
    }
    return normal_paint(frame.view_projection * model * vec4<f32>(vertex.position, 1.0), vertex.uv, rgba);
}

fn sky_ray(ndc: vec2<f32>) -> vec3<f32> {
    let m = frame.view_projection;
    let forward = vec3<f32>(m[0].w, m[1].w, m[2].w);
    if dot(forward, forward) < 0.5 {
        // Orthographic rays are parallel. Translation affects neither case.
        return -vec3<f32>(m[0].z, m[1].z, m[2].z);
    }
    let right = vec3<f32>(m[0].x, m[1].x, m[2].x);
    let up = vec3<f32>(m[0].y, m[1].y, m[2].y);
    return forward + right * (ndc.x / dot(right, right)) + up * (ndc.y / dot(up, up));
}

@vertex
fn overlay(vertex: Vertex) -> Paint {
    if vertex.raster_kind == 3u {
        return Paint(vec4<f32>(vertex.position, 1.0), vertex.uv, vertex.rgba,
            sky_ray(vertex.position.xy), vertex.model_x, vertex.model_y, vertex.model_z,
            vertex.model_w, vertex.tint);
    }
    if vertex.raster_kind != 0u {
        return world_line(vertex);
    }
    return normal_paint(vec4<f32>(vertex.position, 1.0), vertex.uv, vertex.rgba);
}

fn world_line(vertex: Vertex) -> Paint {
    var p = vertex.position;
    var q = vertex.normal;
    var color = vertex.rgba;
    if vertex.raster_kind == 2u {
        let major_spacing = 4.0 * vertex.tint.x;
        let anchor = floor(frame.eye.xz / major_spacing) * major_spacing;
        p += vec3<f32>(anchor.x, 0.0, anchor.y);
        q += vec3<f32>(anchor.x, 0.0, anchor.y);
    }
    var a = frame.view_projection * vec4<f32>(p, 1.0);
    var b = frame.view_projection * vec4<f32>(q, 1.0);
    let plane_a = min(a.z, a.w - 0.00001);
    let plane_b = min(b.z, b.w - 0.00001);
    if plane_a < 0.0 && plane_b < 0.0 {
        return normal_paint(vec4<f32>(0.0, 0.0, -1.0, 1.0), vec2<f32>(0.5), vec4<f32>(0.0));
    }
    if plane_a < 0.0 {
        a = mix(a, b, plane_a / (plane_a - plane_b));
    }
    if plane_b < 0.0 {
        b = mix(b, a, plane_b / (plane_b - plane_a));
    }
    let delta = (b.xy / max(b.w, 0.00001) - a.xy / max(a.w, 0.00001)) * frame.viewport.xy;
    let side = vec2<f32>(-delta.y, delta.x) * inverseSqrt(max(dot(delta, delta), 0.000001));
    let offset = side * vertex.uv.x / frame.viewport.xy * a.w;
    let z = max(a.z - vertex.uv.y * a.w, 0.0);
    if vertex.raster_kind == 2u {
        let distance = length((p.xz + q.xz) * 0.5 - frame.eye.xz);
        color.a *= 1.0 - smoothstep(vertex.tint.y * 0.65, vertex.tint.y, distance);
    }
    return normal_paint(vec4<f32>(a.xy + offset, z, a.w), vec2<f32>(0.5), color);
}

fn star_hash(seed: u32) -> f32 {
    var h = seed;
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    return f32((h ^ (h >> 16u)) & 0xffffffu) / 16777216.0;
}

fn sky_stars(ray: vec3<f32>, density: f32) -> vec3<f32> {
    let a = abs(ray);
    var uv = ray.xy / max(a.z, 0.00001);
    var face = select(5u, 4u, ray.z >= 0.0);
    if a.x >= a.y && a.x >= a.z {
        uv = ray.yz / a.x;
        face = select(1u, 0u, ray.x >= 0.0);
    } else if a.y >= a.z {
        uv = ray.xz / a.y;
        face = select(3u, 2u, ray.y >= 0.0);
    }
    let p = (uv * 0.5 + 0.5) * 20.0;
    let cell = floor(p);
    let seed = u32(cell.x) * 73856093u ^ u32(cell.y) * 19349663u ^ face * 83492791u;
    if star_hash(seed) > density * 0.5 { return vec3<f32>(0.0); }
    let center = vec2<f32>(star_hash(seed ^ 0xa511e9b3u), star_hash(seed ^ 0x63d83595u)) * 0.64 + 0.18;
    let d = abs(fract(p) - center);
    let radius = 0.055 + star_hash(seed ^ 0x9e3779b9u) * 0.05;
    let core = max(0.0, 1.0 - length(d) / radius);
    let bars = max(vec2<f32>(0.0), vec2<f32>(1.0) - d / (radius * 0.28));
    let arms = max(vec2<f32>(0.0), vec2<f32>(1.0) - d / (radius * 1.7));
    let brightness = max(core, 0.7 * max(bars.x * arms.y, bars.y * arms.x)) * min(ray.y * 5.0, 1.0);
    var tint = vec3<f32>(0.72, 0.86, 1.0);
    if frame.lighting.y > 0.5 {
        tint = mix(vec3<f32>(1.0, 0.88, 0.72), vec3<f32>(0.68, 0.80, 1.0), star_hash(seed ^ 0x25cc5f23u));
    }
    return tint * brightness;
}

fn shade_sky(input: Paint) -> vec4<f32> {
    let ray = normalize(input.ray);
    if input.stars.z > 0.5 {
        let u = fract(atan2(ray.z, ray.x) / 6.28318530718 + 1.0);
        let v = acos(clamp(ray.y, -1.0, 1.0)) / 3.14159265359;
        let half_texel = 0.5 / f32(textureDimensions(color_texture).y);
        return textureSampleLevel(color_texture, color_sampler, vec2<f32>(u, clamp(v, half_texel, 1.0 - half_texel)), 0.0);
    }
    var blend = abs(ray.y);
    if input.atmosphere.x > 0.0 { blend = pow(blend, 1.0 + 2.0 * input.atmosphere.x); }
    let pole = select(input.ground.rgb, input.zenith.rgb, ray.y >= 0.0);
    var color = mix(input.horizon.rgb, pole, blend);
    if input.stars.x > 0.0 && input.stars.y > 0.0 && ray.y > 0.0 {
        color += sky_stars(ray, input.stars.x) * input.stars.y;
    }
    let alignment = dot(ray, frame.light.xyz);
    if input.atmosphere.z > 0.0 {
        let glow = clamp((alignment - input.atmosphere.w) / max(1.0 - input.atmosphere.w, 0.000001), 0.0, 1.0);
        color += frame.light_color.xyz * (glow * glow * input.atmosphere.z);
    }
    if alignment > input.atmosphere.y && input.atmosphere.y < 1.0 {
        var disk = clamp((alignment - input.atmosphere.y) / max(1.0 - input.atmosphere.y, 0.0000001), 0.0, 1.0);
        if frame.lighting.y > 0.5 { disk = smoothstep(0.0, 1.0, disk); }
        color = mix(color, frame.light_color.xyz, disk);
    }
    return vec4<f32>(expose_scene(color), 1.0);
}

@fragment
fn paint(input: Paint) -> @location(0) vec4<f32> {
    if input.zenith.a > 0.5 { return shade_sky(input); }
    // Product textures currently have one mip. An explicit level lets the
    // procedural sky bypass the material sample entirely.
    return textureSampleLevel(color_texture, color_sampler, input.uv, 0.0) * input.rgba;
}
