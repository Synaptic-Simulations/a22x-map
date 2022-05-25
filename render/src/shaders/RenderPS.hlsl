struct LatLon {
    float lat;
    float lon;
};

[[vk::binding(0)]]
cbuffer UniformData {
    LatLon MapCenter; // Radians.
    float2 MapDiameter;
};

[[vk::binding(1)]] Texture2D<uint> TileMap;
[[vk::binding(2)]] ByteAddressBuffer Tiles[];

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

float4 Main(float2 UV: UV): SV_Target0 {
    LatLon position = Project(UV);
    uint lat = position.lat + 90.f;
    uint lon = position.lon + 180.f;
    uint index = TileMap.Load(int3(lat, lon, 0));
    if (index != 0) {
        return 1.f;
    } else {
        return 0.f;
    }
}
