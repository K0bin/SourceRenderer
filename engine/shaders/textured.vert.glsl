#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec3 in_color;
layout(location = 3) in vec2 in_uv;

layout(location = 0) out vec3 out_normal;
layout(location = 1) out vec3 out_color;
layout(location = 2) out vec2 out_uv;

layout(set = 2, binding = 0) uniform LowFrequencyUbo {
    mat4 viewProjection;
};

layout(set = 0, binding = 0) uniform HighFrequencyUbo {
    mat4 model;
};

void main(void) {
  out_color = in_color;
  out_uv = in_uv;
  out_normal = in_normal;
  gl_Position = (viewProjection * model) * vec4(in_pos, 1);
  gl_Position.y = -gl_Position.y;
}
