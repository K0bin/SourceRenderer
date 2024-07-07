vec3 calcSHIrradiance(vec3 N)
{
    vec3 irradiance = gSH9Color[0] * 0.282095f * A0
        + gSH9Color[1] * 0.488603f * N.y * A1
        + gSH9Color[2] * 0.488603f * N.z * A1
        + gSH9Color[3] * 0.488603f * N.x * A1
        + gSH9Color[4] * 1.092548f * N.x * N.y * A2
        + gSH9Color[5] * 1.092548f * N.y * N.z * A2
        + gSH9Color[6] * 0.315392f * (3.0f * N.z * N.z - 1.0f) * A2
        + gSH9Color[7] * 1.092548f * N.x * N.z * A2
        + gSH9Color[8] * 0.546274f * (N.x * N.x - N.y * N.y) * A2;
    return irradiance;
}
