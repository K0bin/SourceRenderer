#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

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
    float threshold;
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


uint vertexKeyFromIndexOffsets(uint idx1, uint idx2) {
    uvec3 vertexPos1 = gl_GlobalInvocationID + indexOffset(idx1);
    uvec3 vertexPos2 = gl_GlobalInvocationID + indexOffset(idx2);
    uint vtxKey = vertexKey(vertexPos1, vertexPos2);
    return vtxKey;
}


void main() {
    uvec3 base = gl_GlobalInvocationID;

    uvec3 imgSize = textureSize(sampler3D(densityImage, nearestSampler), int(lod));
    if (any(greaterThan(base, imgSize)))
        return;

    uint voxelKey = 0u;
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uint index = ((x + z) & 1u) + z * 2u + y * 4u;
                uvec3 pos = base + uvec3(x, y, z);

                float value = textureLod(sampler3D(densityImage, nearestSampler), (vec3(pos) + vec3(0.5)) / vec3(imgSize), lod).x;
                bool inRange = gl_GlobalInvocationID.x < imgSize.x - 1u
                    && gl_GlobalInvocationID.y < imgSize.y - 1u
                    && gl_GlobalInvocationID.z < imgSize.z - 1u;
                voxelKey |= ((value >= threshold && inRange) ? 1u : 0u) << index;
            }
        }
    }

    if (voxelKey == 0u || voxelKey == 255u)
        return;

    instanceCount = 1u;
    firstIndex = 0u;
    vertexOffset = 0;
    firstInstance = 0u;
    vertexCount = 0u;

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