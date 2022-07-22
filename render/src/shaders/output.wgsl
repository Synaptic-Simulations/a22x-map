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

[[group(0), binding(0)]]
var<uniform> uniforms: Uniform;
[[group(0), binding(1)]]
var heightmap: texture_2d<u32>;

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
var<private> water: vec3<f32> = vec3<f32>(0.01,0.09,0.31);
var<private> taws_yellow: vec3<f32> = vec3<f32>(0.99, 0.93, 0.09);
var<private> taws_red: vec3<f32> = vec3<f32>(0.93, 0.12, 0.14);

fn radians(degrees: f32) -> f32 {
    return degrees * 0.0174533;
}

fn map_height(height: u32) -> vec3<f32> {
    let feet = f32(i32(height) - 500) * 3.28084;
    if (feet > uniforms.altitude + 2000.0) {
        return taws_red;
    } else if (feet > uniforms.altitude - 500.0) {
        return taws_yellow;
    } else if (feet < 500.0) {
        return l500;
    } else {
        switch (i32(feet / 1000.0)) {
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

fn calculate_hillshade(height: f32) -> f32 {
    let m_per_pixel = (uniforms.vertical_diameter / f32(uniforms.output_resolution_y)) * 6371000.0;
    let zenith = radians(45.0);
    let azimuth = radians(135.0);

    let dzdx = dpdx(height); // * m_per_pixel;
    let dzdy = dpdy(height); // * m_per_pixel;
    let slope = atan(sqrt(dzdx * dzdx + dzdy * dzdy));

    let aspect = atan2(dzdy, -dzdx);

    return clamp(cos(zenith) * cos(slope) + sin(zenith) * sin(slope) * cos(azimuth - aspect), 0.0, 1.0);
}

[[stage(fragment)]]
fn main([[location(0)]] uv: vec2<f32>) -> [[location(0)]] vec4<f32> {
    let uv = vec2<f32>(uv.x, 1.0 - uv.y);
    let pixel = vec2<i32>(uv * vec2<f32>(f32(uniforms.output_resolution_x), f32(uniforms.output_resolution_y)));

    let height = textureLoad(heightmap, pixel, 0).x;
    let is_water = (height >> 15u) & 1u;

    var ret: vec3<f32>;
    if (is_water == 1u) {
        ret = water;
    } else {
        ret = map_height(height) * mix(0.4, 1.0, calculate_hillshade(f32(height)));
    }
    return vec4<f32>(pow(ret, vec3<f32>(2.2)), 1.0);
}
