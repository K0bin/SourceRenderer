#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec3 in_oldPosition;

layout(location = 0) out vec4 out_normal;
layout(location = 1) out vec4 out_motion;

void main(void) {
  out_normal = vec4(in_normal, 0);
  out_motion = vec4(in_position - in_oldPosition, 0);
}
