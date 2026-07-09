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
    layout(offset = 76) float roughness;
    float metalness;
    vec3 f0;
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

//layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler2D albedo;

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler2D transferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform samplerCube envMapDiffuse;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 3) uniform samplerCube envMapSpecular;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 4) uniform sampler2D integrationLUT;

vec3 approximateSpecularIBL(vec3 specularColor, float roughness, vec3 normal, vec3 viewDir) {
    float normalDotViewDir = clamp(dot(normal, viewDir), 0.0, 1.0);
    vec3 reflectionDir = 2.0 * dot(viewDir, normal) * normal - viewDir;
    vec3 prefilteredSpecular = textureLod(envMapSpecular, reflectionDir, float(textureQueryLevels(envMapSpecular)) * roughness).xyz;
    vec2 preintegrated = texture(integrationLUT, vec2(normalDotViewDir, roughness)).xy;
    return prefilteredSpecular * (specularColor * preintegrated.x + preintegrated.y);
}

void main(void) {
    float densityNormalized = in_density;

    vec3 albedo = texture(transferFunction, vec2(densityNormalized, 0.8 + 0.5 * 0.25)).rgb;
    albedo.r = mix(albedo.r, albedo.g, 0.3);

    float roughness = texture(transferFunction, vec2(min(densityNormalized, 0.9), 0.1 + 0.5 * 0.25)).r + 0.5;
    //roughness = 9999.0;

    vec3 radiance = vec3(0.0);

    // Direct lighting
    vec3 lightDir = normalize(-vec3(0.1, 1.0, 0.1));
    vec3 viewDir = normalize(camera.position.xyz - in_worldPosition.xyz);
    vec3 lightPower = vec3(5.0);
    radiance += pbr(lightDir, viewDir, in_normal.xyz, f0, albedo, lightPower, roughness, metalness);

    // Image based lighting (diffuse)
    vec3 rhoDiffuse = (1.0 - metalness) * albedo;
    rhoDiffuse *= vec3(1.0) - f0;
    radiance += rhoDiffuse * texture(envMapDiffuse, in_normal).rgb;

    // Image based lighting (specular)
    radiance += approximateSpecularIBL(f0, roughness, in_normal, viewDir);

    out_color.rgb = radiance;
    out_color.a = densityNormalized;

    //out_color = vec4(min(albedo, vec3(0.1) * albedo + pbr(lightDir, viewDir, in_normal, vec3(0.025), albedo, vec3(15.0), 0.1, 0.8) * 0.6), 1.0);
    //out_color.a = in_density;
    //out_color.rgb = in_normal * 0.5 + 0.5;

    //out_color.rgb = vec3(in_density) * 5.0;

    //out_color.rgb = in_normal.rgb * 0.5 + vec3(0.5);
    //out_color.rgb = in_normal.rgb;
}
