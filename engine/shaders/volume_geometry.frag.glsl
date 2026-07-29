#version 450
#extension GL_ARB_separate_shader_objects: enable
#extension GL_GOOGLE_include_directive: enable

#include "descriptor_sets.inc.glsl"
#include "pbr.inc.glsl"
#include "camera.inc.glsl"

layout (location = 0) in vec3 in_normal;
layout (location = 1) in float in_density;
layout (location = 2) in vec3 in_worldPosition;

layout (location = 0) out vec4 out_color;

layout(push_constant, std430) uniform Params {
    layout(offset = 144) float roughness;
    float metalness;
    float _padding;
    float _padding1;
    vec3 f0;
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler2D transferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform samplerCube envMapDiffuse;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 3) uniform samplerCube envMapSpecular;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 4) uniform sampler2D integrationLUT;

#include "volume_shading.inc.glsl"

void main(void) {
    out_color = shadeFragment(in_density, in_worldPosition, in_normal);
}
