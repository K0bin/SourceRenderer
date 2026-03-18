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
    //out_color = texture(albedo, in_uv);
    //float density = 0.5 - 0.8 * cos(3.1425 * in_density / 0.15);
    float density = min((in_density / 0.15) * 6.0 - 1.5, 0.8);
    vec3 albedo = vec3(0.8, density, density);
    //albedo = texture(transferFunction, vec2(max(0.0, in_density - 0.045) / 0.1, 0.75));
    vec3 lightDir = normalize(vec3(0.1, 1.0, 0.1));
    vec3 viewDir = normalize(camera.position.xyz - in_worldPosition.xyz);
    out_color = vec4(vec3(0.1) + pbr(lightDir, viewDir, in_normal, vec3(0.025), albedo, vec3(15.0), 0.1, 0.8), 1.0);
}
