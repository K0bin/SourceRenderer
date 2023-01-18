#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"

layout(location = 0) in vec3 in_pos;

layout(push_constant) uniform VeryHighFrequencyUbo {
    mat4 viewProj;
    mat4 model;
};

invariant gl_Position;

void main(void) {
    vec4 pos = vec4(in_pos, 1);
    mat4 mvp = viewProj * model;
    gl_Position = mvp * pos;
}
