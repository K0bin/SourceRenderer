#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_normal;
layout(location = 1) in vec3 in_color;
layout(location = 2) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

layout(set = 1, binding = 0) uniform sampler2D tex;

void main(void) {
  vec4 lightDir = normalize(vec4(0.1, -1, 0.1, 0));
  out_color = vec4(in_color, 1.0) * texture(tex, in_uv);
}
