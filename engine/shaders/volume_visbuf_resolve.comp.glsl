#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, r32ui) uniform readonly uimage2D visbuf;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler2D depthMap;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2, rgba8) uniform writeonly image2D outputTexture;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3, std430) readonly buffer Indices {
    uint indices[];
};

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 4) uniform sampler3D densityMap;

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 5) uniform sampler2D transferFunction;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 6) uniform samplerCube envMapDiffuse;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 7) uniform samplerCube envMapSpecular;
layout (set = DESCRIPTOR_SET_FREQUENT, binding = 8) uniform sampler2D integrationLUT;

layout(set = DESCRIPTOR_SET_FRAME, binding = 0, std140) uniform CameraUBO {
  Camera camera;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
    mat4 model;
    mat4 invModel;
    float threshold;
    uint lod;
    float roughness;
    float metalness;
    vec3 f0;
};

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

#define CS
#include "util.inc.glsl"

#include "volume_shading.inc.glsl"

void main() {
    ivec2 outputPx = ivec2(gl_GlobalInvocationID.xy);

    ivec2 outputSize = imageSize(outputTexture);
    if (any(greaterThanEqual(outputPx, outputSize))) {
        return;
    }

    uint primitiveId = imageLoad(visbuf, ivec2(outputPx)).x;
    if (primitiveId == ~0u)
        return;

    vec2 texCoord = (vec2(outputPx) + 0.5) / vec2(outputSize);

    float depth = textureLod(depthMap, texCoord, 0).x;
    vec3 worldPos = worldSpacePosition(texCoord, depth, camera.invViewProj);
    vec4 modelSpacePos = invModel * vec4(worldPos, 1.0);
    vec3 densityMapPos = modelSpacePos.xyz / exp2(lod);
    vec3 densityMapSize = textureSize(densityMap, int(lod));
    vec3 densityMapUV = (densityMapPos + 0.5) / densityMapSize;
    float density = textureLod(densityMap, densityMapUV, lod).x;

    vec3 normal = calculateNormal(densityMapUV, lod);
    mat4 normalModelMat = transpose(invModel);
    vec3 worldSpaceNormal = normalize(normalModelMat * vec4(normal, 0)).xyz;

    /*uint primitiveId = imageLoad(visbuf, ivec2(outputPx)).x;
    uint key0 = indices[primitiveId * 3u];
    uint key1 = indices[primitiveId * 3u];
    uint key2 = indices[primitiveId * 3u];*/

    vec4 color = vec4(0);

    color.xyz = normal * 0.5 + vec3(0.5);
    color.w = 1.0;

    color = shadeFragment(density, worldPos, worldSpaceNormal);

    imageStore(outputTexture, outputPx, color);
}
