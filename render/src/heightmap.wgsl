struct Ray {
    origin: vec3<f32>;
    direction: vec3<f32>;
};

struct Location {
    latitude: f32;
    longitude: f32;
    hit: bool;
};

struct Uniforms {
    camera_origin: vec3<f32>;
    __pad0: f32;
    camera_up: vec3<f32>;
    __pad1: f32;
    camera_right: vec3<f32>;
    pixel_delta: f32;
    screen: vec2<f32>;
};

[[group(0), binding(0)]]
var<uniform> uniforms: Uniforms;
[[group(0), binding(1)]]
var height: texture_storage_2d<r32float, write>;

fn intersect_sphere(ray: Ray, radius: f32) -> Location {
    let a = dot(ray.direction, ray.direction);
    let b = 2.0 * dot(ray.direction, ray.origin);
    let c = dot(ray.origin, ray.origin) - radius * radius;

    let discriminant = b * b - 4.0 * a * c;
    if (discriminant < 0.0) {
        return Location(0.0, 0.0, false);
    }

    let t = (-b - sqrt(discriminant)) / (2.0 * a);
    let intersection = ray.origin + ray.direction * t;
    let latitude = 180.0 - (acos(intersection.y / radius) * 180.0 / 3.14159265);
    let longitude = atan(-intersection.x / intersection.z) * 180.0 / 3.14159265;

    return Location(latitude, longitude, true);
}

[[stage(compute), workgroup_size(1, 1, 1)]]
fn main(
    [[builtin(global_invocation_id)]] invocation: vec3<u32>
) {
    let pixel = invocation.xy;
    let offset = (vec2<f32>(pixel) - 0.5 * uniforms.screen) * uniforms.pixel_delta;
    let origin = uniforms.camera_origin + uniforms.camera_up * offset.y + uniforms.camera_right * offset.x;
    let direction = normalize(-uniforms.camera_origin);
    let location = intersect_sphere(Ray(origin, direction), 6371000.0);
    textureStore(height, vec2<i32>(pixel), vec4<f32>(select(0.0, location.latitude, location.hit)));
}
