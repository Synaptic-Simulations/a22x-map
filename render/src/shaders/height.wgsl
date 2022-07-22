struct LatLon {
    lat: f32;
    lon: f32;
};

struct Uniform {
    map_center: LatLon;
    [[align(16)]] vertical_diameter: f32;
    output_resolution_x: u32;
    output_resolution_y: u32;
    tile_size: u32;
    heading: f32;
    altitude: f32;
};

struct TileStatus {
    values: array<u32>;
};

[[group(0), binding(0)]]
var<uniform> uniforms: Uniform;
[[group(0), binding(1)]]
var tile_map: texture_2d<u32>;
[[group(0), binding(2)]]
var<storage, read_write> tile_status: TileStatus;
[[group(0), binding(3)]]
var tile_atlas: texture_2d<u32>;



fn degrees(radians: f32) -> f32 {
    return radians * 57.295779513082322865;
}

fn project(uv: vec2<f32>) -> LatLon {
    let aspect_ratio = f32(uniforms.output_resolution_x) / f32(uniforms.output_resolution_y);
    let headsin = sin(uniforms.heading);
    let headcos = cos(uniforms.heading);
    let offset_uv = vec2<f32>(uv.x - 0.5, uv.y - 0.5);
    let scaled_uv = vec2<f32>(offset_uv.x * aspect_ratio, offset_uv.y);
    let rotated_uv = vec2<f32>(scaled_uv.x * headcos - scaled_uv.y * headsin, scaled_uv.x * headsin + scaled_uv.y * headcos);
    let uv = vec2<f32>(rotated_uv.x + 0.5, rotated_uv.y + 0.5);
    let xy = (uv - vec2<f32>(0.5, 0.5)) * uniforms.vertical_diameter;

    let latsin = sin(uniforms.map_center.lat);
    let latcos = cos(uniforms.map_center.lat);
    let c = sqrt(xy.x * xy.x + xy.y * xy.y);
    let csin = sin(c);
    let ccos = cos(c);

    let lat = asin(ccos * latsin + xy.y * csin * latcos / c);
    let lon = uniforms.map_center.lon + atan2(xy.x * csin, c * latcos * ccos - xy.y * latsin * csin);

    return LatLon(lat, lon);
}

fn sample_globe(lat: f32, lon: f32) -> u32 {
    let tile_loc = vec2<u32>(u32(lon), u32(lat));
    let index = tile_loc.y * 360u + tile_loc.x;
    tile_status.values[index] = 1u;
    let tile_offset = vec2<i32>(textureLoad(tile_map, vec2<i32>(tile_loc), 0)).xy;

    let atlas_dimensions = textureDimensions(tile_atlas, 0);
    let not_found = tile_offset.x == i32(atlas_dimensions.x);
    let unloaded = tile_offset.y == i32(atlas_dimensions.y);

    if (not_found) {
        return 1u << 15u;
    } else if (unloaded) {
        return ~0u;
    } else {
        let tile_uv = vec2<f32>(lon - floor(lon), 1.0 - (lat - floor(lat)));
        let pixel = vec2<f32>(tile_offset) + tile_uv * f32(uniforms.tile_size);

        return textureLoad(tile_atlas, vec2<i32>(pixel), 0).x;
    }
}

[[stage(fragment)]]
fn main([[location(0)]] uv: vec2<f32>) -> [[location(0)]] u32 {
    let rad_position = project(uv);
    let lat = degrees(rad_position.lat) + 90.0;
    var lon = (degrees(rad_position.lon) + 180.0) % 360.0;
    if (lon < 0.0) {
        lon = lon + 360.0;
    }

    let tile_uv = vec2<f32>(lon - floor(lon), 1.0 - (lat - floor(lat)));
    let pixel = tile_uv * f32(uniforms.tile_size);
    let pixel_offset = pixel - floor(pixel);

    let delta = 1.0 / f32(uniforms.tile_size);
    let x = sample_globe(lat, lon);
    let y = sample_globe(lat, lon + delta);
    let z = sample_globe(lat - delta, lon);
    let w = sample_globe(lat - delta, lon + delta);

    let xh = f32(~(1u << 15u) & x);
    let yh = f32(~(1u << 15u) & y);
    let zh = f32(~(1u << 15u) & z);
    let wh = f32(~(1u << 15u) & w);

    let xl_lerp = mix(xh, yh, pixel_offset.x);
    let xh_lerp = mix(zh, wh, pixel_offset.x);
    let height = u32(mix(xl_lerp, xh_lerp, pixel_offset.y));

    let xw = f32((x >> 15u) & 1u);
    let yw = f32((y >> 15u) & 1u);
    let zw = f32((z >> 15u) & 1u);
    let ww = f32((w >> 15u) & 1u);

    let xl_lerp = mix(xw, yw, pixel_offset.x);
    let xh_lerp = mix(zw, ww, pixel_offset.x);
    let is_water = select(0u, 1u, mix(xl_lerp, xh_lerp, pixel_offset.y) > 0.5);

    return (is_water << 15u) | height;
}
