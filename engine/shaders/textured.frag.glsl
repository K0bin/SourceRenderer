#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_color;
layout(location = 1) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D tex;
/*layout(set = 1, binding = 1) uniform HighFrequencyUbo {
    mat4 model;
};*/

void main(void) {
  out_color = vec4(in_color, 1.0) * texture(tex, in_uv);
}
