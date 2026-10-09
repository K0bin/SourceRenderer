#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

#extension GL_EXT_mesh_shader : require
#extension GL_EXT_shader_explicit_arithmetic_types_int8 : require
#extension GL_EXT_scalar_block_layout : enable

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_shuffle : enable
#extension GL_EXT_maximal_reconvergence : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(local_size_x = 4, local_size_y = 4, local_size_z = 2) in;

#include "descriptor_sets.inc.glsl"
#include "frame_set_common.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform texture3D densityMap;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 7, std430) uniform TriTable {
    int[256u][17u] tris;
};

layout(push_constant, std430) uniform Config {
    mat4 model;
    uvec3 lodExtents;
    float threshold;
    uint lod;
};

struct TaskPayload {
    uint8_t voxelKeys[32];
    uvec3 workgroupBasePos;
};
taskPayloadSharedEXT TaskPayload payload;

layout(constant_id = 0) const bool renderDebugCube = false;

void main() {
    uvec3 workgroupBasePos = gl_WorkGroupID * gl_WorkGroupSize;

    uint8_t voxelKey = uint8_t(0u);
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uvec3 offset = uvec3(x, y, z);

                uvec3 pos = gl_GlobalInvocationID + offset;
                float density = texelFetch(sampler3D(densityMap, samplerNearest), ivec3(pos), int(lod)).x;

                uint index = ((x + z) & 1u) + z * 2u + y * 4u;

                voxelKey |= uint8_t(density >= threshold) << index;
            }
        }
    }

    uint indexCount;
    if (!renderDebugCube) {
        indexCount = tris[voxelKey][0u];
        indexCount = min(indexCount, 15u);
    } else {
        indexCount = 12u * 3u;
        indexCount *= uint(voxelKey != 0u && voxelKey != 255u);
    }

    if (gl_LocalInvocationIndex == 0u) {
        payload.workgroupBasePos = workgroupBasePos;
    }
    payload.voxelKeys[gl_LocalInvocationIndex] = voxelKey;

    uint primitiveCount = indexCount / 3u;
    uint subgroupPrimitiveCount = subgroupMax(primitiveCount);
    // 1 Workgroup per primitive. 1 Task thread = 1 Mesh thread = 1 triangle of a voxel
    EmitMeshTasksEXT(subgroupPrimitiveCount, 1, 1);
}
