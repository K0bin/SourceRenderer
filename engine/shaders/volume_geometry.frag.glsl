#version 450
#extension GL_ARB_separate_shader_objects: enable
#extension GL_GOOGLE_include_directive: enable

#include "descriptor_sets.inc.glsl"
#include "pbr.inc.glsl"
#include "camera.inc.glsl"
#include "util.inc.glsl"

layout (location = 0) in float in_density;
layout (location = 1) in vec3 in_worldPosition;
layout (location = 2) in vec3 in_densityMapUV;

layout (location = 0) out vec4 out_color;
layout (location = 1) out float out_sss_intensity;

layout (push_constant, std430) uniform Params {
    layout (offset = 96) vec3 f0;
    float roughness;
    mat4 invModel;
    float metalness;
    uint lod;
    float threshold;
    float width;
    float height;
};

layout (set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
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
    uint meshLod = lod;

    vec3 worldPos = worldSpacePosition(gl_FragCoord.xy / vec2(width, height), 0.0, camera.invViewProj);
    vec3 modelPos = (invModel * vec4(worldPos, 1.0)).xyz;
    // Half pixel because pixel center when sampling is 0.5
    modelPos += vec3(0.5);

    vec2 ndc = (gl_FragCoord.xy / vec2(width, height)) * 2.0 - 1.0;
    float viewX = ndc.x / camera.proj[0][0];
    float viewY = -ndc.y / camera.proj[1][1];
    vec4 viewRay = vec4(viewX, viewY, 1, 0);

    vec4 worldRay = camera.invView * viewRay;
    vec4 modelRay = invModel * worldRay;
    modelRay = normalize(modelRay);

    // Use normal instead of view ray. More consistent.

    vec3 invRay = vec3(1.0 / modelRay.x, 1.0 / modelRay.y, 1.0 / modelRay.z);

    // resolution of mip 0
    uvec3 texSize = textureSize(densityMap, 0);
    // resolution of the higher res mip
    uvec3 targetTexSize = uvec3(texSize.x >> targetLod, texSize.y >> targetLod, texSize.z >> targetLod);
    // resolution of the lower res mip that was used to generate the mesh
    uvec3 geometryTexSize = uvec3(texSize.x >> meshLod, texSize.y >> meshLod, texSize.z >> meshLod);

    // factor to go from lower res to higher res
    uint lodFactor = 1u << (meshLod - targetLod);
    // min and max corner of the lower res voxel in the higher res mip
    vec3 pos1 = floor((startPosNormalized * targetTexSize) / vec3(float(lodFactor))) * float(lodFactor);
    vec3 pos2 = pos1 + vec3(lodFactor);
    vec3 origin = modelPos.xyz * float(lodFactor);

    vec3 bbMin = min(pos1, pos2);
    vec3 bbMax = max(pos1, pos2);
	bbMin *= -sign(bbMin) * 1.5;
	bbMax *= 1.5;

    vec3 t1 = (bbMin - origin) * invRay;
    vec3 t2 = (bbMax - origin) * invRay;

    vec3 tMin = min(t1, t2);
    vec3 tMax = max(t1, t2);

    float tEnter = max(tMin.x, max(tMin.y, tMin.z));
    float tExit = min(tMax.x, min(tMax.y, tMax.z));

    // Calculate intersections with texture box
	vec3 tTex1 = (vec3(0) - origin) * invRay;
    vec3 tTex2 = (targetTexSize - origin) * invRay;

    vec3 tTexMin = min(tTex1, tTex2);
    vec3 tTexMax = max(tTex1, tTex2);

    float tTexEnter = max(tTexMin.x, max(tTexMin.y, tTexMin.z));
    float tTexExit = min(tTexMax.x, min(tTexMax.y, tTexMax.z));

	tEnter = max(tEnter, tTexEnter);
	tExit = min(tExit, tTexExit);

    // tEnter must be <= tExit
    // tExit must be >= 0
    if (tExit < 0.0 || tEnter > tExit)
    return vec3(0);

    float stepLen = 1.0;
    float t = tEnter;

    while (t <= tExit) {
        vec3 pos = origin + t * modelRay.xyz;
        float density = textureLod(densityMap, pos / vec3(targetTexSize), int(targetLod)).x;
        if (density >= threshold)
        return pos / vec3(targetTexSize);

        t += stepLen;
    }

    return vec3(0);
}

#include "volume_shading.inc.glsl"

void main(void) {
    uint normalLod = 0;
    vec3 normalLookUpNormalized = rayMarchPositionInMip(in_densityMapUV, normalLod);
    vec3 normal;
    float density;
    if (dot(normalLookUpNormalized, normalLookUpNormalized) > 0.001) {
        normal = calculateNormal(normalLookUpNormalized, normalLod);
        density = textureLod(densityMap, normalLookUpNormalized, int(normalLod)).x;
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
