#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"

layout(location = 0) in vec2 in_pos;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec4 in_color;

layout(location = 0) out vec2 out_uv;
layout(location = 1) out vec4 out_color;

layout(push_constant) uniform VeryHighFrequencyUbo {
    mat4 transform;
};

void main(void) {
  gl_Position = transform * vec4(in_pos, 0.0, 1.0);
  out_uv = in_uv;
  out_color = in_color;
}
