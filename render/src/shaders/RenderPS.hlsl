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

#define TILE_USED 1

struct LatLon {
    float lat;
    float lon;
};

[[vk::binding(0)]]
cbuffer UniformData {
    LatLon MapCenter; // Radians.
    float2 MapDiameter;
    uint TileSize;
};

[[vk::binding(1)]] Texture2D<uint2> TileMap;
[[vk::binding(2)]] RWStructuredBuffer<uint> TileStatus;
[[vk::binding(3)]] Texture2D<int> TileAtlas;

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

float3 MapHeightToColor(int height) {
    if (height < 500) {
        return L500;
    } else {
        if (height < 500) {
            return L500;
        } else {
            switch(height / 1000) {
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
    LatLon position = Project(UV);
    float lat = position.lat + 90.f;
    float lon = position.lon + 180.f;
    int mod_lon = lon % 360;
    if (mod_lon < 0)  {
        mod_lon = 360 + mod_lon;
    }
    uint2 tile_loc = uint2(mod_lon, lat);

    uint index = tile_loc.y * 360 + tile_loc.x;
    TileStatus[index] = TILE_USED;

    uint2 tile_offset = TileMap.Load(int3(tile_loc, 0));
    uint atlas_width, atlas_height, _;
    TileAtlas.GetDimensions(0, atlas_width, atlas_height, _);
    uint2 atlas_dimensions = uint2(atlas_width, atlas_height);
    bool not_found = tile_offset.x == atlas_dimensions.x;
    bool unloaded = tile_offset.y == atlas_dimensions.y;
    
    float3 ret;
    if (not_found) {
        ret = WATER;
    } else if (unloaded) {
        ret = float3(0.f, 0.f, 0.f);
    } else {
        float2 tile_uv = float2(1.f - (lat - (uint)lat), lon - (uint)lon);
        uint2 pixel = tile_uv * TileSize + tile_offset;
        int height = TileAtlas.Load(int3(pixel, 0));

        ret = MapHeightToColor((float)height * 3.28084f);
    }

    return float4(pow(ret, 2.2f), 1.f);
}
