#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec3 in_pos;

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 0, std140) uniform CameraUBO {
  Camera camera;
};

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 1) uniform PerFrameUbo {
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

layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 2, std430) readonly buffer sceneBuffer {
  GPUScene scene;
};

layout(location = 0) out flat uint out_drawIndex;
layout(location = 1) out flat uint out_firstIndex;

invariant gl_Position;

void main(void) {
  vec4 pos = vec4(in_pos, 1);

  uint drawIndex = gl_InstanceIndex;
  GPUDraw draw = scene.draws[drawIndex];
  GPUMeshPart part = scene.parts[draw.partIndex];
  uint materialIndex = part.materialIndex;
  uint drawableIndex = draw.drawableIndex;
  GPUDrawable drawable = scene.drawables[drawableIndex];
  mat4 model = drawable.transform;

  out_drawIndex = drawIndex;
  out_firstIndex = part.meshFirstIndex;

  mat4 mvp = camera.viewProj * model;
  mat4 mv = camera.view * model;

  mat4 jitterMat;
  jitterMat[0] = vec4(1.0, 0.0, 0.0, 0.0);
  jitterMat[1] = vec4(0.0, 1.0, 0.0, 0.0);
  jitterMat[2] = vec4(0.0, 0.0, 1.0, 0.0);
  jitterMat[3] = vec4(jitterPoint.x, jitterPoint.y, 0.0, 1.0);
  mat4 swapchainMvp = swapchainTransform * mvp;
  mat4 jitterMvp = jitterMat * swapchainMvp;
  vec4 jitteredPoint = jitterMvp * pos;
  gl_Position = jitteredPoint;
}
