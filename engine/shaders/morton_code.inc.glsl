uint mortonCode(uvec3 vector, uvec3 size) {
    vector = min(size, vector);

    return vector.z * size.x * size.y +
    vector.y * size.x +
    vector.x;
}

uvec3 vectorFromMortonCode(uint mortonCode, uvec3 size) {
    return uvec3(mortonCode % size.x,
            (mortonCode / size.x) % size.y,
            mortonCode / (size.x * size.y));
}
