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

layout(local_size_x = 4, local_size_y = 4, local_size_z = 4) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0, std430) buffer readonly EdgeTable {
  uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, std430) buffer readonly TriTable {
  int[256u][17u] tris;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform texture3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, std430) buffer indicesBuffer {
    uint[] indices;
};

struct IndirectCommand {
    uint indexCount;
    uint instanceCount;
    uint firstIndex;
    int vertexOffset;
    uint firstInstance;
    uint vertexCount;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 5, scalar) buffer bufferatomics {
    IndirectCommand opaque;
    IndirectCommand transparent;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 6, std430) buffer indicesTransparentBuffer {
    uint[] indicesTransparent;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 7) uniform sampler linearSampler;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 8) uniform sampler nearestSampler;

layout(push_constant) uniform Config {
    uvec3 extent;
    float threshold;
    uvec3 minBox;
    uint lod;
    float thresholdTransparency;
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

uint vertexKeyFromIndexOffsets(uint idx1, uint idx2) {
    uvec3 workgroupBase = gl_WorkGroupID * gl_WorkGroupSize + minBox;
    uvec3 base = workgroupBase + gl_LocalInvocationID;
    uvec3 vertexPos1 = base + indexOffset(idx1);
    uvec3 vertexPos2 = base + indexOffset(idx2);
    uint vtxKey = vertexKey(vertexPos1, vertexPos2);
    return vtxKey;
}

uint[12u] buildVertexKeys(uint voxelKey) {
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
    return vertexKeys;
}


void main() {
    uvec3 workgroupBase = gl_WorkGroupID * gl_WorkGroupSize + minBox;
    uvec3 base = workgroupBase + gl_LocalInvocationID;
    uvec3 unshiftedBase = base - minBox;

    if (subgroupAll(any(greaterThanEqual(unshiftedBase + uvec3(1u), extent))))
        return;

    uint voxelKey = 0u;
    uint voxelKeyTransparent = 0u;
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uvec3 offset = uvec3(x, y, z);

                uvec3 pos = base + offset;
                float density = texelFetch(sampler3D(densityImage, nearestSampler), ivec3(pos), int(lod)).x;

                uint index = ((x + z) & 1u) + z * 2u + y * 4u;
                voxelKey |= uint(density >= threshold) << index;
                voxelKeyTransparent |= uint(density >= thresholdTransparency) << index;
            }
        }
    }

    if (any(greaterThanEqual(unshiftedBase + uvec3(1u), extent)))
        return;

    if ((voxelKey == 0u || voxelKey == 255u) && (voxelKeyTransparent == 0u || voxelKeyTransparent == 255u))
        return;

    transparent.instanceCount = 1u;
    opaque.instanceCount = 1u;

    uint indexCount = voxelKey != 0u || voxelKey != 255u
        ? tris[voxelKey][0u] : 0u;
    uint transparentIndexCount = voxelKeyTransparent != 0u || voxelKeyTransparent != 255u
        ? tris[voxelKeyTransparent][0u] : 0u;

    indexCount = min(indexCount, 16u);
    transparentIndexCount = min(transparentIndexCount, 16u);

    if (indexCount != 0u) {
        uint[12u] vertexKeys = buildVertexKeys(voxelKey);
        uint firstIndex = atomicAdd(opaque.indexCount, indexCount);

        for (uint i = 0u; i < indexCount; i += 3u) {
            indices[firstIndex + i + 0u] = vertexKeys[tris[voxelKey][1u + i + 0u]];
            indices[firstIndex + i + 1u] = vertexKeys[tris[voxelKey][1u + i + 1u]];
            indices[firstIndex + i + 2u] = vertexKeys[tris[voxelKey][1u + i + 2u]];
        }
    }

    if (transparentIndexCount != 0u) {
        uint[12u] vertexKeysTransparent = buildVertexKeys(voxelKeyTransparent);
        uint firstIndex = atomicAdd(transparent.indexCount, transparentIndexCount);

        for (uint i = 0u; i < transparentIndexCount; i += 3u) {
            indicesTransparent[firstIndex + i + 0u] = vertexKeysTransparent[tris[voxelKeyTransparent][1u + i + 0u]];
            indicesTransparent[firstIndex + i + 1u] = vertexKeysTransparent[tris[voxelKeyTransparent][1u + i + 1u]];
            indicesTransparent[firstIndex + i + 2u] = vertexKeysTransparent[tris[voxelKeyTransparent][1u + i + 2u]];
        }
    }
}
