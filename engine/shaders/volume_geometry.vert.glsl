#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec3 in_pos;
//layout(location = 1) in vec3 in_normal;
//layout(location = 3) in vec2 in_lightmap_uv;
//layout(location = 4) in float in_alpha;

layout(location = 0) out vec3 out_normal;
layout(location = 1) out float out_density;
layout(location = 2) out vec3 out_worldPosition;

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
  vec3 size;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler3D densityMap;


vec3 calculateNormal(vec3 pos, vec3 densityMapSize) {
    vec3 imgPos = pos / size + 0.5;
    vec3 normal = vec3(0.0);
    normal.x = (texture(densityMap, (imgPos - vec3(1, 0, 0)) / densityMapSize)
                                - texture(densityMap, (imgPos + vec3(1, 0, 0)) / densityMapSize)).x;
    normal.y = (texture(densityMap, (imgPos - vec3(0, 1, 0)) / densityMapSize)
                                - texture(densityMap, (imgPos + vec3(0, 1, 0)) / densityMapSize)).x;
    normal.z = (texture(densityMap, (imgPos - vec3(0, 0, 1)) / densityMapSize)
                                - texture(densityMap, (imgPos + vec3(0, 0, 1)) / densityMapSize)).x;
    return normalize(normal);
}

void main(void) {
  vec3 densityMapSize = textureSize(densityMap, 0);
  vec3 normal = calculateNormal(in_pos, densityMapSize);

  mat4 inverseModelMat = inverse(model);
  mat4 normalModelMat = transpose(inverseModelMat);
  out_normal = normalize(normalModelMat * vec4(normal, 0)).xyz;

  out_density = texture(densityMap, (in_pos / size + 0.5) / densityMapSize).x;

  vec4 rayDir = vec4(0.0, 0.0, 1.0, 0.0);
  rayDir = (inverseModelMat * camera.invView) * rayDir;
  rayDir = normalize(rayDir);

  out_density = max(out_density, texture(densityMap, (in_pos / size + 0.5 + rayDir.xyz) / densityMapSize).x);
  //out_density = max(out_density, texture(densityMap, (in_pos / size + 0.5 - in_normal.xyz) / densityMapSize).x);

  mat4 mvp = camera.viewProj * model;
  gl_Position = mvp * vec4(in_pos, 1.0);
  out_worldPosition = gl_Position.xyz;
}
