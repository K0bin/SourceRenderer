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

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

//layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler2D albedo;

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler2D transferFunction;

void main(void) {
    float colorComponent = min((in_density / 0.15) * 6.0 - 1.5, 0.7);
    vec3 albedo = vec3(0.7, colorComponent, colorComponent);
    vec3 lightDir = normalize(vec3(0.1, 1.0, 0.1));
    vec3 viewDir = normalize(camera.position.xyz - in_worldPosition.xyz);
    out_color = vec4(vec3(0.4) * albedo + pbr(lightDir, viewDir, in_normal, vec3(0.025), albedo, vec3(15.0), 0.1, 0.8) * 0.6, 1.0);
}
