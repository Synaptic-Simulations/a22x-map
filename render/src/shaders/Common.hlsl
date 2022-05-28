struct LatLon {
    float lat;
    float lon;
};

float DegToRad(float deg) {
    return deg * 0.01745329251f;
}

float RadToDeg(float rad) {
    return rad * 57.2957795131f;
}

[[vk::binding(0)]]
cbuffer UniformData {
    LatLon MapCenter; // Radians.
    float VerticalDiameter;
    float AspectRatio;
    uint TileSize;
    float Heading; // Radians.
    float Zenith;
    float Azimuth;
    float Altitude;
};
