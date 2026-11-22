
struct ShadowUniform {
    light_view_projection: mat4x4<f32>,
    parameters: vec4<f32>,
    sampling: vec4<f32>,
};

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var<uniform> shadow_frame: ShadowUniform;

@vertex
fn shadow_depth(vertex: Vertex) -> Paint {
    let model = mat4x4<f32>(vertex.model_x, vertex.model_y, vertex.model_z, vertex.model_w);
    let clip = shadow_frame.light_view_projection * model * vec4<f32>(vertex.position, 1.0);
    return normal_paint(clip, vertex.uv, vertex.rgba * vertex.tint);
}

@fragment
fn shadow_caster(input: Paint) {
    if textureSample(color_texture, color_sampler, input.uv).a * input.rgba.a < 0.5 {
        discard;
    }
}

struct ShadowPaint {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rgba: vec4<f32>,
    @location(2) light_clip: vec4<f32>,
    @location(3) diffuse: vec3<f32>,
    @location(4) world: vec3<f32>,
    @location(5) @interpolate(flat) lit: f32,
    @location(6) depth_bias: f32,
};

@vertex
fn shadow_scene(vertex: Vertex) -> ShadowPaint {
    let base = scene_paint(vertex);
    var light_clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    var diffuse = vec3<f32>(0.0);
    var world = vec3<f32>(0.0);
    var depth_bias = shadow_frame.parameters.x;
    let lit = dot(vertex.normal, vertex.normal) > 0.0 && (vertex.raster_kind & 255u) == 0u;
    if (vertex.raster_kind & 256u) != 0u && lit {
        let model = mat4x4<f32>(vertex.model_x, vertex.model_y, vertex.model_z, vertex.model_w);
        world = (model * vec4<f32>(vertex.position, 1.0)).xyz;
        light_clip = shadow_frame.light_view_projection * vec4<f32>(world, 1.0);
        let normal = mesh_normal(vertex);
        let ambient = vertex.rgba.rgb * vertex.tint.rgb * ambient_light(normal.y);
        let cosine = clamp(dot(normal, frame.light.xyz), 0.05, 1.0);
        let slope = min(sqrt(max(0.0, 1.0 - cosine * cosine)) / cosine, 8.0);
        depth_bias += slope * shadow_frame.sampling.x;
        diffuse = max(base.rgba.rgb - ambient, vec3<f32>(0.0));
    }
    return ShadowPaint(base.position, base.uv, base.rgba, light_clip, diffuse, world, select(0.0, 1.0, lit), depth_bias);
}

fn depth_compare(pixel: vec2<i32>, upper: vec2<i32>, reference: f32) -> f32 {
    return step(reference, textureLoad(shadow_map, clamp(pixel, vec2<i32>(0), upper), 0));
}

@fragment
fn shadow_paint(input: ShadowPaint) -> @location(0) vec4<f32> {
    var diffuse_visibility = 1.0;
    let projected = input.light_clip.xyz / max(input.light_clip.w, 0.00001);
    let uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if input.light_clip.w > 0.0 && projected.z >= 0.0 && projected.z <= 1.0
        && all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)) {
        let size = vec2<i32>(textureDimensions(shadow_map));
        let pixel = uv * vec2<f32>(size) - vec2<f32>(0.5);
        let origin = vec2<i32>(floor(pixel));
        let weight = fract(pixel);
        let compare_depth = projected.z - input.depth_bias;
        let upper = size - vec2<i32>(1);
        let taps = shadow_frame.parameters.w;
        if taps == 1.0 {
            diffuse_visibility = depth_compare(vec2<i32>(floor(pixel + 0.5)), upper, compare_depth);
        } else if taps == 9.0 {
            let center = vec2<i32>(floor(pixel + 0.5));
            var total = 0.0;
            for (var y = -1; y <= 1; y++) {
                for (var x = -1; x <= 1; x++) {
                    total += depth_compare(center + vec2<i32>(x, y), upper, compare_depth);
                }
            }
            diffuse_visibility = total / 9.0;
        } else {
            let a = depth_compare(origin, upper, compare_depth);
            let b = depth_compare(origin + vec2<i32>(1, 0), upper, compare_depth);
            let c = depth_compare(origin + vec2<i32>(0, 1), upper, compare_depth);
            let d = depth_compare(origin + vec2<i32>(1), upper, compare_depth);
            diffuse_visibility = mix(mix(a, b, weight.x), mix(c, d, weight.x), weight.y);
        }
        let range = shadow_frame.parameters.y;
        if range > 0.0 {
            let fade = clamp((range - distance(input.world, frame.eye.xyz)) / (range * 0.2), 0.0, 1.0);
            diffuse_visibility = 1.0 - (1.0 - diffuse_visibility) * fade * shadow_frame.parameters.z;
        }
    }
    var lit = max(input.rgba.rgb - input.diffuse * (1.0 - diffuse_visibility), vec3<f32>(0.0));
    if input.lit > 0.5 { lit = expose_scene(lit); }
    return textureSample(color_texture, color_sampler, input.uv) * vec4<f32>(lit, input.rgba.a);
}
