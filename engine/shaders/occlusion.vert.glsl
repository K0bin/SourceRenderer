#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.h"

layout(location = 0) in vec3 in_pos;

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 0, std140) uniform CameraUbo {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
} camera;

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
};

invariant gl_Position;

void main(void) {
  vec4 pos = vec4(in_pos, 1);
  mat4 mvp = camera.viewProj * model;
  gl_Position = mvp * pos;
  gl_Position.y = -gl_Position.y;
}
