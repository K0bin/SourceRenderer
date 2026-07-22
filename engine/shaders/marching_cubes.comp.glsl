#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_shader_explicit_arithmetic_types_float16 : enable

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable


#define GLOBAL_HASHMAP

// Only supports warp sizes <= 64
#define SUBGROUP_SHARING


layout(local_size_x = 4, local_size_y = 4, local_size_z = 4) in;

#include "descriptor_sets.inc.glsl"

struct Vertex {
    f16vec3 pos;
    //f16vec3 normal;
};

struct HashmapEntry {
    uint key;
    uint vertexIndex;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0, std430) buffer readonly EdgeTable {
  uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, std430) buffer readonly TriTable {
  int[256u][16u] tris;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2, r32f) uniform readonly image3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 3, scalar) buffer verticesBuffer {
    Vertex[] vertices;
};
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
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 6, std430) buffer hashmap {
    HashmapEntry[] hashmapEntries;
};

layout(push_constant) uniform Config {
    vec3 scale;
    float threshold;
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

    uvec3 sizes = gl_NumWorkGroups * gl_WorkGroupSize * 2;
    pos = min(sizes, pos);

    uint key = pos.z * sizes.x * sizes.y +
         pos.y * sizes.x +
         pos.x;

    return key;
}

vec4 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    float value1 = imageLoad(densityImage, ivec3(pos1)).x;
    float value2 = imageLoad(densityImage, ivec3(pos2)).x;
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec4(vec3(pos1), value1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec4(vec3(pos2), value2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return mix(vec4(pos1, value1), vec4(pos2, value2), a);
}

vec3 calculateNormal(vec3 pos) {
    ivec3 imgPos = ivec3(round(pos / scale));
    vec3 normal = vec3(0.0);
    normal.x = (imageLoad(densityImage, imgPos - ivec3(1, 0, 0))
                                - imageLoad(densityImage, imgPos + ivec3(1, 0, 0))).x;
    normal.y = (imageLoad(densityImage, imgPos - ivec3(0, 1, 0))
                                - imageLoad(densityImage, imgPos + ivec3(0, 1, 0))).x;
    normal.z = (imageLoad(densityImage, imgPos - ivec3(0, 0, 1))
                                - imageLoad(densityImage, imgPos + ivec3(0, 0, 1))).x;
    return normalize(normal);
}


// https://nosferalatu.com/SimpleGPUHashTable.html
// Modified it to add a probing maximum and prevent hangs if it runs out of space.

const uint HashmapEmptyKey = ~0u;
const uint HashmapEmptyValue = ~0u;
const uint HashmapCapacity = 400000u;
const uint HashmapMaxProbing = 0u; //99u; //0u; // 0 to disable the limit.

uint hash(uint key) {
    uint hash = key;
    hash ^= hash >> 16;
    hash *= 0x85ebca6b;
    hash ^= hash >> 13;
    hash *= 0xc2b2ae35;
    hash ^= hash >> 16;
    return hash;
}

void hashmapInsert(uint key, uint vertexIndex) {
    uint slot = hash(key) % HashmapCapacity;
    uint startSlot = slot;
    uint probes = 0u;

    while (true) {
        uint prev = atomicCompSwap(hashmapEntries[slot].key, HashmapEmptyKey, key);
        if (prev == key || prev == HashmapEmptyKey) {
            atomicExchange(hashmapEntries[slot].vertexIndex, vertexIndex);
            return;
        }
        slot = (slot + 1u) % HashmapCapacity;
        if (slot == startSlot) {
            return;
        }
        if (HashmapMaxProbing != 0u && probes >= HashmapMaxProbing) {
            return;
        }
        probes++;
    }
}

uint hashmapLookup(uint key) {
    uint slot = hash(key) % HashmapCapacity;
    uint startSlot = slot;
    uint probes = 0u;

    while (true) {
       uint slotKey = atomicAdd(hashmapEntries[slot].key, 0u);
       if (slotKey == key) {
           return atomicAdd(hashmapEntries[slot].vertexIndex, 0u);
       }
       if (slotKey == HashmapEmptyKey) {
           return HashmapEmptyValue;
       }
       slot = (slot + 1u) % HashmapCapacity;
       if (slot == startSlot) {
           return HashmapEmptyValue;
       }
       if (HashmapMaxProbing != 0u && probes >= HashmapMaxProbing) {
           return HashmapEmptyValue;
       }
       probes++;
    }
    return HashmapEmptyValue;
}


uint vertexKeyFromIndexOffsets(uint idx1, uint idx2) {
    uvec3 vertexPos1 = gl_GlobalInvocationID + indexOffset(idx1);
    uvec3 vertexPos2 = gl_GlobalInvocationID + indexOffset(idx2);
    uint vtxKey = vertexKey(vertexPos1, vertexPos2);
    return vtxKey;
}


uint addVertex(vec4 vertexPos) {
    uint index = atomicAdd(vertexCount, 1u);
    vertices[index].pos = f16vec3(vertexPos.xyz);
    return index;
}


uint calculateAndAddVertex(uint idx1, uint idx2) {
#ifdef SUBGROUP_SHARING
    return ~0u;
#endif

    uvec3 vertexPos1 = gl_GlobalInvocationID + indexOffset(idx1);
    uvec3 vertexPos2 = gl_GlobalInvocationID + indexOffset(idx2);

    uint index;

#ifdef GLOBAL_HASHMAP
    uint vtxKey = vertexKey(vertexPos1, vertexPos2);
    index = hashmapLookup(vtxKey);
    if (index == HashmapEmptyValue) {
#endif

    vec4 vertexPos = interpolateVertices(vertexPos1, vertexPos2) * vec4(scale, 1.0);
    index = addVertex(vertexPos);

#ifdef GLOBAL_HASHMAP
    hashmapInsert(vtxKey, index);
    }
#endif

    return index;
}


vec4 vertexPosFromKey(uint vertexKey) {
    uvec3 sizes = gl_NumWorkGroups * gl_WorkGroupSize * 2;

    uvec3 pos = uvec3(vertexKey % sizes.x,
        (vertexKey / sizes.x) % sizes.y,
        vertexKey / (sizes.x * sizes.y));

    uvec3 pos1 = pos / 2u;
    uvec3 pos2 = pos1 + (pos % 2u);

    return interpolateVertices(pos1, pos2) * vec4(scale, 1.0);
}


uint localKeyFromOffsets(uint localIndex, uvec3 offset1, uvec3 offset2) {
        uint key = localIndex * 2u;
        key += offset1.z * gl_WorkGroupSize.y * gl_WorkGroupSize.x + offset1.y * gl_WorkGroupSize.x + offset1.x;
        key += offset2.z * gl_WorkGroupSize.y * gl_WorkGroupSize.x + offset2.y * gl_WorkGroupSize.x + offset2.x;
        return key;
}

uint localKeyFromIndexOffsets(uint localIndex, uint idx1, uint idx2) {
    return localKeyFromOffsets(localIndex, indexOffset(idx1), indexOffset(idx2));
}


/* 4 * 4 * 4 Workgroup size
   * 2 * 2 * 2 because of the doubled resolution to vertex positions inbetween two voxels.
*/
shared uint sharedVertexIndices[512u];


void main() {
    uvec3 base = gl_GlobalInvocationID;

    ivec3 imgSize = imageSize(densityImage);
    uint voxelKey = 0u;
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uint index = ((x + z) & 1u) + z * 2u + y * 4u;
                ivec3 pos = ivec3(int(base.x + x), int(base.y + y), int(base.z + z));

                float value = imageLoad(densityImage, pos).x;
                bool inRange = gl_GlobalInvocationID.x < imgSize.x - 1u
                    && gl_GlobalInvocationID.y < imgSize.y - 1u
                    && gl_GlobalInvocationID.z < imgSize.z - 1u;
                voxelKey |= ((value >= threshold && inRange) ? 1u : 0u) << index;
            }
        }
    }

    if (subgroupAll(voxelKey == 0u || voxelKey == 255u))
        return;

    instanceCount = 1u;
    firstIndex = 0u;
    vertexOffset = 0;
    firstInstance = 0u;

    uint[12u] vertexKeys;
    uint[12u] cubeVertexIndices;
    uint arrayIndexMask = 0u;

    uint subgroupMinKey = localKeyFromIndexOffsets((gl_LocalInvocationIndex / gl_SubgroupSize) * gl_SubgroupSize, 0u, 0u);
    const uint localKeyBitMasksCount = 64u * 2u / 32u;
    uint[localKeyBitMasksCount] localKeyUsedBitMask;
    for (uint i = 0u; i < localKeyBitMasksCount; i++) {
        localKeyUsedBitMask[i] = 0u;
    }

    for (uint i = 0u; i < 4u; i++) {
        if ((edges[voxelKey] & (1u << i)) != 0u) {
            uint idx1 = i;
            uint idx2 = (i + 1u) % 4u;

            cubeVertexIndices[i] = calculateAndAddVertex(idx1, idx2);

            uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
            vertexKeys[i] = vtxKey;
            arrayIndexMask |= 1u << i;

            uint localKey = localKeyFromIndexOffsets(gl_LocalInvocationIndex, idx1, idx2);
            uint diff = localKey - subgroupMinKey;
            localKeyUsedBitMask[diff / 32u] |= 1u << (diff % 32u);
        } else {
            cubeVertexIndices[i] = 0u;
            vertexKeys[i] = ~0u;
        }
        if ((edges[voxelKey] & (16u << i)) != 0u) {
            uint idx1 = i + 4u;
            uint idx2 = (i + 1u) % 4u + 4u;

            cubeVertexIndices[i + 4u] = calculateAndAddVertex(idx1, idx2);

            uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
            vertexKeys[i + 4u] = vtxKey;
            arrayIndexMask |= 1u << (i + 4u);

            uint localKey = localKeyFromIndexOffsets(gl_LocalInvocationIndex, idx1, idx2);
            uint diff = localKey - subgroupMinKey;
            localKeyUsedBitMask[diff / 32u] |= 1u << (diff % 32u);
        } else {
            cubeVertexIndices[i + 4u] = 0u;
            vertexKeys[i + 4u] = ~0u;
        }
        if ((edges[voxelKey] & (256u << i)) != 0u) {
            uint idx1 = i;
            uint idx2 = i + 4u;

            cubeVertexIndices[i + 8u] = calculateAndAddVertex(idx1, idx2);

            uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
            vertexKeys[i + 8u] = vtxKey;
            arrayIndexMask |= 1u << (i + 8u);

            uint localKey = localKeyFromIndexOffsets(gl_LocalInvocationIndex, idx1, idx2);
            uint diff = localKey - subgroupMinKey;
            localKeyUsedBitMask[diff / 32u] |= 1u << (diff % 32u);
        } else {
            cubeVertexIndices[i + 8u] = 0u;
            vertexKeys[i + 8u] = ~0u;
        }
    }


#ifdef SUBGROUP_SHARING_OLD
    for (uint i = 0u; i < 12u; i++) {
        cubeVertexIndices[i] = ~0u;
    }

    subgroupBarrier();

    uint outerLoopMask = subgroupOr(arrayIndexMask);
    while (outerLoopMask != 0u) {
        uint i = findLSB(outerLoopMask);
        outerLoopMask &= ~(1u << i);

        uint keyAtI = vertexKeys[i];

        while (true) {
            uint refkey = subgroupMin(keyAtI);
            if (refkey == ~0u)
                break;

            uint vertexIndex = ~0u;
            // The vertices are scalarized and done linearly, so only check the the indices before (and including) this one.
            uint innerLoopMask = subgroupOr(arrayIndexMask) & ((2u << i) - 1u);
            while (innerLoopMask != 0u) {
                uint j = findLSB(innerLoopMask);
                innerLoopMask &= ~(1u << j);

                if (subgroupAny(vertexIndex != ~0u)) {
                    vertexIndex = subgroupMin(vertexIndex);
                    break;
                }

                uint keyAtJ = vertexKeys[j];
                if (keyAtJ != refkey)
                    continue;

                vertexIndex = cubeVertexIndices[j];
            }
            if (keyAtI == refkey) {
                if (vertexIndex != ~0u) {
                    cubeVertexIndices[i] = vertexIndex;
                } else {
                    // Add vertex
                    uint index;
                    if (subgroupElect()) {
                        #ifdef GLOBAL_HASHMAP
                        index = hashmapLookup(refkey);
                        if (index == HashmapEmptyValue) {
                        #endif

                        vec4 vertexPos = vertexPosFromKey(refkey);
                        index = addVertex(vertexPos);

                        #ifdef GLOBAL_HASHMAP
                        hashmapInsert(refkey, index);
                        }
                        #endif
                    }
                    index = subgroupBroadcastFirst(index);

                    cubeVertexIndices[i] = index;
                }
                keyAtI = ~0u;
            }
        }
    }
#endif

subgroupBarrier();

/* We double the resolution of the voxel grid so we can store 'in between' coordinates.
So every invocation can at most store 2 * 2 * 2 vertices. */

for (uint i = 0u; i < localKeyBitMasksCount; i++) {
    localKeyUsedBitMask[i] = subgroupOr(localKeyUsedBitMask[i]);
}

uint workGroupMinKey = localKeyFromOffsets(uvec3(0u), uvec3(0u), uvec3(0u));
uint[2u * 2u * 2u] vtxIndices;
for (uint z = 0u; z < 2u; z++) {
    for (uint y = 0u; y < 2u; y++) {
        for (uint x = 0u; x < 2u; x++) {
            uint localKey = localKeyFromOffsets(gl_LocalInvocationIndex, uvec3(0u, 0u, 0u), uvec3(x, y, z));
            uint diff = localKey - workGroupMinKey;

            if ((localKeyUsedBitMask[diff / 32u] & (1u << (diff % 32u))) == 0u)
                continue;

            uvec3 pos1 = gl_GlobalInvocationID + uvec3(0u);
            uvec3 pos2 = gl_GlobalInvocationID + uvec3(x, y, z);
            uint key = vertexKey(pos1, pos2);

            uint index;
#ifdef GLOBAL_HASHMAP
            index = hashmapLookup(key);
            if (index == HashmapEmptyValue) {
#endif

            vec4 vertexPos = interpolateVertices(pos1, pos2) * vec4(scale, 1.0);
            index = addVertex(vertexPos);

#ifdef GLOBAL_HASHMAP
            hashmapInsert(key, index);
            }
#endif

            vtxIndices[z * 2u * 2u + y * 2u + x] = index;
        }
    }
}

uint outerLoopMask = subgroupOr(arrayIndexMask);
while (outerLoopMask != 0u) {
    uint i = findLSB(outerLoopMask);
    outerLoopMask &= ~(1u << i);


}



for (uint i = 0u; i < (gl_WorkGroupSize.x * gl_WorkGroupSize.y * gl_WorkGroupSize.z) / gl_SubgroupSize; i++) {
    uint mask1 = subgroupOr(localKeyUsedBitMask[i]);
    uint mask2 = gl_SubgroupSize > 32u ? subgroupOr(localKeyUsedBitMask[i + 1u]) : 0u;
    if (mask1 == 0u && mask2 == 0u)
        continue;

    uint mask = gl_SubgroupInvocationID > 32u ? mask2 : mask1;
    if ((mask & (1u << (gl_SubgroupInvocationID % 32u))) != 0u) {

    }
}

barrier();


    for (uint i = 0u; i < 16u && tris[voxelKey][i] != -1; i += 3u) {
        uint firstIndex = atomicAdd(indexCount, 3u);

        uint index0 = cubeVertexIndices[tris[voxelKey][i + 0u]];
        uint index1 = cubeVertexIndices[tris[voxelKey][i + 1u]];
        uint index2 = cubeVertexIndices[tris[voxelKey][i + 2u]];

        indices[firstIndex + 0u] = index0;
        indices[firstIndex + 1u] = index1;
        indices[firstIndex + 2u] = index2;
    }
}