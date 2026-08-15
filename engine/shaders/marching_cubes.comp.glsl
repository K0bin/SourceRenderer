#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_shuffle : enable
#extension GL_EXT_maximal_reconvergence : enable

layout(local_size_x = 4, local_size_y = 4, local_size_z = 4) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0, std430) buffer readonly EdgeTable {
  uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, std430) buffer readonly TriTable {
  int[256u][16u] tris;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform texture3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, std430) buffer indicesBuffer {
    uint[] indices;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 5, std430) buffer bufferatomics {
    uint indexCount;
    uint instanceCount;
    uint firstIndex;
    int vertexOffset;
    uint firstInstance;
    uint vertexCount;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 7) uniform sampler linearSampler;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 8) uniform sampler nearestSampler;

layout(push_constant) uniform Config {
    uvec3 extent;
    float threshold;
    uvec3 minBox;
    uint lod;
};

uvec3 indexOffset(uint idx) {
    return uvec3(
         ((idx >> 1u) ^ idx) & 1u,
         (idx >> 2u) & 1u,
         (idx >> 1u) & 1u
    );
}

uint vertexKey(uvec3 pos1, uvec3 pos2) {
    uvec3 pos = pos1 + pos2;

    uvec3 sizes = uvec3(512 * 2);
    pos = min(sizes, pos);

    uint key = pos.z * sizes.x * sizes.y +
         pos.y * sizes.x +
         pos.x;

    return key;
}

const bool disableSubgroups = false;
const uvec3 WorkGroupSizeWithoutHelper = uvec3(3u, 3u, 3u);

uint vertexKeyFromIndexOffsets(uint idx1, uint idx2) {
    const bool useSubgroups = !disableSubgroups && gl_NumSubgroups <= 2;

    uvec3 workgroupBase = useSubgroups ? gl_WorkGroupID * WorkGroupSizeWithoutHelper : gl_WorkGroupID * gl_WorkGroupSize;
    uvec3 base = workgroupBase + gl_LocalInvocationID;
    uvec3 vertexPos1 = base + indexOffset(idx1);
    uvec3 vertexPos2 = base + indexOffset(idx2);
    uint vtxKey = vertexKey(vertexPos1, vertexPos2);
    return vtxKey;
}


uint localInvocationIndex(uvec3 invocationId) {
    return invocationId.z * gl_WorkGroupSize.x * gl_WorkGroupSize.y +
        invocationId.y * gl_WorkGroupSize.x +
        invocationId.x;
}


uvec3 localInvocationID(uint invocationIndex) {
    return uvec3(invocationIndex % gl_WorkGroupSize.x,
        (invocationIndex / gl_WorkGroupSize.x) % gl_WorkGroupSize.y,
        invocationIndex / (gl_WorkGroupSize.x * gl_WorkGroupSize.y));
}


void main() [[maximally_reconverges]] {
    const bool useSubgroups = !disableSubgroups && gl_NumSubgroups <= 2;

    uvec3 workgroupBase = useSubgroups ? gl_WorkGroupID * WorkGroupSizeWithoutHelper : gl_WorkGroupID * gl_WorkGroupSize;
    uvec3 base = workgroupBase + gl_LocalInvocationID;

    if (subgroupAll(any(greaterThanEqual(base + uvec3(1u), extent))))
        return;

    float densityInvocation1 = 0.0;
    float densityInvocation2 = 0.0;

    if (useSubgroups) {
        densityInvocation1 = texelFetch(sampler3D(densityImage, nearestSampler),
            min(ivec3(workgroupBase + localInvocationID(gl_SubgroupInvocationID)),
            ivec3(extent)), int(lod)).x;

        if (gl_NumSubgroups == 2u) {
            densityInvocation2 = texelFetch(sampler3D(densityImage, nearestSampler),
                min(ivec3(workgroupBase + localInvocationID((gl_SubgroupInvocationID + gl_SubgroupSize))),
                ivec3(extent)), int(lod)).x;
        }
    }

    uint voxelKey = 0u;
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uvec3 offset = uvec3(x, y, z);

                float value;
                bool targetIsActive = true;
                if (useSubgroups) {
                    uint densityInvocationIndex = localInvocationIndex(gl_LocalInvocationID + offset);
                    uint densitySubgroupInvocationIndex = densityInvocationIndex % gl_SubgroupSize;
                    uint densitySubgroupIndex = densityInvocationIndex / gl_SubgroupSize;

                    float densities[2u];
                    densities[0u] = subgroupShuffle(densityInvocation1, densitySubgroupInvocationIndex);
                    if (gl_NumSubgroups == 2u) {
                        densities[1u] = subgroupShuffle(densityInvocation2, densitySubgroupInvocationIndex);
                    } else {
                        densities[1u] = 0.0;
                    }
                    value = densities[densitySubgroupIndex % 2u];
                } else {
                    uvec3 pos = base + offset;
                    value = texelFetch(sampler3D(densityImage, nearestSampler), ivec3(pos), int(lod)).x;
                }

                uint index = ((x + z) & 1u) + z * 2u + y * 4u;
                voxelKey |= uint(value >= threshold) << index;
            }
        }
    }

    if (any(greaterThanEqual(base + uvec3(1u), extent)))
        return;

    if (useSubgroups && any(greaterThanEqual(gl_LocalInvocationID, uvec3(3u, 3u, 3u))))
        return; // Helper lanes, don't write vertices.

    if (voxelKey == 0u || voxelKey == 255u)
        return;

    instanceCount = 1u;

    uint[12u] vertexKeys;

    for (uint i = 0u; i < 4u; i++) {
        if ((edges[voxelKey] & (1u << i)) != 0u) {
            uint idx1 = i;
            uint idx2 = (i + 1u) % 4u;

            uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
            vertexKeys[i] = vtxKey;
        } else {
            vertexKeys[i] = ~0u;
        }
        if ((edges[voxelKey] & (16u << i)) != 0u) {
            uint idx1 = i + 4u;
            uint idx2 = (i + 1u) % 4u + 4u;

            uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
            vertexKeys[i + 4u] = vtxKey;
        } else {
            vertexKeys[i + 4u] = ~0u;
        }
        if ((edges[voxelKey] & (256u << i)) != 0u) {
            uint idx1 = i;
            uint idx2 = i + 4u;

            uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
            vertexKeys[i + 8u] = vtxKey;
        } else {
            vertexKeys[i + 8u] = ~0u;
        }
    }


    for (uint i = 0u; i < 16u && tris[voxelKey][i] != -1; i += 3u) {
        uint firstIndex = atomicAdd(indexCount, 3u);

        indices[firstIndex + 0u] = vertexKeys[tris[voxelKey][i + 0u]];
        indices[firstIndex + 1u] = vertexKeys[tris[voxelKey][i + 1u]];
        indices[firstIndex + 2u] = vertexKeys[tris[voxelKey][i + 2u]];
    }
}