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

struct LatLon {
    float lat;
    float lon;
};

[[vk::binding(0)]]
cbuffer UniformData {
    LatLon MapCenter; // Radians.
    float2 MapDiameter;
    uint3 AtlasSize;
};

[[vk::binding(1)]] Texture2D<uint2> TileMap;
[[vk::binding(2)]] Texture2D<int> TileAtlas;

float DegToRad(float deg) {
    return deg * 0.01745329251f;
}

float RadToDeg(float rad) {
    return rad * 57.2957795131f;
}

LatLon Project(float2 UV) {
    float2 xy = (UV - float2(0.5f, 0.5f)) * MapDiameter;

    float latsin, latcos;
    sincos(MapCenter.lat, latsin, latcos);
    float csin, ccos;
    float c = sqrt(xy.x * xy.x + xy.y * xy.y);
    sincos(c, csin, ccos);

    float lat = asin(ccos * latsin + xy.y * csin * latcos / c);
    float lon = MapCenter.lon + atan2(xy.x * csin, c * latcos * ccos - xy.y * latsin * csin);

    LatLon ret = { RadToDeg(lat), RadToDeg(lon) };
    return ret;
}

float3 MapHeightToColor(uint height) {
    if (height < 500) {
        return L500;
    } else {
        float4 mapping[] = {
            L1000,
            L2000,
            L3000,
            L4000,
            L5000,
            L6000,
            L7000,
            L8000,
            L9000,
            L10000,
            L11000,
            L12000,
            L13000,
            L15000,
            L15000,
            L17000,
            L17000,
            L19000,
            L19000,
            L21000,
            L21000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000,
            L33000
        };

        return mapping[height / 1000];
    }
}

float4 Main(float2 UV: UV): SV_Target0 {
    LatLon position = Project(UV);
    float lat = position.lat + 90.f;
    float lon = position.lon + 180.f;
    uint2 tile_offset = TileMap.Load(int3(lon, lat, 0));
    
    if (tile_offset.x < AtlasSize.x) {
        float2 tile_uv = float2(lat - (uint)lat, lon - (uint)lon);
        uint2 pixel = tile_uv * AtlasSize.z + tile_offset;
        int height = TileAtlas.Load(int3(pixel, 0));
        return pow(float4(MapHeightToColor(height * 3.28084), 1.f), 2.2f); // TODO: Remove stupid sRGB
    } else {
        return float4(WATER, 1.f);
    }
}
