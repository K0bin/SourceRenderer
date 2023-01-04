#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

layout(location = 0) in vec3 in_pos;

layout(location = 0) out flat uint out_drawIndex;
layout(location = 1) out flat uint out_firstIndex;

#include "frame_set.inc.glsl"

invariant gl_Position;

void main(void) {
  vec4 pos = vec4(in_pos, 1);

  uint drawIndex = gl_InstanceIndex;
  GPUDraw draw = scene_draws[drawIndex];
  GPUMeshPart part = scene_parts[draw.partIndex];
  uint materialIndex = part.materialIndex;
  uint drawableIndex = draw.drawableIndex;
  GPUDrawable drawable = scene_drawables[drawableIndex];
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
