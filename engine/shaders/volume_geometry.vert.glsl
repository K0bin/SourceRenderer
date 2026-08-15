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
  mat4 invModel;
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

    return interpolateVertices(pos1, pos2);
}

vec3 calculateNormal(vec3 densityMapUV, uint normalLod) {
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(textureSize(densityMap, int(normalLod)));
    vec3 singlePixel = vec3(1.0) / imgSize;

    normal.x = textureLod(densityMap, densityMapUV - vec3(singlePixel.x, 0, 0), normalLod).x
                                - textureLod(densityMap, densityMapUV + vec3(singlePixel.x, 0, 0), normalLod).x;
    normal.y = textureLod(densityMap, densityMapUV - vec3(0, singlePixel.y, 0), normalLod).x
                                - textureLod(densityMap, densityMapUV + vec3(0, singlePixel.y, 0), normalLod).x;
    normal.z = textureLod(densityMap, densityMapUV - vec3(0, 0, singlePixel.z), normalLod).x
                                - textureLod(densityMap, densityMapUV + vec3(0, 0, singlePixel.z), normalLod).x;
    return normalize(normal);
}


void main(void) {
  vec3 densityMapSize = textureSize(densityMap, int(lod));
  uint vtxkey = gl_VertexIndex;

  vec4 posAndDensity = vertexPosFromKey(vtxkey);
  vec3 pos = posAndDensity.xyz;
  float density = posAndDensity.w;

  vec3 densityMapPosition = (pos + 0.5) / densityMapSize;
  vec3 normal = calculateNormal(densityMapPosition, lod);

  mat4 normalModelMat = transpose(invModel);
  out_normal = normalize(normalModelMat * vec4(normal, 0)).xyz;

  out_density = density;

  vec4 rayDir = vec4(0.0, 0.0, 1.0, 0.0);
  rayDir = (invModel * camera.invView) * rayDir;
  rayDir = normalize(rayDir);

  out_density = max(out_density, textureLod(densityMap, (pos + 0.5 + rayDir.xyz) / densityMapSize, lod).x);

  float lodScale = exp2(lod);
  pos *= vec3(lodScale);

  mat4 mvp = camera.viewProj * model;
  gl_Position = mvp * vec4(pos, 1.0);
  out_worldPosition = (model * vec4(pos, 1.0)).xyz;
}
