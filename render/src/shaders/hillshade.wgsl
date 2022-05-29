struct Uniform {
    azimuth: f32;
    zenith: f32;
    tile_size: u32;
    tile_offset: vec2<u32>;
};

[[group(0), binding(0)]]
var tile_atlas: texture_2d<i32>;

[[group(1), binding(0)]]
var<uniform> uniforms: Uniform;

[[stage(fragment)]]
fn main([[location(0)]] uv: vec2<f32>) -> [[location(0)]] f32 {
    let pixel = vec2<i32>(uniforms.tile_offset + vec2<u32>(uv * f32(uniforms.tile_size)));
    let min = vec3<i32>(uniforms.tile_offset);
    let max = vec3<i32>(uniforms.tile_offset + vec2<u32>(uniforms.tile_size));

    let a = textureLoad(tile_atlas, vec2<i32>(pixel.x - 1, pixel.y - 1), 0).r;
    let b = textureLoad(tile_atlas, vec2<i32>(pixel.x, pixel.y - 1), 0).r;
    let c = textureLoad(tile_atlas, vec2<i32>(pixel.x + 1, pixel.y - 1), 0).r;
    let d = textureLoad(tile_atlas, vec2<i32>(pixel.x - 1, pixel.y), 0).r;
    let f = textureLoad(tile_atlas, vec2<i32>(pixel.x + 1, pixel.y), 0).r;
    let g = textureLoad(tile_atlas, vec2<i32>(pixel.x - 1, pixel.y + 1), 0).r;
    let h = textureLoad(tile_atlas, vec2<i32>(pixel.x, pixel.y + 1), 0).r;
    let i = textureLoad(tile_atlas, vec2<i32>(pixel.x + 1, pixel.y + 1), 0).r;

    // Sobol
    let dzdx = f32((c + 2 * f + i) - (a + 2 * d + g));
    let dzdy = f32((g + 2 * h + i) - (a + 2 * b + c));
    let slope = atan(sqrt(dzdx * dzdx + dzdy * dzdy));
    var aspect: f32;
    if (dzdx != 0.0) {
        aspect = atan2(dzdy, -dzdx);
        if (aspect < 0.0) {
            aspect = aspect + 6.28318530718;
        }
    } else {
        if (dzdy > 0.0) {
            aspect = 1.57079632679;
        } else if (dzdy < 0.0) {
            aspect = 4.71238898038;
        } else {
            aspect = 0.0;
        }
    }

    return cos(uniforms.zenith) * cos(slope) + sin(uniforms.zenith) * sin(slope) * cos(uniforms.azimuth - aspect);
}
