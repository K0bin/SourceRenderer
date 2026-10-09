#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"
#include "frame_set_volume.inc.glsl"

layout(location = 0) out float out_density;
layout(location = 1) out vec3 out_worldPosition;
layout(location = 2) out vec3 out_densityMapUV;

layout(push_constant) uniform VeryHighFrequencyUbo {
    mat4 model;
//mat4 invModel;
    uvec3 lodExtents;
    float threshold;
    uint lod;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform texture3D densityMap;


vec4 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    vec3 fpos1 = vec3(pos1) + 0.5;
    vec3 fpos2 = vec3(pos2) + 0.5;
    float value1 = texelFetch(sampler3D(densityMap, samplerNearest), ivec3(pos1), int(lod)).x;
    float value2 = texelFetch(sampler3D(densityMap, samplerNearest), ivec3(pos2), int(lod)).x;
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec4(fpos1, value1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec4(fpos2, value2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return mix(vec4(fpos1, value1), vec4(fpos2, value2), a);
    //return (vec4(fpos1, value1) + vec4(fpos2, value2)) / 2.0; // debug with simple average
}


vec4 vertexPosFromKey(uint vertexKey) {
    uvec3 sizes = uvec3(512u * 2u + 1u);

    uvec3 pos = uvec3(vertexKey % sizes.x,
            (vertexKey / sizes.x) % sizes.y,
            vertexKey / (sizes.x * sizes.y));

    uvec3 pos1 = pos / 2u;
    uvec3 pos2 = pos1 + (pos % 2u);

    return interpolateVertices(pos1, pos2);
}

vec3 calculateNormal(vec3 densityMapUV, uint normalLod) {
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(lodExtents);
    vec3 singlePixel = vec3(1.0) / imgSize;

    normal.x = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - vec3(singlePixel.x, 0, 0), normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + vec3(singlePixel.x, 0, 0), normalLod).x;
    normal.y = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - vec3(0, singlePixel.y, 0), normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + vec3(0, singlePixel.y, 0), normalLod).x;
    normal.z = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - vec3(0, 0, singlePixel.z), normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + vec3(0, 0, singlePixel.z), normalLod).x;
    return normalize(normal);
}


void main(void) {
    vec3 densityMapSize = vec3(lodExtents);
    uint vtxkey = gl_VertexIndex;

    vec4 posAndDensity = vertexPosFromKey(vtxkey);
    vec3 pos = posAndDensity.xyz;
    float density = posAndDensity.w;

    vec3 densityMapPosition = pos / densityMapSize;
    out_densityMapUV = densityMapPosition;
    vec3 normal = calculateNormal(densityMapPosition, lod);

    out_density = density;

    mat4 mvp = camera.viewProj * model;
    gl_Position = mvp * vec4(pos, 1.0);
    out_worldPosition = (model * vec4(pos, 1.0)).xyz;
}
