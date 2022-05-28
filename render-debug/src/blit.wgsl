struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] uv: vec2<f32>;
};

[[group(0), binding(0)]]
var s: sampler;
[[group(0), binding(1)]]
var tex: texture_2d<f32>;

[[stage(vertex)]]
fn vertex([[builtin(vertex_index)]] id: u32) -> VertexOutput {
    let uv = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    return VertexOutput(vec4<f32>(uv * 2. - 1., 0., 1.), uv);
}

[[stage(fragment)]]
fn pixel(vertex: VertexOutput) -> [[location(0)]] vec4<f32> {
    return textureSample(tex, s, vertex.uv);
}
