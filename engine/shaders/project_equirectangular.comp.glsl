#version 460

// Adapted from:
// Physically Based Rendering
// Copyright (c) 2017-2018 Michał Siejak
// MIT Licensed

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable

#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#define CS
#include "descriptor_sets.inc.glsl"
#include "util.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform sampler2D envMap;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1, rgba8) uniform coherent writeonly imageCube outputTexture;

const float PI = 3.141592;
const float TwoPI = 2 * PI;

// Calculate normalized sampling direction vector based on current fragment coordinates (gl_GlobalInvocationID.xyz).
// This is essentially "inverse-sampling": we reconstruct what the sampling vector would be if we wanted it to "hit"
// this particular fragment in a cubemap.
// See: OpenGL core profile specs, section 8.13.
vec3 getSamplingVector()
{
    vec2 st = gl_GlobalInvocationID.xy/vec2(imageSize(outputTexture));
    vec2 uv = 2.0 * vec2(st.x, 1.0-st.y) - vec2(1.0);

    vec3 ret;
    switch (gl_GlobalInvocationID.z) {
        case 0: ret = vec3(1.0,  uv.y, -uv.x); break;
        case 1: ret = vec3(-1.0, uv.y,  uv.x); break;
        case 2: ret = vec3(uv.x, 1.0, -uv.y); break;
        case 3: ret = vec3(uv.x, -1.0, uv.y); break;
        case 4: ret = vec3(uv.x, uv.y, 1.0); break;
        case 5: ret = vec3(-uv.x, uv.y, -1.0); break;
    }
    return normalize(ret);
}

vec2 directionToSphericalEnvmap(vec3 dir) {
    // Convert Cartesian direction vector to spherical coordinates.
    float phi   = atan(dir.z, dir.x);
    float theta = acos(dir.y);
    return vec2(0.5 - phi/TwoPI, theta/PI);
}

void main(void) {
    vec3 v = getSamplingVector();

    vec2 sphericalUV = directionToSphericalEnvmap(v);

    // Sample equirectangular texture.
    vec4 color = texture(envMap, sphericalUV);

    // Write out color to output cubemap.
    imageStore(outputTexture, ivec3(gl_GlobalInvocationID), color);
}
