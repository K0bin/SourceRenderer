#version 450
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "frame_set_volume.inc.glsl"

layout(location = 0) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform texture2D color;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform texture2D ssao;

vec3 linearToSRGB(vec3 color) {
    vec3 higher = 1.055 * pow(color, vec3(1.0 / 2.4)) - 0.055;
    vec3 lower  = color * 12.92;
    return mix(lower, higher, vec3(greaterThan(color, vec3(0.0031308))));
}

void main(void) {
    vec4 withSSAO = textureLod(sampler2D(color, samplerLinear), in_uv, 0) * vec4(textureLod(sampler2D(ssao, samplerLinear), in_uv, 0).x);

    // We need to do color conversion manually here because DearImgui expects rendering to a linear image
    // and it renders to the swapchain image.
    // Easier to do the color conversion here than to do another copy of the DearImgui contents or convert those
    out_color = vec4(linearToSRGB(withSSAO.rgb), 1.0);
}
