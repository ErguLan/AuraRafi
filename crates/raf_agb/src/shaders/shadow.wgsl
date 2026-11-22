
struct ShadowUniform {
    light_view_projection: mat4x4<f32>,
    parameters: vec4<f32>,
};

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var<uniform> shadow_frame: ShadowUniform;

@vertex
fn shadow_depth(vertex: Vertex) -> Paint {
    let model = mat4x4<f32>(vertex.model_x, vertex.model_y, vertex.model_z, vertex.model_w);
    let clip = shadow_frame.light_view_projection * model * vec4<f32>(vertex.position, 1.0);
    return Paint(clip, vertex.uv, vertex.rgba * vertex.tint);
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
};

@vertex
fn shadow_scene(vertex: Vertex) -> ShadowPaint {
    let base = scene_paint(vertex);
    var light_clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    var diffuse = vec3<f32>(0.0);
    if (vertex.raster_kind & 256u) != 0u && dot(vertex.normal, vertex.normal) > 0.0 {
        let model = mat4x4<f32>(vertex.model_x, vertex.model_y, vertex.model_z, vertex.model_w);
        light_clip = shadow_frame.light_view_projection * model * vec4<f32>(vertex.position, 1.0);
        let ambient = vertex.rgba.rgb * vertex.tint.rgb * frame.environment.rgb * (frame.light.w * frame.environment.w);
        diffuse = max(base.rgba.rgb - ambient, vec3<f32>(0.0));
    }
    return ShadowPaint(base.position, base.uv, base.rgba, light_clip, diffuse);
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
        let compare_depth = projected.z - shadow_frame.parameters.x;
        let upper = size - vec2<i32>(1);
        let a = textureLoad(shadow_map, clamp(origin, vec2<i32>(0), upper), 0);
        let b = textureLoad(shadow_map, clamp(origin + vec2<i32>(1, 0), vec2<i32>(0), upper), 0);
        let c = textureLoad(shadow_map, clamp(origin + vec2<i32>(0, 1), vec2<i32>(0), upper), 0);
        let d = textureLoad(shadow_map, clamp(origin + vec2<i32>(1), vec2<i32>(0), upper), 0);
        let samples = step(vec4<f32>(compare_depth), vec4<f32>(a, b, c, d));
        diffuse_visibility = mix(mix(samples.x, samples.y, weight.x), mix(samples.z, samples.w, weight.x), weight.y);
    }
    let lit = max(input.rgba.rgb - input.diffuse * (1.0 - diffuse_visibility), vec3<f32>(0.0));
    return textureSample(color_texture, color_sampler, input.uv) * vec4<f32>(lit, input.rgba.a);
}
