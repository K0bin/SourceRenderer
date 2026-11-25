#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

#define CS
#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"
#include "frame_set_common.inc.glsl"
#include "util.inc.glsl"

#ifdef VBAO_VISIBILITY_ONLY
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, r16f) uniform writeonly image2D outputTexture;
#else
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, rgba16f) uniform writeonly image2D outputTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform texture2D shadedColor;
#endif
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform texture2D depthMap;

// Adapted from https://cybereality.com/screen-space-indirect-lighting-with-visibility-bitmask-improvement-to-gtao-ssao-real-time-ambient-occlusion-algorithm-glsl-shader-implementation/
// Which is adapted from "Screen Space Indirect Lighting with Visibility Bitmask" by Olivier Therrien, et al.
// https://cdrinmatane.github.io/posts/cgspotlight-slides/
// https://cdrinmatane.github.io/posts/ssaovb-code/

layout(push_constant, std430) uniform SSAOKernel {
    uint sampleCount;
    float sampleRadius;
    uint sliceCount;
    float hitThickness;
};

// https://blog.demofox.org/2022/01/01/interleaved-gradient-noise-a-different-kind-of-low-discrepancy-sequence/
float randf(int x, int y) {
    return mod(52.9829189 * mod(0.06711056 * float(x) + 0.00583715 * float(y), 1.0), 1.0);
}

// https://cdrinmatane.github.io/posts/ssaovb-code/
const uint sectorCount = 32u;
uint updateSectors(float minHorizon, float maxHorizon, uint outBitfield) {
    uint startBit = uint(minHorizon * float(sectorCount));
    uint horizonAngle = uint(ceil((maxHorizon - minHorizon) * float(sectorCount)));
    uint angleBit = horizonAngle > 0u ? uint(0xFFFFFFFFu >> (sectorCount - horizonAngle)) : 0u;
    uint currentBitfield = angleBit << startBit;
    return outBitfield | currentBitfield;
}

// get indirect lighting and ambient occlusion
vec4 getVisibility() {
    vec2 fragUV = (vec2(gl_GlobalInvocationID.xy) + vec2(0.5)) / vec2(imageSize(outputTexture));

    float visibility = 0.0;
    vec3 lighting = vec3(0.0);
    vec2 frontBackHorizon = vec2(0.0);
    vec2 aspect = vec2(1.0 / camera.aspectRatio, 1.0);
    float depth = textureLod(sampler2D(depthMap, samplerNearest), fragUV, 0).x;
    vec3 position = viewSpacePosition(fragUV, depth, camera.invProj);
    vec3 cameraPos = normalize(-position);
    vec3 normal = reconstructViewSpaceNormalCS(depthMap, samplerNearest, fragUV, camera.invProj);

    float sliceRotation = TwoPI / float(sliceCount);
    float sampleScale = (sampleRadius * camera.proj[0][0]) / position.z;
    float sampleOffset = 0.01;
    float jitter = randf(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y)) - 0.5;

    for (uint slice = 0u; slice < sliceCount; slice++) {
        uint occlusion = 0u;

        float phi = sliceRotation * (float(slice) + jitter) + PI;
        vec2 omega = vec2(cos(phi), sin(phi));
        vec3 direction = vec3(omega.x, omega.y, 0.0);
        vec3 orthoDirection = direction - dot(direction, cameraPos) * cameraPos;
        vec3 axis = cross(direction, cameraPos);
        vec3 projNormal = normal - axis * dot(normal, axis);
        float projLength = length(projNormal);

        float signN = sign(dot(orthoDirection, projNormal));
        float cosN = clamp(dot(projNormal, cameraPos) / projLength, 0.0, 1.0);
        float n = signN * acos(cosN);

        for (uint currentSample = 0u; currentSample < sampleCount; currentSample++) {
            float sampleStep = (float(currentSample) + jitter) / float(sampleCount) + sampleOffset;
            vec2 sampleUV = fragUV + sampleStep * sampleScale * vec2(omega.x, -omega.y) * aspect;
            float sampleDepth = textureLod(sampler2D(depthMap, samplerNearest), sampleUV, 0).x;
            vec3 samplePosition = viewSpacePosition(sampleUV, sampleDepth, camera.invProj);
            vec3 sampleDistance = samplePosition - position;
            float sampleLength = length(sampleDistance);
            vec3 sampleHorizon = sampleDistance / sampleLength;

            frontBackHorizon.x = dot(sampleHorizon, cameraPos);
            frontBackHorizon.y = dot(normalize(sampleDistance - cameraPos * hitThickness), cameraPos);

            frontBackHorizon = acos(frontBackHorizon);
            frontBackHorizon = clamp((frontBackHorizon + n + HalfPI) / PI, 0.0, 1.0);

            uint indirect = updateSectors(frontBackHorizon.x, frontBackHorizon.y, 0u);

            #ifndef VBAO_VISIBILITY_ONLY
            vec3 sampleNormal = reconstructViewSpaceNormalCS(depthMap, samplerNearest, sampleUV, camera.invProj);
            vec3 sampleLight = textureLod(sampler2D(shadedColor, samplerLinear), sampleUV, 0).rgb;
            lighting += (float(bitCount(indirect & ~occlusion)) / float(sectorCount)) *
                sampleLight * clamp(dot(normal, sampleHorizon), 0.0, 1.0) *
                clamp(dot(sampleNormal, -sampleHorizon), 0.0, 1.0);
            #endif

            occlusion |= indirect;
        }
        visibility += 1.0 - float(bitCount(occlusion)) / float(sectorCount);
    }

    visibility /= float(sliceCount);
    lighting /= float(sliceCount);

    return vec4(lighting, visibility);
}

void main() {
    vec4 color = getVisibility();
    #ifdef VBAO_VISIBILITY_ONLY
    color.r = color.a;
    #endif
    imageStore(outputTexture, ivec2(gl_GlobalInvocationID.xy), color);
}
