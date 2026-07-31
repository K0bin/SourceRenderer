#version 450
#extension GL_ARB_separate_shader_objects: enable
#extension GL_GOOGLE_include_directive: enable

#include "descriptor_sets.inc.glsl"
#include "pbr.inc.glsl"
#include "camera.inc.glsl"

layout (location = 0) in vec3 in_normal;
layout (location = 1) in float in_density;
layout (location = 2) in vec3 in_worldPosition;
layout (location = 3) in vec3 in_densityMapUV;

layout (location = 0) out vec4 out_color;

layout(push_constant, std430) uniform Params {
    layout(offset = 144) float roughness;
    float metalness;
    float width;
    float height;
    mat4 invModel;
    vec3 f0;
    uint lod;
    float threshold;
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler3D densityMap;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler2D transferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform samplerCube envMapDiffuse;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 3) uniform samplerCube envMapSpecular;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 4) uniform sampler2D integrationLUT;


vec3 calculateNormal(vec3 densityMapUV, uint normalLod) {
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(textureSize(densityMap, int(normalLod)));
    vec3 singlePixel = vec3(1.0) / imgSize;

    normal.x = textureLod(densityMap, densityMapUV - vec3(singlePixel.x, 0, 0), normalLod).x
                                - textureLod(densityMap, densityMapUV + vec3(singlePixel.x, 0, 0), normalLod).x;
    normal.y = textureLod(densityMap, densityMapUV - vec3(0, singlePixel.y, 0), normalLod).x
                                - textureLod(densityMap, densityMapUV + vec3(0, singlePixel.y, 0), normalLod).x;
    normal.z = textureLod(densityMap, densityMapUV - vec3(0, 0, singlePixel.z), normalLod).x
                                - textureLod(densityMap, densityMapUV + vec3(0, 0, singlePixel.z), normalLod).x;
    return normalize(normal);
}


// targetLod must be smaller (=> higher res) than the current lod in the push constants
vec3 rayMarchPositionInMip(vec3 startPosNormalized, uint targetLod) {
    vec4 ndcPos = vec4((gl_FragCoord.x / width) * 2.0 - 1.0, (gl_FragCoord.y / height) * -2.0 + 1.0, gl_FragCoord.z, 1.0);

    vec4 worldPos = camera.invViewProj * ndcPos;
    worldPos /= worldPos.w;
    vec4 modelPos = invModel * worldPos;
    modelPos.xyz *= exp2(lod);

    vec4 viewRay = vec4(0,0,1,0);
    vec4 worldRay = camera.invView * viewRay;
    vec4 modelRay = invModel * worldRay;
    modelRay = normalize(modelRay);
    vec3 invRay = vec3(1.0 / modelRay.x, 1.0 / modelRay.y, 1.0 / modelRay.z);

    // factor to go from lower res to higher res
    uint lodFactor = 1u << (lod - targetLod);
    // resolution of the higher res mip
    uvec3 targetTexSize = textureSize(densityMap, int(targetLod));
    // resolution of the lower res mip
    uvec3 currentTexSize = uvec3(targetTexSize.x >> (lod - targetLod),
        targetTexSize.y >> (lod - targetLod),
        targetTexSize.z >> (lod - targetLod));

    // min and max corner of the lower res voxel in the higher res mip
    vec3 pos1 = floor(startPosNormalized * currentTexSize) * float(lodFactor);
    vec3 pos2 = ceil(startPosNormalized * currentTexSize) * float(lodFactor);
    vec3 origin = startPosNormalized * currentTexSize * float(lodFactor);
    origin = modelPos.xyz;

    vec3 bbMin = min(pos1, pos2);
    vec3 bbMax = max(pos1, pos2);

    vec3 t1 = (bbMin - origin) * invRay;
    vec3 t2 = (bbMax - origin) * invRay;

    vec3 tMin = min(t1, t2);
    vec3 tMax = max(t1, t2);

    float tEnter = max(tMin.x, max(tMin.y, tMin.z));
    float tExit = min(tMax.x, min(tMax.y, tMax.z));
    // tEnter must be <= tExit
    // tExit must be >= 0

    if (tExit < 0.0 || tEnter > tExit)
        return vec3(0.0);

    vec3 entry = origin + tEnter * modelRay.xyz;

    float t = tEnter;
    float stepLen = length(modelRay);

    uint debugMaxSteps = 255;
    uint debugSteps = 0;

    while (t <= tExit) {
        vec3 pos = origin + t * modelRay.xyz;
        float density = texelFetch(densityMap, ivec3(round(pos)), int(targetLod)).x;
        if (density >= threshold)
            return pos / vec3(targetTexSize);

        t += stepLen;
        debugSteps++;
        if (debugSteps > debugMaxSteps)
            break;
    }

    return (origin + t * modelRay.xyz) / vec3(targetTexSize);
}


#include "volume_shading.inc.glsl"

void main(void) {
    vec3 normalLookUpNormalized = rayMarchPositionInMip(in_densityMapUV, 0u);
    vec3 normal = calculateNormal(normalLookUpNormalized, 0u);

    /*vec3 normalLookUpNormalized = rayMarchPositionInMip(in_densityMapUV, lod - 1);
    vec3 normal = calculateNormal(normalLookUpNormalized, lod - 1);*/

    //vec3 normal = in_normal;

    out_color = shadeFragment(in_density, in_worldPosition, normal);
}
