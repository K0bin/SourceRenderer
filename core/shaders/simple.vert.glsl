#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_color;

layout(location = 0) out vec3 out_color;

void main(void) {
  out_color = in_color;
  gl_Position = vec4(in_pos, 1);
}
