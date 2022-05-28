#include "Common.hlsl"

#define L500 float3(0.17f, 0.31f, 0.16f)
#define L1000 float3(0.22f, 0.36f, 0.19f)
#define L2000 float3(0.33f, 0.46f, 0.21f)
#define L3000 float3(0.41f, 0.51f, 0.28f)
#define L4000 float3(0.49f, 0.55f, 0.3f)
#define L5000 float3(0.47f, 0.52f, 0.26f)
#define L6000 float3(0.46f, 0.49f, 0.29f)
#define L7000 float3(0.41f, 0.43f, 0.24f)
#define L8000 float3(0.45f, 0.4f, 0.22f)
#define L9000 float3(0.4f, 0.35f, 0.18f)
#define L10000 float3(0.33f, 0.25f, 0.12f)
#define L11000 float3(0.27f, 0.21f, 0.11f)
#define L12000 float3(0.31f, 0.3f, 0.25f)
#define L13000 float3(0.35f, 0.38f, 0.33f)
#define L15000 float3(0.43f, 0.45f, 0.43f)
#define L17000 float3(0.48f, 0.48f, 0.46f)
#define L19000 float3(0.51f, 0.53f, 0.52f)
#define L21000 float3(0.51f, 0.55f, 0.55f)
#define L33000 float3(0.56f, 0.6f, 0.6f)
#define UNKNOWN_TERRAIN float3(0.41f, 0.15f, 0.42f)
#define WATER float3(0.06f, 0.24f, 0.41f)
#define TAWS_YELLOW float3(0.99f, 0.93f, 0.09f)
#define TAWS_RED float3(0.93f, 0.12f, 0.14f)

[[vk::binding(1)]] SamplerState Sampler;
[[vk::binding(2)]] Texture2D Heightmap;

float3 MapHeightToColor(int height) {
    if (height == -600) {
        return 0.f;
    } else if (height == -500) {
        return WATER;
    } else {
        int feet = height * 3.28084f;
        if (feet - 2000 > Altitude) {
            return TAWS_RED;
        } else if (feet > Altitude - 500) {
            return TAWS_YELLOW;
        } else if (feet < 500) {
            return L500;
        } else {
            switch(feet / 1000) {
                case 0: return L1000;
                case 1: return L2000;
                case 2: return L3000;
                case 3: return L4000;
                case 4: return L5000;
                case 5: return L6000;
                case 6: return L7000;
                case 7: return L8000;
                case 8: return L9000;
                case 9: return L10000;
                case 10: return L11000;
                case 11: return L12000;
                case 12: return L13000;
                case 13: return L15000;
                case 14: return L15000;
                case 15: return L17000;
                case 16: return L17000;
                case 17: return L19000;
                case 18: return L19000;
                case 19: return L21000;
                case 20: return L21000;
                case 21: return L33000;
                case 22: return L33000;
                case 23: return L33000;
                case 24: return L33000;
                case 25: return L33000;
                case 26: return L33000;
                case 27: return L33000;
                case 28: return L33000;
                case 29: return L33000;
                case 30: return L33000;
                case 31: return L33000;
                case 32: return L33000;     
            }
        }
    }

    return UNKNOWN_TERRAIN;
}

float4 Main(float2 UV: UV): SV_Target0 {
    int3 heightmap_dimensions;
    Heightmap.GetDimensions(0, heightmap_dimensions.x, heightmap_dimensions.y, heightmap_dimensions.z);
    float2 uvd = 1.f / heightmap_dimensions.xy;
    float uvdx = uvd.x;
    float uvdy = uvd.y;

    float4 a = Heightmap.Sample(Sampler, UV - uvd);
    float4 b = Heightmap.Sample(Sampler, UV + float2(0.f, -uvdy));
    float4 c = Heightmap.Sample(Sampler, UV + float2(uvdx, -uvdy));
    float4 d = Heightmap.Sample(Sampler, UV + float2(-uvdx, 0.f));
    float4 e = Heightmap.Sample(Sampler, UV);
    float4 f = Heightmap.Sample(Sampler, UV + float2(uvdx, 0.f));
    float4 g = Heightmap.Sample(Sampler, UV + float2(-uvdx, uvdy));
    float4 h = Heightmap.Sample(Sampler, UV + float2(0.f, uvdy));
    float4 i = Heightmap.Sample(Sampler, UV + uvd);

    float2 dr = i.xy - a.xy;
    float dpdx = dr.x * 6371000.f;
    float dpdy = dr.y * 6371000.f;

    float dzdx = ((c.z + 2.f * f.z + i.z) - (a.z + 2.f * d.z + g.z)) / (4.f * dpdx);
    float dzdy = ((g.z + 2.f * h.z + i.z) - (a.z * 2.f * b.z + c.z)) / (4.f * dpdy);

    float slope = atan(sqrt(dzdx * dzdx + dzdy * dzdy));
    float aspect = 0.f;
    if (dzdx != 0.f) {
        aspect = atan2(dzdy, -dzdx);
        if (aspect < 0.f) {
            aspect += 6.28318530718f;
        }
    } else {
        if (dzdy > 0.f) {
            aspect = 1.57079632679f;
        } else if (dzdy < 0.f) {
            aspect = 4.71238898038f;
        }
    }

    float zsin, zcos;
    sincos(Zenith, zsin, zcos);
    float ssin, scos;
    sincos(slope, ssin, scos);
    float hillshade = clamp(zcos * scos + zsin * ssin * cos(Azimuth - aspect), 0.f, 1.f);

    float3 height_color = MapHeightToColor(e.z) * (hillshade * 0.5f + 0.5f);

    return float4(pow(height_color, 2.2f), 1.f);
}
