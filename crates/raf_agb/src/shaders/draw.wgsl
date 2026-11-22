struct Frame {
    view_projection: mat4x4<f32>,
    light: vec4<f32>,
    light_color: vec4<f32>,
    viewport: vec4<f32>,
    eye: vec4<f32>,
    environment: vec4<f32>,
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
};

@vertex
fn scene(vertex: Vertex) -> Paint {
    return scene_paint(vertex);
}

fn scene_paint(vertex: Vertex) -> Paint {
    if (vertex.raster_kind & 255u) != 0u {
        return world_line(vertex);
    }
    let model = mat4x4<f32>(vertex.model_x, vertex.model_y, vertex.model_z, vertex.model_w);
    var rgba = vertex.rgba * vertex.tint;
    if dot(vertex.normal, vertex.normal) > 0.0 {
        let x = vertex.model_x.xyz;
        let y = vertex.model_y.xyz;
        let z = vertex.model_z.xyz;
        let yz = cross(y, z);
        // Inverse transpose without a matrix inverse. Preserve the orientation
        // of mirrored transforms and avoid normalizing degenerate vectors.
        let n = (yz * vertex.normal.x + cross(z, x) * vertex.normal.y +
            cross(x, y) * vertex.normal.z) * sign(dot(x, yz));
        let diffuse = max(dot(n, frame.light.xyz) * inverseSqrt(max(dot(n, n), 1e-20)), 0.0);
        let shade = (frame.environment.rgb * frame.light.w + frame.light_color.xyz * (diffuse * (1.0 - frame.light.w))) * frame.environment.w;
        rgba = vec4<f32>(rgba.rgb * shade, rgba.a);
    }
    return Paint(frame.view_projection * model * vec4<f32>(vertex.position, 1.0), vertex.uv, rgba);
}

@vertex
fn overlay(vertex: Vertex) -> Paint {
    if vertex.raster_kind == 3u {
        let clip = frame.view_projection * vec4<f32>(frame.eye.xyz + vertex.position, 1.0);
        let color = vec4<f32>(vertex.rgba.rgb * frame.environment.w, vertex.rgba.a);
        return Paint(vec4<f32>(clip.xy, clip.w, clip.w), vertex.uv, color);
    }
    if vertex.raster_kind != 0u {
        return world_line(vertex);
    }
    return Paint(vec4<f32>(vertex.position, 1.0), vertex.uv, vertex.rgba);
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
        return Paint(vec4<f32>(0.0, 0.0, -1.0, 1.0), vec2<f32>(0.5), vec4<f32>(0.0));
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
    return Paint(vec4<f32>(a.xy + offset, z, a.w), vec2<f32>(0.5), color);
}

@fragment
fn paint(input: Paint) -> @location(0) vec4<f32> {
    return textureSample(color_texture, color_sampler, input.uv) * input.rgba;
}
