struct LatLon {
    lat: f32;
    lon: f32;
};

struct Uniform {
    map_center: LatLon;
    [[align(16)]] vertical_diameter: f32;
    aspect_ratio: f32;
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
[[group(0), binding(4)]]
var hillshade_atlas: texture_2d<f32>;

var<private> l500: vec3<f32> = vec3<f32>(0.17, 0.31, 0.16);
var<private> l1000: vec3<f32> = vec3<f32>(0.22, 0.36, 0.19);
var<private> l2000: vec3<f32> = vec3<f32>(0.33, 0.46, 0.21);
var<private> l3000: vec3<f32> = vec3<f32>(0.41, 0.51, 0.28);
var<private> l4000: vec3<f32> = vec3<f32>(0.49, 0.5, 0.3);
var<private> l5000: vec3<f32> = vec3<f32>(0.47, 0.52, 0.26);
var<private> l6000: vec3<f32> = vec3<f32>(0.46, 0.49, 0.29);
var<private> l7000: vec3<f32> = vec3<f32>(0.41, 0.43, 0.24);
var<private> l8000: vec3<f32> = vec3<f32>(0.45, 0.4, 0.22);
var<private> l9000: vec3<f32> = vec3<f32>(0.4, 0.35, 0.18);
var<private> l10000: vec3<f32> = vec3<f32>(0.33, 0.25, 0.12);
var<private> l11000: vec3<f32> = vec3<f32>(0.27, 0.21, 0.11);
var<private> l12000: vec3<f32> = vec3<f32>(0.31, 0.3, 0.25);
var<private> l13000: vec3<f32> = vec3<f32>(0.35, 0.38, 0.33);
var<private> l15000: vec3<f32> = vec3<f32>(0.43, 0.45, 0.43);
var<private> l17000: vec3<f32> = vec3<f32>(0.48, 0.48, 0.46);
var<private> l19000: vec3<f32> = vec3<f32>(0.51, 0.53, 0.52);
var<private> l21000: vec3<f32> = vec3<f32>(0.51, 0.55, 0.55);
var<private> l33000: vec3<f32> = vec3<f32>(0.56, 0.6, 0.6);
var<private> unknown_terrain: vec3<f32> = vec3<f32>(0.41, 0.15, 0.42);
var<private> water: vec3<f32> = vec3<f32>(0.06, 0.24, 0.41);
var<private> taws_yellow: vec3<f32> = vec3<f32>(0.99, 0.93, 0.09);
var<private> taws_red: vec3<f32> = vec3<f32>(0.93, 0.12, 0.14);

fn degrees(radians: f32) -> f32 {
    return radians * 57.295779513082322865;
}

fn project(uv: vec2<f32>) -> LatLon {
    let headsin = sin(uniforms.heading);
    let headcos = cos(uniforms.heading);
    let offset_uv = vec2<f32>(uv.x - 0.5, uv.y - 0.5);
    let scaled_uv = vec2<f32>(offset_uv.x * uniforms.aspect_ratio, offset_uv.y);
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

fn map_height(height: u32) -> vec3<f32> {
    let is_water = ((height >> 15u) & 1u) == 1u;
    let height = ~(1u << 15u) & height;
    if (is_water) {
        return water;
    } else {
        let feet = i32(f32(i32(height) - 500) * 3.28084);
        if (feet - 2000 > i32(uniforms.altitude)) {
            return taws_red;
        } else if (feet > i32(uniforms.altitude - 500.0)) {
            return taws_yellow;
        } else if (feet < 500) {
            return l500;
        } else {
            switch (feet / 1000) {
                case 0: { return l1000; }
                case 1: { return l2000; }
                case 2: { return l3000; }
                case 3: { return l4000; }
                case 4: { return l5000; }
                case 5: { return l6000; }
                case 6: { return l7000; }
                case 7: { return l8000; }
                case 8: { return l9000; }
                case 9: { return l10000; }
                case 10: { return l11000; }
                case 11: { return l12000; }
                case 12: { return l13000; }
                case 13: { return l15000; }
                case 14: { return l15000; }
                case 15: { return l17000; }
                case 16: { return l17000; }
                case 17: { return l19000; }
                case 18: { return l19000; }
                case 19: { return l21000; }
                case 20: { return l21000; }
                case 21: { return l33000; }
                case 22: { return l33000; }
                case 23: { return l33000; }
                case 24: { return l33000; }
                case 25: { return l33000; }
                case 26: { return l33000; }
                case 27: { return l33000; }
                case 28: { return l33000; }
                case 29: { return l33000; }
                case 30: { return l33000; }
                case 31: { return l33000; }
                case 32: { return l33000; }
                default: { return unknown_terrain; }
            }
        }
    }
}

[[stage(fragment)]]
fn main([[location(0)]] uv: vec2<f32>) -> [[location(0)]] vec4<f32> {
    let rad_position = project(uv);
    let lat = degrees(rad_position.lat) + 90.0;
    var lon = (degrees(rad_position.lon) + 180.0) % 360.0;
    if (lon < 0.0) {
        lon = lon + 360.0;
    }
    let tile_loc = vec2<u32>(u32(lon), u32(lat));
    let index = tile_loc.y * 360u + tile_loc.x;
    tile_status.values[index] = 1u;
    let tile_offset = textureLoad(tile_map, vec2<i32>(tile_loc), 0);

    let atlas_dimensions = textureDimensions(tile_atlas, 0);
    let not_found = tile_offset.x == u32(atlas_dimensions.x);
    let unloaded = tile_offset.y == u32(atlas_dimensions.y);

    var ret: vec3<f32>;
    if (not_found) {
        ret = water;
    } else if (unloaded) {
        ret = vec3<f32>(0.0, 0.0, 0.0);
    } else {
        let tile_uv = vec2<f32>(lon - floor(lon), 1.0 - (lat - floor(lat)));
        let pixel = vec2<f32>(f32(tile_offset.x), f32(tile_offset.y)) + tile_uv * f32(uniforms.tile_size);
        let height = textureLoad(tile_atlas, vec2<i32>(i32(pixel.x), i32(pixel.y)), 0).x;
        let hillshade = textureLoad(hillshade_atlas, vec2<i32>(i32(pixel.x), i32(pixel.y)), 0).x;
        ret = map_height(height) * mix(0.6, 1.0, hillshade);
    }
    return vec4<f32>(pow(ret, vec3<f32>(2.2)), 1.0);
}
