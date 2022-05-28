#include "Common.hlsl"

#define TILE_USED 1

[[vk::binding(1)]] Texture2D<uint2> TileMap;
[[vk::binding(2)]] RWStructuredBuffer<uint> TileStatus;
[[vk::binding(3)]] Texture2D<int> TileAtlas;

LatLon Project(float2 UV) {
    float headsin, headcos;
    sincos(Heading, headsin, headcos);
    float2 c_uv = float2(UV.x * AspectRatio, UV.y);
    float2 uv = float2(c_uv.x * headcos - c_uv.y * headsin, c_uv.x * headsin + c_uv.y * headcos);
    float2 xy = (uv - float2(0.5f, 0.5f)) * VerticalDiameter;

    float latsin, latcos;
    sincos(MapCenter.lat, latsin, latcos);
    float csin, ccos;
    float c = sqrt(xy.x * xy.x + xy.y * xy.y);
    sincos(c, csin, ccos);

    float lat = asin(ccos * latsin + xy.y * csin * latcos / c);
    float lon = MapCenter.lon + atan2(xy.x * csin, c * latcos * ccos - xy.y * latsin * csin);

    LatLon ret = { lat, lon };
    return ret;
}

float4 Main(float2 UV: UV): SV_Target0 {
    LatLon rad_position = Project(UV);
    LatLon position = { RadToDeg(rad_position.lat), RadToDeg(rad_position.lon) };
    float lat = position.lat + 90.f;
    float raw_lon = position.lon + 180.f;
    float lon = raw_lon % 360;
    if (lon < 0)  {
        lon = 360 + lon;
    }
    uint2 tile_loc = uint2(lon, lat);

    uint index = tile_loc.y * 360 + tile_loc.x;
    TileStatus[index] = TILE_USED;
    uint2 tile_offset = TileMap.Load(int3(tile_loc, 0));

    int atlas_width, atlas_height, _;
    TileAtlas.GetDimensions(0, atlas_width, atlas_height, _);
    int2 atlas_dimensions = uint2(atlas_width, atlas_height);
    bool not_found = tile_offset.x == (uint)atlas_dimensions.x;
    bool unloaded = tile_offset.y == (uint)atlas_dimensions.y;
    
    float3 ret;
    ret.xy = float2(rad_position.lon, rad_position.lat);
    if (not_found) {
        ret.z = -500;
    } else if (unloaded) {
        ret.z = -600;
    } else {
        float2 tile_uv = float2(1.f - (lat - (uint)lat), lon - (uint)lon);
        uint2 pixel = tile_uv * TileSize + tile_offset;
        int height = TileAtlas.Load(int3(pixel, 0));

        ret.z = height;
    }
    return float4(ret, 0.f);
}
