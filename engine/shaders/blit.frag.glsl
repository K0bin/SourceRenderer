#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform texture2D tex;
layout(set = 0, binding = 1) uniform sampler samplerLinear;

void main(void) {
    out_color = texture(sampler2D(tex, samplerLinear), in_uv);
}
