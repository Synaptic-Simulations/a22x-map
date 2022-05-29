struct Output {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] uv: vec2<f32>;
};

[[stage(vertex)]]
fn main([[builtin(vertex_index)]] id: u32) -> Output {
    let uv = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    return Output(vec4<f32>(uv * 2. - 1., 0., 1.), uv);
}
