#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
  float threshold;
  uint lod;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler3D densityMap;


vec4 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    vec3 imgSize = vec3(textureSize(densityMap, int(lod)));

    float value1 = textureLod(densityMap, (vec3(pos1) + vec3(0.5)) / imgSize, lod).x;
    float value2 = textureLod(densityMap, (vec3(pos2) + vec3(0.5)) / imgSize, lod).x;
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec4(vec3(pos1), value1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec4(vec3(pos2), value2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return mix(vec4(pos1, value1), vec4(pos2, value2), a);
}


vec4 vertexPosFromKey(uint vertexKey) {
    uvec3 sizes = uvec3(512 * 2);

    uvec3 pos = uvec3(vertexKey % sizes.x,
        (vertexKey / sizes.x) % sizes.y,
        vertexKey / (sizes.x * sizes.y));

    uvec3 pos1 = pos / 2u;
    uvec3 pos2 = pos1 + (pos % 2u);

    float lodScale = exp2(lod);
    return interpolateVertices(pos1, pos2);
}


void main(void) {
    uint vtxkey = gl_VertexIndex;

    vec4 posAndDensity = vertexPosFromKey(vtxkey);
    vec3 pos = posAndDensity.xyz;

    float lodScale = exp2(lod);
    pos *= vec3(lodScale);

    mat4 mvp = camera.viewProj * model;
    gl_Position = mvp * vec4(pos, 1.0);
}
