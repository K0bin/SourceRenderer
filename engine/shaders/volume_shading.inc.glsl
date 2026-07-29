#ifndef VOLUME_SHADING_H

#define VOLUME_SHADING_H

#ifndef TRANSFER_FUNCTION_IMG_NAME
#define TRANSFER_FUNCTION_IMG_NAME transferFunction
#endif

#ifndef TRANSFER_FUNCTION_IMG_NAME
#define TRANSFER_FUNCTION_IMG_NAME transferFunction
#endif

#ifndef ENV_MAP_SPECULAR_IMG_NAME
#define ENV_MAP_SPECULAR_IMG_NAME envMapSpecular
#endif

#ifndef INTEGRATION_LUT_IMG_NAME
#define INTEGRATION_LUT_IMG_NAME integrationLUT
#endif

#ifndef ENV_MAP_DIFFUSE_IMG_NAME
#define ENV_MAP_DIFFUSE_IMG_NAME envMapDiffuse
#endif

#include "pbr.inc.glsl"
#include "camera.inc.glsl"

vec3 approximateSpecularIBL(vec3 specularColor, float roughness, vec3 normal, vec3 viewDir) {
    float normalDotViewDir = clamp(dot(normal, viewDir), 0.0, 1.0);
    vec3 reflectionDir = 2.0 * dot(viewDir, normal) * normal - viewDir;
    vec3 prefilteredSpecular = textureLod(ENV_MAP_SPECULAR_IMG_NAME, reflectionDir, float(textureQueryLevels(ENV_MAP_SPECULAR_IMG_NAME)) * roughness).xyz;
    vec2 preintegrated = textureLod(INTEGRATION_LUT_IMG_NAME, vec2(normalDotViewDir, roughness), 0).xy;
    return prefilteredSpecular * (specularColor * preintegrated.x + preintegrated.y);
}

vec4 shadeFragment(float densityNormalized, vec3 worldPosition, vec3 normal) {
    vec3 albedo = texture(TRANSFER_FUNCTION_IMG_NAME, vec2(densityNormalized, 0.8 + 0.5 * 0.25)).rgb;
    albedo.r = mix(albedo.r, albedo.g, 0.3);

    float roughness = texture(TRANSFER_FUNCTION_IMG_NAME, vec2(min(densityNormalized, 0.9), 0.1 + 0.5 * 0.25)).r + 0.5;
    //roughness = 9999.0;

    vec3 radiance = vec3(0.0);

    // Direct lighting
    vec3 lightDir = normalize(-vec3(0.1, 1.0, 0.3));
    vec3 viewDir = normalize(camera.position.xyz - worldPosition);
    vec3 lightPower = vec3(5.0);
    radiance += pbr(lightDir, viewDir, normal.xyz, f0, albedo, lightPower, roughness, metalness);

    // Image based lighting (diffuse)
    vec3 rhoDiffuse = (1.0 - metalness) * albedo;
    rhoDiffuse *= vec3(1.0) - f0;
    radiance += rhoDiffuse * texture(ENV_MAP_DIFFUSE_IMG_NAME, normal).rgb;

    // Image based lighting (specular)
    radiance += approximateSpecularIBL(f0, roughness, normal, viewDir);

    vec4 color = vec4(0.0);
    color.rgb = radiance;
    // TODO use transferFunction texture to get SSS intensity
    color.a = clamp((1.0 - densityNormalized) * 0.33, 0.0, 1.0);

    //color = vec4(min(albedo, vec3(0.1) * albedo + pbr(lightDir, viewDir, normal, vec3(0.025), albedo, vec3(15.0), 0.1, 0.8) * 0.6), 1.0);
    //color.a = in_density;
    //color.rgb = normal * 0.5 + 0.5;

    //color.rgb = vec3(in_density) * 5.0;

    color.rgb = normal.rgb * 0.5 + vec3(0.5);
    //color.rgb = normal.rgb;

    return color;
}

#endif
