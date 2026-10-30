#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

#extension GL_EXT_scalar_block_layout : enable

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_shuffle : enable
#extension GL_EXT_maximal_reconvergence : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;

#include "descriptor_sets.inc.glsl"
#include "frame_set_common.inc.glsl"
#include "morton_code.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform texture3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform texture3D densityImageMin;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform texture3D densityImageMax;

layout(constant_id = 0) const uint thresholdsCountConst = 0u;
layout(constant_id = 1) const uint TargetWorkGroupSizeX = 8;
layout(constant_id = 2) const uint TargetWorkGroupSizeY = 8;
layout(constant_id = 3) const uint TargetWorkGroupSizeZ = 8;
layout(constant_id = 4) const bool PrepareBatchedDispatch = true;

struct IndirectCommand {
    uint x;
    uint y;
    uint z;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 3, scalar) buffer indirectbuffers{
    IndirectCommand command;
    uint _padding;
    uvec4[] workgroupPositions;
} indirectBuffers[16u];

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, scalar) uniform thresholds {
    float[16u] minThresholds;
};

layout(push_constant, std430) uniform Config {
    uvec3 targetLodExtents;
    uint targetLod;
    uint reduction;
    uint thresholdsCount;
};

uint divCeil(uint numerator, uint denominator) {
    return (numerator + denominator - 1u) / denominator;
}

uvec3 divCeil(uvec3 numerator, uvec3 denominator) {
    return (numerator + denominator - uvec3(1u)) / denominator;
}

void spawnWorkgroups(uvec3 count, uint targetBufferIndex) {
    // Assume that the number of workgroups spawned by one culling invocation is never more than 65k.
    // Culling voxel size of 32x32x32 (reduction by 5) is 32k.

    uint totalWorkgroupsToSpawn = count.x * count.y * count.z;
    uint firstGlobalWorkgroupIndex = atomicAdd(indirectBuffers[targetBufferIndex].command.x, totalWorkgroupsToSpawn);
    indirectBuffers[targetBufferIndex].command.y = 1;
    indirectBuffers[targetBufferIndex].command.z = 1;
    // TODO spread this out on Qualcomm because max workgroups x there is 65535

    for (uint x = 0u; x < count.x; x++) {
        for (uint y = 0u; y < count.y; y++) {
            for (uint z = 0u; z < count.z; z++) {
                uint index = mortonCode(uvec3(x, y, z), count);
                indirectBuffers[targetBufferIndex].workgroupPositions[firstGlobalWorkgroupIndex + index] = uvec4(gl_GlobalInvocationID, 1u);
            }
        }
    }
}

void main() {
    // When targetting a workgroup size of 4x4x4, reduction should be at least 2. For 8x8x8 at least 3.
    uint maxGroupSizeDimension = max(TargetWorkGroupSizeX, max(TargetWorkGroupSizeY, TargetWorkGroupSizeZ));
    uint actualReduction = max(reduction, 1u << findMSB(maxGroupSizeDimension));

    // Naive count is 1 << reduction but that breaks due to rounding.
    uvec3 cullingVoxelSize = divCeil(targetLodExtents, targetLodExtents >> actualReduction);

    // Number of workgroups
    uvec3 workgroups = divCeil(cullingVoxelSize, uvec3(TargetWorkGroupSizeX, TargetWorkGroupSizeY, TargetWorkGroupSizeZ));

    uint finalThresholdsCount = thresholdsCountConst == 0u ? thresholdsCount : thresholdsCountConst;

    float density = texelFetch(sampler3D(densityImageMax, samplerNearest), ivec3(gl_GlobalInvocationID), int(targetLod + actualReduction)).r;
    bool empty = true;
    for (uint i = 0u; i < finalThresholdsCount; i++) {
        float threshold = minThresholds[i];
        bool notEmptyDrawable = density >= threshold;
        empty = empty && notEmptyDrawable;

        if (!PrepareBatchedDispatch && notEmptyDrawable)
            spawnWorkgroups(workgroups, i);
    }

    if (PrepareBatchedDispatch && !empty)
        spawnWorkgroups(workgroups, 0u);
}
