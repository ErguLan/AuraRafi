// WGSL shaders module.
// These shaders are prepared for when GPU rendering is enabled.
// When use_gpu = false (default), this module is not loaded.
// The shader source is embedded as string constants for zero-IO.

/// Basic PBR vertex shader (WGSL).
/// Transforms vertices and passes normals/UVs to fragment stage.
pub const PBR_VERTEX_WGSL: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct Uniforms {
    model: mat4x4<f32>,
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    camera_pos: vec3<f32>,
    time: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_pos = uniforms.model * vec4<f32>(input.position, 1.0);
    output.world_position = world_pos.xyz;
    output.world_normal = (uniforms.model * vec4<f32>(input.normal, 0.0)).xyz;
    output.clip_position = uniforms.projection * uniforms.view * world_pos;
    output.uv = input.uv;
    return output;
}
"#;

/// Basic PBR fragment shader (WGSL).
/// Implements metallic/roughness workflow with one directional light.
pub const PBR_FRAGMENT_WGSL: &str = r#"
struct PbrMaterial {
    base_color: vec4<f32>,
    metallic: f32,
    roughness: f32,
    emissive: vec3<f32>,
    _padding: f32,
};

struct LightData {
    direction: vec3<f32>,
    intensity: f32,
    color: vec3<f32>,
    ambient: f32,
};

@group(1) @binding(0) var<uniform> material: PbrMaterial;
@group(1) @binding(1) var<uniform> light: LightData;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(input.world_normal);
    let L = normalize(light.direction);
    let V = normalize(uniforms.camera_pos - input.world_position);
    let H = normalize(L + V);

    // Lambertian diffuse.
    let NdotL = max(dot(N, L), 0.0);

    // Blinn-Phong specular approximation.
    let NdotH = max(dot(N, H), 0.0);
    let shininess = mix(8.0, 128.0, 1.0 - material.roughness);
    let spec = pow(NdotH, shininess);

    // Fresnel (Schlick approximation).
    let F0 = mix(vec3<f32>(0.04), material.base_color.rgb, material.metallic);
    let VdotH = max(dot(V, H), 0.0);
    let fresnel = F0 + (vec3<f32>(1.0) - F0) * pow(1.0 - VdotH, 5.0);

    // Combine.
    let diffuse = material.base_color.rgb * (1.0 - material.metallic);
    let ambient_term = diffuse * light.ambient;
    let diffuse_term = diffuse * NdotL * light.color * light.intensity;
    let specular_term = fresnel * spec * light.intensity;

    let color = ambient_term + diffuse_term + specular_term + material.emissive;
    return vec4<f32>(color, material.base_color.a);
}
"#;

/// Flat/unlit shader for debug, wireframe, gizmos.
pub const FLAT_VERTEX_WGSL: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct Uniforms {
    mvp: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}
"#;

/// Flat fragment shader.
pub const FLAT_FRAGMENT_WGSL: &str = r#"
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

/// Post-process bloom shader (two-pass: extract bright + blur + composite).
pub const BLOOM_WGSL: &str = r#"
// Brightness extraction pass.
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    _pad: vec2<f32>,
};

@group(0) @binding(2) var<uniform> bloom: BloomUniforms;

@fragment
fn fs_extract(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let color = textureSample(input_tex, tex_sampler, uv);
    let brightness = dot(color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (brightness > bloom.threshold) {
        return vec4<f32>(color.rgb * bloom.intensity, 1.0);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
"#;

/// Shadow map vertex shader.
pub const SHADOW_VERTEX_WGSL: &str = r#"
struct Uniforms {
    light_vp: mat4x4<f32>,
    model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return uniforms.light_vp * uniforms.model * vec4<f32>(position, 1.0);
}
"#;

/// FXAA post-process shader.
pub const FXAA_WGSL: &str = r#"
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

struct FxaaUniforms {
    texel_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(2) var<uniform> fxaa: FxaaUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let center = textureSample(input_tex, tex_sampler, uv);
    let n = textureSample(input_tex, tex_sampler, uv + vec2<f32>(0.0, -fxaa.texel_size.y));
    let s = textureSample(input_tex, tex_sampler, uv + vec2<f32>(0.0,  fxaa.texel_size.y));
    let e = textureSample(input_tex, tex_sampler, uv + vec2<f32>( fxaa.texel_size.x, 0.0));
    let w = textureSample(input_tex, tex_sampler, uv + vec2<f32>(-fxaa.texel_size.x, 0.0));

    let luma_c = dot(center.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let luma_n = dot(n.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let luma_s = dot(s.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let luma_e = dot(e.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let luma_w = dot(w.rgb, vec3<f32>(0.299, 0.587, 0.114));

    let range = max(max(luma_n, luma_s), max(luma_e, luma_w)) - min(min(luma_n, luma_s), min(luma_e, luma_w));

    if (range < 0.05) {
        return center;
    }

    // Simple blur for edges.
    return (center + n + s + e + w) / 5.0;
}
"#;
