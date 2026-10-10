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

layout(constant_id = 0) const uint TargetWorkGroupSizeX = 8;
layout(constant_id = 1) const uint TargetWorkGroupSizeY = 8;
layout(constant_id = 2) const uint TargetWorkGroupSizeZ = 8;

struct IndirectCommand {
    uint x;
    uint y;
    uint z;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 3, scalar) buffer bufferatomics {
    IndirectCommand command;
    uint _padding;
    uvec4[] workgroupPositions;
};

layout(push_constant, std430) uniform Config {
    uvec3 targetLodExtents;
    uint targetLod;
    uint reduction;
    float threshold;
};

uint divCeil(uint numerator, uint denominator) {
    return (numerator + denominator - 1u) / denominator;
}

void main() {
    // When targetting a workgroup size of 4x4x4, reduction should be at least 2. For 8x8x8 at least 3.

    float density = texelFetch(sampler3D(densityImageMax, samplerNearest), ivec3(gl_GlobalInvocationID), int(targetLod + reduction)).r;
    if (density < threshold)
        return;

    // Naive count is 1 << reduction but that breaks due to rounding.
    uvec3 cullingVoxelSize = uvec3(
            divCeil(targetLodExtents.x, targetLodExtents.x >> reduction),
            divCeil(targetLodExtents.y, targetLodExtents.y >> reduction),
            divCeil(targetLodExtents.z, targetLodExtents.z >> reduction)
    );

    uvec3 workgroups = uvec3(
            (cullingVoxelSize.x + TargetWorkGroupSizeX - 1u) / TargetWorkGroupSizeX,
            (cullingVoxelSize.y + TargetWorkGroupSizeY - 1u) / TargetWorkGroupSizeY,
            (cullingVoxelSize.y + TargetWorkGroupSizeZ - 1u) / TargetWorkGroupSizeZ);
    uint totalSpawnedWorkgroups = workgroups.x * workgroups.y * workgroups.z;

    // Assume that the number of workgroups spawned by one culling invocation is never more than 65k.
    // Culling voxel size of 32x32x32 (reduction by 5) is 32k.

    uint firstGlobalWorkgroupIndex = atomicAdd(command.x, totalSpawnedWorkgroups);
    command.y = 1;
    command.z = 1;
    // TODO spread this out on Qualcomm because max workgroups x there is 65535

    for (uint x = 0u; x < workgroups.x; x++) {
        for (uint y = 0u; y < workgroups.y; y++) {
            for (uint z = 0u; z < workgroups.z; z++) {
                uint index = mortonCode(uvec3(x,y,z), workgroups);
                workgroupPositions[firstGlobalWorkgroupIndex + index] = uvec4(gl_GlobalInvocationID, 1u);
            }
        }
    }
}
