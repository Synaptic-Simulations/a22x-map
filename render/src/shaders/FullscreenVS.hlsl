struct Output {
    float2 UV: UV;
    float4 Position: SV_Position;
};

Output Main(uint Index: SV_VertexID) {
    Output output;

    output.UV = float2((Index << 1) & 2, Index & 2);
    output.Position = float4(output.UV * 2.f - 1.f, 0.f, 1.f);

    return output;
}
