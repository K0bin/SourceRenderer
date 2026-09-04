#version 450
#extension GL_ARB_separate_shader_objects: enable
#extension GL_GOOGLE_include_directive: enable

#include "descriptor_sets.inc.glsl"

layout(location = 0) in vec2 in_uv;
layout(location = 1) in vec4 in_color;

layout(location = 0) out vec4 out_color;

layout (set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform texture2D texture;
layout (set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler samp;

void main(void) {
    vec4 color = in_color;
    color *= textureLod(sampler2D(texture, samp), in_uv, 0);
    out_color = color;
}
