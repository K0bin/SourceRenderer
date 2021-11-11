#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.h"

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_lightmap_uv;
layout(location = 4) in float in_alpha;

layout(location = 0) out vec3 out_worldPosition;
layout(location = 1) out vec3 out_normal;
layout(location = 2) out vec2 out_uv;
layout(location = 3) out vec2 out_lightmap_uv;

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 0, std140) uniform CameraUbo {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
} camera;

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 3) uniform PerFrameUbo {
  mat4 swapchainTransform;
  vec2 jitterPoint;
  float zNear;
  float zFar;
  uvec2 rtSize;
  float clusterZBias;
  float clusterZScale;
  vec3 clusterCount;
  uint pointLightCount;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
};

void main(void) {
  vec4 pos = vec4(in_pos, 1);

  mat4 mvp = camera.viewProj * model;
  mat4 mv = camera.view * model;

  out_worldPosition = (model * pos).xyz;
  out_uv = in_uv;
  out_lightmap_uv = in_lightmap_uv;
  out_normal = normalize((model * vec4(in_normal, 0)).xyz);

  mat4 jitterMat;
  jitterMat[0] = vec4(1.0, 0.0, 0.0, 0.0);
  jitterMat[1] = vec4(0.0, 1.0, 0.0, 0.0);
  jitterMat[2] = vec4(0.0, 0.0, 1.0, 0.0);
  jitterMat[3] = vec4(jitterPoint.x, jitterPoint.y, 0.0, 1.0);
  mat4 swapchainMvp = swapchainTransform * mvp;
  mat4 jitterMvp = jitterMat * swapchainMvp;
  vec4 jitteredPoint = jitterMvp * pos;
  jitteredPoint.y = -jitteredPoint.y;
  gl_Position = jitteredPoint;
}
