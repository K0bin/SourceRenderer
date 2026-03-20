#version 450
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"

layout(location = 0) in vec2 in_uv;

layout(location = 0) out vec4 out_color;


layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform sampler2D color;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler2D ssao;

void main(void) {
    out_color = textureLod(color, in_uv, 0) * vec4(textureLod(ssao, in_uv, 0).x);
}