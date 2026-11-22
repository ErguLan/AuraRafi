@group(0) @binding(0)
var source_texture: texture_2d<f32>;

const MAX_BLEND_STRENGTH: f32 = 0.45;
const CONTRAST_FLOOR: f32 = 0.035;
const CONTRAST_FULL_STRENGTH: f32 = 0.25;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let corner = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(corner * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return output;
}

fn luma(color: vec3<f32>) -> f32 {
    return color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722;
}

@fragment
fn fs_main(@builtin(position) fragment_position: vec4<f32>) -> @location(0) vec4<f32> {
    let dimensions = vec2<i32>(textureDimensions(source_texture, 0));
    let last_coordinate = max(dimensions - vec2<i32>(1, 1), vec2<i32>(0, 0));
    let center_coordinate = clamp(vec2<i32>(fragment_position.xy), vec2<i32>(0, 0), last_coordinate);

    let north_coordinate = max(center_coordinate + vec2<i32>(0, -1), vec2<i32>(0, 0));
    let south_coordinate = min(center_coordinate + vec2<i32>(0, 1), last_coordinate);
    let west_coordinate = max(center_coordinate + vec2<i32>(-1, 0), vec2<i32>(0, 0));
    let east_coordinate = min(center_coordinate + vec2<i32>(1, 0), last_coordinate);

    let center = textureLoad(source_texture, center_coordinate, 0);
    let north = textureLoad(source_texture, north_coordinate, 0);
    let south = textureLoad(source_texture, south_coordinate, 0);
    let west = textureLoad(source_texture, west_coordinate, 0);
    let east = textureLoad(source_texture, east_coordinate, 0);

    let center_luma = luma(center.rgb);
    let north_luma = luma(north.rgb);
    let south_luma = luma(south.rgb);
    let west_luma = luma(west.rgb);
    let east_luma = luma(east.rgb);

    let local_min = min(center_luma, min(min(north_luma, south_luma), min(west_luma, east_luma)));
    let local_max = max(center_luma, max(max(north_luma, south_luma), max(west_luma, east_luma)));
    let contrast = local_max - local_min;

    let gradient_x = abs(east_luma - west_luma);
    let gradient_y = abs(south_luma - north_luma);
    let gradient_sum = gradient_x + gradient_y;
    var corner_measure = 0.0;
    if gradient_sum > 0.0 {
        corner_measure = clamp(2.0 * min(gradient_x, gradient_y) / gradient_sum, 0.0, 1.0);
    }
    let contrast_weight = clamp(
        (contrast - CONTRAST_FLOOR) / (CONTRAST_FULL_STRENGTH - CONTRAST_FLOOR),
        0.0,
        1.0,
    );
    let strength = clamp(MAX_BLEND_STRENGTH * contrast_weight * corner_measure, 0.0, MAX_BLEND_STRENGTH);

    let neighbor_average = (north.rgb + south.rgb + west.rgb + east.rgb) * 0.25;
    return vec4<f32>(mix(center.rgb, neighbor_average, strength), center.a);
}
