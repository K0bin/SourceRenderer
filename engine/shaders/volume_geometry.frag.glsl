#version 450
#extension GL_ARB_separate_shader_objects: enable
#extension GL_GOOGLE_include_directive: enable

#include "descriptor_sets.inc.glsl"
#include "pbr.inc.glsl"
#include "camera.inc.glsl"
#include "util.inc.glsl"
#include "frame_set_volume.inc.glsl"

layout (location = 0) in float in_density;
layout (location = 1) in vec3 in_worldPosition;
layout (location = 2) in vec3 in_densityMapUV;

layout (location = 0) out vec4 out_color;
layout (location = 1) out float out_sss_intensity;

layout(constant_id = 0) const bool rayMarchNormals = true;

layout (push_constant, std430) uniform Params {
    layout (offset = 96) mat4 invModel;
    vec3 f0;
    float roughness;
    float metalness;
    uint lod;
    float threshold;
    float width;
    float height;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform texture3D densityMap;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform texture2D albedoTransferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform texture2D roughnessTransferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 3) uniform texture2D metalnessTransferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 4) uniform textureCube envMapDiffuse;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 5) uniform textureCube envMapSpecular;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 6) uniform texture2D integrationLUT;


vec3 calculateGradient(vec3 densityMapUV, uint normalLod) {
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(textureSize(sampler3D(densityMap, samplerNearest), int(normalLod)));
    vec3 singlePixel = vec3(1.0) / imgSize;
    vec3 singlePixelX = vec3(singlePixel.x, 0, 0);
    vec3 singlePixelY = vec3(0, singlePixel.y, 0);
    vec3 singlePixelZ = vec3(0, 0, singlePixel.z);

    normal.x = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - singlePixelX, normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + singlePixelX, normalLod).x;
    normal.y = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - singlePixelY, normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + singlePixelY, normalLod).x;
    normal.z = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - singlePixelZ, normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + singlePixelZ, normalLod).x;
    return normal;
}

vec3 calculateNormal(vec3 densityMapUV, uint normalLod) {
    return normalize(calculateGradient(densityMapUV, normalLod));
}


// targetLod must be smaller (=> higher res) than the current lod in the push constants
vec4 rayMarchPositionInMip(vec3 startPosNormalized, uint targetLod) {
    uint meshLod = lod;
    // resolution of mip 0
    uvec3 texSize = textureSize(sampler3D(densityMap, samplerNearest), 0);
    // resolution of the higher res mip
    uvec3 targetTexSize = uvec3(texSize.x >> targetLod, texSize.y >> targetLod, texSize.z >> targetLod);
    // resolution of the lower res mip that was used to generate the mesh
    uvec3 geometryTexSize = uvec3(texSize.x >> meshLod, texSize.y >> meshLod, texSize.z >> meshLod);

    vec3 worldPos = camera.invView[3].xyz;
    vec3 modelPos = (invModel * vec4(worldPos, 1.0)).xyz;

    vec2 ndc = (gl_FragCoord.xy / vec2(width, height)) * 2.0 - 1.0;
    float viewX = ndc.x / camera.proj[0][0];
    float viewY = -ndc.y / camera.proj[1][1];
    vec4 viewRay = vec4(viewX, viewY, 1, 0);

    vec4 worldRay = camera.invView * viewRay;
    vec4 modelRay = invModel * worldRay;
    modelRay = normalize(modelRay / vec4(geometryTexSize, 1.0)); // Divide by geometryTexSize because of non-uniform scaling
    vec3 invRay = vec3(1.0 / modelRay.x, 1.0 / modelRay.y, 1.0 / modelRay.z);

    vec3 pos1TargetSpace = floor(startPosNormalized * geometryTexSize - 0.5) + 0.5;
    vec3 pos2TargetSpace = pos1TargetSpace + vec3(1.0);
    vec3 pos1Normalized = pos1TargetSpace / geometryTexSize;
    vec3 pos2Normalized = pos2TargetSpace / geometryTexSize;
    vec3 origin = modelPos.xyz / geometryTexSize;

    vec3 bbMin = min(pos1Normalized, pos2Normalized);
    vec3 bbMax = max(pos1Normalized, pos2Normalized);

    bbMin -= vec3(1.0) / geometryTexSize;
    bbMax += vec3(1.0) / geometryTexSize;

    vec3 t1 = (bbMin - origin) * invRay;
    vec3 t2 = (bbMax - origin) * invRay;

    vec3 tMin = min(t1, t2);
    vec3 tMax = max(t1, t2);

    float tEnter = max(tMin.x, max(tMin.y, tMin.z));
    float tExit = min(tMax.x, min(tMax.y, tMax.z));

    // Calculate intersections with texture box
    vec3 tTex1 = (vec3(0) - origin) * invRay;
    vec3 tTex2 = (vec3(1.0) - origin) * invRay;

    vec3 tTexMin = min(tTex1, tTex2);
    vec3 tTexMax = max(tTex1, tTex2);

    float tTexEnter = max(tTexMin.x, max(tTexMin.y, tTexMin.z));
    float tTexExit = min(tTexMax.x, min(tTexMax.y, tTexMax.z));

    tEnter = max(tEnter, tTexEnter);
    tExit = min(tExit, tTexExit);

    // tEnter must be <= tExit
    // tExit must be >= 0
    if (tExit < 0.0 || tEnter > tExit)
    return vec4(0.0);

    float stepLen = 1.0 / length(targetTexSize);
    float t = tEnter;

    while (t <= tExit) {
        vec3 pos = origin + t * modelRay.xyz;
        float density = textureLod(sampler3D(densityMap, samplerNearest), pos, int(targetLod)).x;
        if (density >= threshold)
        return vec4(pos, density);

        t += stepLen;
    }

    return vec4(0.0);
}

vec3 approximateSpecularIBL(vec3 specularColor, float roughness, vec3 normal, vec3 viewDir) {
    float normalDotViewDir = clamp(dot(normal, viewDir), 0.0, 1.0);
    vec3 reflectionDir = 2.0 * dot(viewDir, normal) * normal - viewDir;
    vec3 prefilteredSpecular = textureLod(samplerCube(envMapSpecular, samplerLinear), reflectionDir, float(textureQueryLevels(samplerCube(envMapSpecular, samplerLinear))) * roughness).xyz;
    vec2 preintegrated = textureLod(sampler2D(integrationLUT, samplerLinear), vec2(normalDotViewDir, roughness), 0).xy;
    return prefilteredSpecular * (specularColor * preintegrated.x + preintegrated.y);
}

vec4 shadeFragment(float densityNormalized, vec3 worldPosition, vec3 normal, out float sssIntensity) {
    vec4 albedoAndAlpha = texture(sampler2D(albedoTransferFunction, samplerLinear), vec2(densityNormalized, 0.0));
    vec3 albedo = albedoAndAlpha.rgb;
    float alpha = albedoAndAlpha.a;
    float roughness = texture(sampler2D(roughnessTransferFunction, samplerLinear), vec2(densityNormalized, 0.0)).r;
    float metalness = texture(sampler2D(metalnessTransferFunction, samplerLinear), vec2(densityNormalized, 0.0)).r;

    vec3 radiance = vec3(0.0);

    // Direct lighting
    vec3 lightDir = normalize(-vec3(0.1, 1.0, 0.3));
    vec3 viewDir = normalize(camera.position.xyz - worldPosition);
    vec3 lightPower = vec3(0.5);
    radiance += pbr(lightDir, viewDir, normal.xyz, f0, albedo, lightPower, roughness, metalness);

    // Image based lighting (diffuse)
    vec3 rhoDiffuse = (1.0 - metalness) * albedo;
    rhoDiffuse *= vec3(1.0) - f0;
    radiance += rhoDiffuse * texture(samplerCube(envMapDiffuse, samplerLinear), normal).rgb;

    // Image based lighting (specular)
    radiance += approximateSpecularIBL(f0, roughness, normal, viewDir);

    vec4 color = vec4(0.0);
    color.rgb = radiance;
    // TODO use transferFunction texture to get SSS intensity
    //color.a = clamp((1.0 - densityNormalized) * 0.33, 0.0, 1.0);

    //color = vec4(min(albedo, vec3(0.1) * albedo + pbr(lightDir, viewDir, normal, vec3(0.025), albedo, vec3(15.0), 0.1, 0.8) * 0.6), 1.0);
    //color.a = in_density;
    //color.rgb = normal * 0.5 + 0.5;

    //color.rgb = vec3(in_density) * 5.0;

    //color.rgb = normal.rgb * 0.5 + vec3(0.5);
    //color.rgb = normal.rgb;

    color.a = alpha;
    sssIntensity = clamp((1.0 - densityNormalized) * 0.33, 0.0, 1.0);

    return color;
}

void main(void) {
    uint normalLod = 0u;

    vec3 normal;
    float density;
    if ((normalLod != lod && rayMarchNormals)) {
        vec4 normalLookUpNormalizedAndDensity = rayMarchPositionInMip(in_densityMapUV, normalLod);
        if (normalLookUpNormalizedAndDensity.w >= threshold) {
            normal = calculateNormal(normalLookUpNormalizedAndDensity.xyz, normalLod);
            density = normalLookUpNormalizedAndDensity.w;
        } else {
            normal = calculateNormal(in_densityMapUV, lod);
            density = in_density;
        }
    } else {
        normal = calculateNormal(in_densityMapUV, lod);
        density = in_density;
    }

    mat4 normalModelMat = transpose(invModel);
    normal = (normalModelMat * vec4(normal, 0.0)).xyz;
    normal = normalize(normal);

    out_color = shadeFragment(in_density, in_worldPosition, normal, out_sss_intensity);

    //out_color.xyz = normal * 0.5 + vec3(0.5);
    //out_color.a = 1.0;
}
