#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) out vec3 out_normal;
layout(location = 1) out float out_density;
layout(location = 2) out vec3 out_worldPosition;

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
  vec3 scale;
  float threshold;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler3D densityMap;


vec4 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    vec3 imgSize = vec3(textureSize(densityMap, 0));

    float value1 = textureLod(densityMap, (vec3(pos1) + vec3(0.5)) / imgSize, 0u).x;
    float value2 = textureLod(densityMap, (vec3(pos2) + vec3(0.5)) / imgSize, 0u).x;
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec4(vec3(pos1), value1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec4(vec3(pos2), value2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return mix(vec4(pos1, value1), vec4(pos2, value2), a);
}

vec3 getVertexPosFromKey(uint vtxKey) {

    return vec3(0.0);
}

vec4 vertexPosFromKey(uint vertexKey, ivec3 densityMapSize) {
    uvec3 sizes = uvec3(densityMapSize);

    // Round up. The key is calculated with NumWorkgroups * WorkgroupSize.
    // WorkgroupSize is 4,4,4; NumWorkgroups gets rounded up.
    sizes = ((sizes + uvec3(3u)) / uvec3(4u)) * uvec3(4u);

    sizes *= 2;

    uvec3 pos = uvec3(vertexKey % sizes.x,
        (vertexKey / sizes.x) % sizes.y,
        vertexKey / (sizes.x * sizes.y));

    uvec3 pos1 = pos / 2u;
    uvec3 pos2 = pos1 + (pos % 2u);

    return interpolateVertices(pos1, pos2) * vec4(scale, 1.0);
}


vec3 calculateNormal(vec3 pos) {
    vec3 imgPos = pos / scale;
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(textureSize(densityMap, 0));
    vec3 singlePixel = vec3(1.0) / imgSize;
    vec3 halfPixel = singlePixel * 0.5;
    vec3 texPos = imgPos / imgSize + halfPixel;

    normal.x = textureLod(densityMap, texPos - vec3(singlePixel.x, 0, 0), 0u).x
                                - textureLod(densityMap, texPos + vec3(singlePixel.x, 0, 0), 0u).x;
    normal.y = textureLod(densityMap, texPos - vec3(0, singlePixel.y, 0), 0u).x
                                - textureLod(densityMap, texPos + vec3(0, singlePixel.y, 0), 0u).x;
    normal.z = textureLod(densityMap, texPos - vec3(0, 0, singlePixel.z), 0u).x
                                - textureLod(densityMap, texPos + vec3(0, 0, singlePixel.z), 0u).x;
    return normalize(normal);
}


void main(void) {
  vec3 densityMapSize = textureSize(densityMap, 0);
  uint vtxkey = gl_VertexIndex;

  vec3 pos = vertexPosFromKey(vtxkey, ivec3(densityMapSize)).xyz;
  vec3 normal = calculateNormal(pos);

  mat4 inverseModelMat = inverse(model);
  mat4 normalModelMat = transpose(inverseModelMat);
  out_normal = normalize(normalModelMat * vec4(normal, 0)).xyz;

  out_density = texture(densityMap, (pos / scale + 0.5) / densityMapSize).x;

  vec4 rayDir = vec4(0.0, 0.0, 1.0, 0.0);
  rayDir = (inverseModelMat * camera.invView) * rayDir;
  rayDir = normalize(rayDir);

  out_density = max(out_density, texture(densityMap, (pos / scale + 0.5 + rayDir.xyz) / densityMapSize).x);
  //out_density = max(out_density, texture(densityMap, (pos / scale + 0.5 - in_normal.xyz) / densityMapSize).x);

  mat4 mvp = camera.viewProj * model;
  gl_Position = mvp * vec4(pos, 1.0);
  out_worldPosition = gl_Position.xyz;
}
