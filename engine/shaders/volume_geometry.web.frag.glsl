#version 450
#extension GL_ARB_separate_shader_objects: enable
#extension GL_GOOGLE_include_directive: enable

#include "descriptor_sets.inc.glsl"

const float PI = 3.14159265359;

layout (location = 0) in vec3 in_normal;
layout (location = 1) in float in_density;

layout (location = 0) out vec4 out_color;

//layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler2D albedo;

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler2D transferFunction;

void main(void) {
    //out_color = texture(albedo, in_uv);
    //out_color = vec4(0.8, in_density * 2.0 / 0.14, in_density * 2.0 / 0.14, 1.0);
    out_color = texture(transferFunction, vec2(0.35 + in_density, 0.85));
    out_color.a = texture(transferFunction, vec2(0.35 + in_density, 0.65)).x;
}
