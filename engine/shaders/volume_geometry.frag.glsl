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
layout (location = 1) out float out_sss_intensity;

layout(push_constant, std430) uniform Params {
    layout(offset = 160) vec3 f0;
    float roughness;
    mat4 invModel;
    float metalness;
    uint lod;
    float threshold;
    float width;
    float height;
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler3D densityMap;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler2D transferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform samplerCube envMapDiffuse;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 3) uniform samplerCube envMapSpecular;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 4) uniform sampler2D integrationLUT;


vec3 calculateGradient(vec3 densityMapUV, uint normalLod) {
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(textureSize(densityMap, int(normalLod)));
    vec3 singlePixel = vec3(1.0) / imgSize;
    vec3 singlePixelX = vec3(singlePixel.x, 0, 0);
    vec3 singlePixelY = vec3(0, singlePixel.y, 0);
    vec3 singlePixelZ = vec3(0, 0, singlePixel.z);

    normal.x = textureLod(densityMap, densityMapUV - singlePixelX, normalLod).x
                                - textureLod(densityMap, densityMapUV + singlePixelX, normalLod).x;
    normal.y = textureLod(densityMap, densityMapUV - singlePixelY, normalLod).x
                                - textureLod(densityMap, densityMapUV + singlePixelY, normalLod).x;
    normal.z = textureLod(densityMap, densityMapUV - singlePixelZ, normalLod).x
                                - textureLod(densityMap, densityMapUV + singlePixelZ, normalLod).x;
    return normal;
}

vec3 calculateNormal(vec3 densityMapUV, uint normalLod) {
    return normalize(calculateGradient(densityMapUV, normalLod));
}


// targetLod must be smaller (=> higher res) than the current lod in the push constants
vec3 rayMarchPositionInMip(vec3 startPosNormalized, uint targetLod) {
    vec4 ndcPos = vec4((gl_FragCoord.x / width) * 2.0 - 1.0, (gl_FragCoord.y / height) * -2.0 + 1.0, gl_FragCoord.z, 1.0);

    vec4 worldPos = camera.invViewProj * ndcPos;
    worldPos /= worldPos.w;
    vec4 modelPos = invModel * worldPos;
    modelPos.xyz *= exp2(lod);
    vec4 viewPos = camera.invProj * ndcPos;
    viewPos.xyz /= viewPos.w;

    vec4 viewRay = vec4(0,0,1,0);
    //vec4 viewRay = vec4(normalize(viewPos.xyz), 0.0); //
    vec4 worldRay = camera.invView * viewRay;
    vec4 modelRay = invModel * worldRay;
    modelRay = normalize(modelRay);
    vec3 invRay = vec3(1.0 / modelRay.x, 1.0 / modelRay.y, 1.0 / modelRay.z);

    uint meshLod = lod;
    // factor to go from lower res to higher res
    uint lodFactor = 1u << (meshLod - targetLod);
    // resolution of the higher res mip
    uvec3 targetTexSize = textureSize(densityMap, int(targetLod));
    // resolution of the lower res mip
    uvec3 currentTexSize = uvec3(targetTexSize.x >> (meshLod - targetLod),
        targetTexSize.y >> (meshLod - targetLod),
        targetTexSize.z >> (meshLod - targetLod));

    // min and max corner of the lower res voxel in the higher res mip
    vec3 pos1 = floor(startPosNormalized * currentTexSize) * float(lodFactor);
    vec3 pos2 = pos1 + vec3(lodFactor);
    vec3 origin = startPosNormalized * currentTexSize * float(lodFactor);

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

    // even when we find hits, the position doesnt always work with the normal calc algorithm
    // maybe i can do something with subgroup quads?

    float stepLen = min(abs(tEnter), 1.0);

    // Actual hit might not be in the current small res voxel :(
    // Adding extra size before and after works around that.
    //tEnter -= stepLen;
    //tExit += stepLen;

    float t = tEnter;

    uint debugMaxSteps = 255;
    uint debugSteps = 0;

    while (t <= tExit) {
        vec3 pos = origin + t * modelRay.xyz;
        //float density = texelFetch(densityMap, ivec3(round(pos)), int(targetLod)).x;
        /*float density = textureLod(densityMap, pos / vec3(targetTexSize), int(targetLod)).x;
        if (density >= threshold)
            return pos / vec3(targetTexSize);*/

        vec3 grad = calculateGradient(pos / vec3(targetTexSize), targetLod);
        if (dot(grad, grad) > 0.001)
            return pos / vec3(targetTexSize);

        t += stepLen;
        debugSteps++;
        if (debugSteps > debugMaxSteps)
            return vec3(0.0);
    }

    return vec3(0.0);
}



vec3 findNormalPoint(vec3 densityMapUV, uint normalLod) {
    float highResDensity = threshold - textureLod(densityMap, densityMapUV, normalLod).x;
    float dir = sign(highResDensity);

    vec3 lowResNormal = calculateNormal(densityMapUV, lod);
    //vec3 lowResNormal = normalize(modelRay.xyz);

    uint lodFactor = 1u << (lod - normalLod);
    float lodDividend = 1.0 / float(lodFactor);
    uvec3 res = textureSize(densityMap, int(normalLod));

    const float eps = 1.0 / float(lodFactor);
    const float minGrad = sqrt(0.001);
    const uint maxSteps = 999u;
    vec3 highResUV = densityMapUV + lowResNormal * eps * dir * floor(float(maxSteps) * 0.5);
    vec3 grad;
    float gradLenSq;
    uint steps = 0u;
    do {
        highResUV -= lowResNormal * eps * dir;
        grad = calculateGradient(highResUV, normalLod);
        gradLenSq = dot(grad, grad);
        steps++;
    } while (gradLenSq < minGrad && steps < maxSteps);

    return highResUV;
}


#include "volume_shading.inc.glsl"

void main(void) {
    uint normalLod = 3u;
    vec3 normalLookUpNormalized = rayMarchPositionInMip(in_densityMapUV, normalLod);
    vec3 normal;
    if (dot(normalLookUpNormalized, normalLookUpNormalized) > 0.001)
        normal = calculateNormal(normalLookUpNormalized, normalLod);
    else
        normal = vec3(0.0);
        //normal = calculateNormal(in_densityMapUV, lod);

    /*normal = calculateGradient(in_densityMapUV, normalLod);
    float len = length(normal);
    if (len < 0.01) {
        normal = calculateNormal(in_densityMapUV, lod);
        len = length(normal);
    }
    normal /= len;*/

    //normal = (in_densityMapUV - normalLookUpNormalized) * 1.5;
    //vec3 normal = normalLookUpNormalized;
    //normal = in_densityMapUV;
    //normal = vec3(1.0);
    //normal = vec3(0.0);

    //vec3 normalUV = findNormalPoint(in_densityMapUV, 0u);
    //vec3 normal = calculateNormal(normalUV, 0u);

    /*vec3 normalLookUpNormalized = rayMarchPositionInMip(in_densityMapUV, lod - 1);
    vec3 normal = calculateNormal(normalLookUpNormalized, lod - 1);*/

    //vec3 normal = in_normal;

    out_color = shadeFragment(in_density, in_worldPosition, normal, out_sss_intensity);
}
