#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec3 in_pos;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, std140) uniform CameraUBO {
  Camera camera;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
};

invariant gl_Position;

void main(void) {
  vec4 pos = vec4(in_pos, 1);
  mat4 mvp = camera.viewProj * model;
  gl_Position = mvp * pos;
}
