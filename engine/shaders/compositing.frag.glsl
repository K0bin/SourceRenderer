#version 450
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "frame_set_volume.inc.glsl"

layout(location = 0) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform texture2D color;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform texture2D ssao;

void main(void) {
    vec4 withSSAO = textureLod(sampler2D(color, samplerLinear), in_uv, 0) * vec4(textureLod(sampler2D(ssao, samplerLinear), in_uv, 0).x);
    out_color = vec4(pow(withSSAO.rgb, vec3(1.0 / 2.2)), 1.0);
}
