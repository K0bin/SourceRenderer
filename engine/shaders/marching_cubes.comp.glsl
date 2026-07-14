#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_shader_explicit_arithmetic_types_float16 : enable

#define CACHE 1

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable

#extension GL_KHR_memory_scope_semantics : enable


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

uvec3 localInvocationFromIndex(uint index) {
    return uvec3(
        index % gl_WorkGroupSize.x,
        (index / gl_WorkGroupSize.x) % gl_WorkGroupSize.y,
        index / (gl_WorkGroupSize.y * gl_WorkGroupSize.z)
    );
}


uint localVertexKey(uint idx1, uint idx2) {
    uint minIdx = min(idx1, idx2);
    uint maxIdx = max(idx1, idx2);
    uint diff = maxIdx - minIdx;
    // Max value: 4. That's 3 bits just like the original value.
    // Min value: 1. So we can subtract that. Max value 3 only takes 2 bits.
    // Possible values: 4, 3, 1
    diff -= 1;

    return (gl_LocalInvocationIndex << 5u) | (diff << 3u) | minIdx;
}

uvec3 indexOffset(uint idx) {
    return uvec3(
         ((idx >> 1u) ^ idx) & 1u,
         (idx >> 2u) & 1u,
         (idx >> 1u) & 1u
    );
}

uint offsetToIndex(uvec3 offset) {
    uint index = 0u;
    index |= min(offset.z, 1u) << 1u;
    index |= min(offset.y, 1u) << 2u;
    index |= min(offset.x, 1u) & min(offset.z, 1u);
    return index;
}

uint vertexKey(uint idx1, uint idx2) {
    uint minIdx = min(idx1, idx2);
    uint maxIdx = max(idx1, idx2);

    uvec3 minPos = indexOffset(minIdx);
    uvec3 maxPos = indexOffset(maxIdx);

    ivec3 diff = ivec3(maxPos) - ivec3(minPos);

    uvec3 invocationCount = gl_NumWorkGroups * gl_WorkGroupSize;
    uvec3 invocation = min(invocationCount, gl_GlobalInvocationID + minPos);

    uint baseIndex = invocation.z * invocationCount.x * invocationCount.y +
         invocation.y * invocationCount.x +
         invocation.x;

    uint key = baseIndex << 8u; // 24 Bits for the position. Works up to 256x256x256!
    key |= (uint(diff.x < 0 ? (diff.x * -1) : (diff.x | 4)) & 7) << 5u; // Could be optimized with unreadable bit operations.
    key |= (uint(diff.z < 0 ? (diff.z * -1) : (diff.z | 4)) & 7) << 2u;
    key |= uint(diff.y & 3); // diff.y can never be negative

    return key;
}

uvec3 indexToCubePos(uint idx) {
    return gl_GlobalInvocationID + indexOffset(idx);
}

uvec3 indexToCubePos(uvec3 localInvocation, uint idx) {
      return gl_WorkGroupSize * gl_WorkGroupID + localInvocation + indexOffset(idx);
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

uint calculateAndAddVertex(uint idx1, uint idx2) {
    uint minIdx = min(idx1, idx2);
    uint maxIdx = max(idx1, idx2);

    uvec3 vertexPos1 = indexToCubePos(minIdx);
    uvec3 vertexPos2 = indexToCubePos(maxIdx);
    vec4 vertex = interpolateVertices(vertexPos1, vertexPos2) * vec4(scale, 1.0);

    uint index = atomicAdd(vertexCount, 1u);
    vertices[index].pos = f16vec3(vertex.xyz);
    return index;
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
const uint HashmapMaxProbing = 0u; // 0 to disable the limit.

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
        uint prev = atomicCompSwap(hashmapEntries[slot].key, HashmapEmptyKey, key, gl_ScopeQueueFamily,
            gl_StorageSemanticsBuffer, gl_SemanticsAcquireRelease | gl_SemanticsMakeAvailable,
            gl_StorageSemanticsNone, gl_SemanticsRelaxed);
        //uint prev = atomicCompSwap(hashmapEntries[slot].key, HashmapEmptyKey, key);
        if (prev == key || prev == HashmapEmptyKey) {
            atomicStore(hashmapEntries[slot].vertexIndex, vertexIndex, gl_ScopeQueueFamily, gl_StorageSemanticsBuffer, gl_SemanticsRelease | gl_SemanticsMakeAvailable);
            //atomicExchange(hashmapEntries[slot].vertexIndex, vertexIndex);
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
       //uint slotKey = atomicAdd(hashmapEntries[slot].key, 0u);
       uint slotKey = atomicLoad(hashmapEntries[slot].key, gl_ScopeQueueFamily, gl_StorageSemanticsBuffer, gl_SemanticsAcquire | gl_SemanticsMakeVisible);
       if (slotKey == key) {
           //return atomicAdd(hashmapEntries[slot].vertexIndex, 0u);
           return atomicLoad(hashmapEntries[slot].vertexIndex, gl_ScopeQueueFamily, gl_StorageSemanticsBuffer, gl_SemanticsAcquire | gl_SemanticsMakeVisible);
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

void main() {
    uvec3 base = gl_GlobalInvocationID;

    ivec3 imgSize = imageSize(densityImage);
    uint key = 0u;
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uint index = ((x + z) & 1u) + z * 2u + y * 4u;
                ivec3 pos = ivec3(int(base.x + x), int(base.y + y), int(base.z + z));

                float value = imageLoad(densityImage, pos).x;
                bool inRange = gl_GlobalInvocationID.x < imgSize.x - 1u
                    && gl_GlobalInvocationID.y < imgSize.y - 1u
                    && gl_GlobalInvocationID.z < imgSize.z - 1u;
                key |= ((value >= threshold && inRange) ? 1u : 0u) << index;
            }
        }
    }

    bool skipInvocation = key == 0u || key == 255u;

    if (subgroupAll(skipInvocation))
        return;

    instanceCount = 1u;
    firstIndex = 0u;
    vertexOffset = 0;
    firstInstance = 0u;

    uint minKey = ~0u;
    uint maxKey = 0u;
    uint usedIndices = 0u;
    uint[12u] cubeVertexIndices;
    for (uint i = 0u; i < 4u; i++) {
        if ((edges[key] & (1u << i)) != 0u) {
            uint idx1 = i;
            uint idx2 = (i + 1u) % 4u;
            uint key = localVertexKey(idx1, idx2);
            cubeVertexIndices[i] = key | (1u << 31u);
            minKey = min(minKey, key);
            maxKey = max(maxKey, key);
            usedIndices |= 1u << i;
        } else {
            cubeVertexIndices[i] = 0u;
        }
        if ((edges[key] & (16u << i)) != 0u) {
            uint idx1 = i + 4u;
            uint idx2 = (i + 1u) % 4u + 4u;
            uint key = localVertexKey(idx1, idx2);
            cubeVertexIndices[i + 4u] = key | (1u << 31u);
            minKey = min(minKey, key);
            maxKey = max(maxKey, key);
            usedIndices |= 1u << (i + 4u);
        } else {
            cubeVertexIndices[i + 4u] = 0u;
        }
        if ((edges[key] & (256u << i)) != 0u) {
            uint idx1 = i;
            uint idx2 = i + 4u;
            uint key = localVertexKey(idx1, idx2);
            cubeVertexIndices[i + 8u] = key | (1u << 31u);
            minKey = min(minKey, key);
            maxKey = max(maxKey, key);
            usedIndices |= 1u << (i + 8u);
        } else {
            cubeVertexIndices[i + 8u] = 0u;
        }
    }

#if CACHE == 2
    // The subgroup based cache has a terrible hit rate.
    // I probably need to change the addressing to go over the image in subgroup sized cubes.
    // Right now I just use the addressing done by the driver (global/local invocation id).
    // I wonder if this works better on older AMD GPUs with warp64.

    subgroupBarrier();

    uint uniformIndicesMask = subgroupOr(usedIndices);
    int lsb = findLSB(uniformIndicesMask);
    while (lsb != -1) {
        uint key = cubeVertexIndices[lsb];
        uint sharedKey = subgroupMax(key);

        // As long as any invocation has a key in this array slot, all invocations will search their arrays for usages of this key.
        while ((sharedKey & (1u << 31u)) != 0) {
            int indicesIndex = -1;
            uint innerMask = usedIndices;
            int innerLsb = findLSB(innerMask);
            while (innerLsb != -1) {
                if (cubeVertexIndices[innerLsb] == sharedKey) {
                    indicesIndex = innerLsb;
                    innerMask = 0u;
                }

                innerMask &= ~(1u << innerLsb);
                innerLsb = findLSB(innerMask);
            }

            if (indicesIndex != -1) {
                // The invocation found the key in their array.
                // Pick an invocation that adds it to the vertex buffer.

                uint index;
                if (subgroupElect()) {
                    uint appendKey = sharedKey;
                    appendKey &= ~(1u << 31u);
                    uint minIdx = appendKey & 7u; // 7u = 0b111
                    uint maxIdx = ((appendKey >> 3u) & 3u) + minIdx + 1u; // 3u = 0b11
                    uint localIndex = appendKey >> 5u;
                    uvec3 localInvocation = localInvocationFromIndex(localIndex);

                    uvec3 vertexPos1 = indexToCubePos(localInvocation, minIdx);
                    uvec3 vertexPos2 = indexToCubePos(localInvocation, maxIdx);
                    vec4 vertex = interpolateVertices(vertexPos1, vertexPos2) * vec4(scale, 1.0);

                    index = atomicAdd(vertexCount, 1u);
                    vertices[index].pos = f16vec3(vertex.xyz);
                }
                index = subgroupBroadcastFirst(index);
                cubeVertexIndices[indicesIndex] = index;
                key = index;
            }

            sharedKey = subgroupMax(key);
        }

        uniformIndicesMask &= ~(1u << lsb);
        lsb = findLSB(uniformIndicesMask);
    }
#else
    int lsb = findLSB(usedIndices);
    while (lsb != -1) {
        uint key = cubeVertexIndices[lsb] & ~(1u << 31u);
        uint minIdx = key & 7u; // 7u = 0b111
        uint maxIdx = ((key >> 3u) & 3u) + minIdx + 1u; // 3u = 0b11
        uint localIndex = key >> 5u;
        uvec3 localInvocation = localInvocationFromIndex(localIndex);

        uvec3 vertexPos1 = indexToCubePos(localInvocation, minIdx);
        uvec3 vertexPos2 = indexToCubePos(localInvocation, maxIdx);
        vec4 vertex = interpolateVertices(vertexPos1, vertexPos2) * vec4(scale, 1.0);

        // Skipped invocations never end up here because the edge lookup table value for 0 or 255 is 0.
        // See the loop above.

        uint index;
        #if CACHE == 1
        uint cacheKey = vertexKey(minIdx, maxIdx);
        index = hashmapLookup(cacheKey);
        if (index == HashmapEmptyValue) {
        #endif

        index = atomicAdd(vertexCount, 1u);
        vertices[index].pos = f16vec3(vertex.xyz);

        #if CACHE == 1
        hashmapInsert(cacheKey, index);
        }
        #endif

        cubeVertexIndices[lsb] = index;

        usedIndices &= ~(1u << lsb);
        lsb = findLSB(usedIndices);
    }
#endif

    if (skipInvocation)
        return;

    for (uint i = 0u; i < 16u && tris[key][i] != -1; i += 3u) {
        uint firstIndex = atomicAdd(indexCount, 3u);

        uint index0 = cubeVertexIndices[tris[key][i + 0u]];
        uint index1 = cubeVertexIndices[tris[key][i + 1u]];
        uint index2 = cubeVertexIndices[tris[key][i + 2u]];

        indices[firstIndex + 0u] = index0;
        indices[firstIndex + 1u] = index1;
        indices[firstIndex + 2u] = index2;
    }
}