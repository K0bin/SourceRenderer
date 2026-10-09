#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

#extension GL_EXT_mesh_shader : require

#extension GL_EXT_scalar_block_layout : enable

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_shuffle : enable
#extension GL_EXT_maximal_reconvergence : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(local_size_x = 32, local_size_y = 0, local_size_z = 0) in;

#include "descriptor_sets.inc.glsl"
#include "frame_set_common.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0, std430) uniform EdgeTable {
    uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, std430) uniform TriTable {
    int[256u][17u] tris;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform texture3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 3, std430) buffer indicesBuffer {
    uint[] indices;
} indicesBuffers[16u];

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, scalar) uniform thresholds {
    float[16u] minThresholds;
};


layout(set = DESCRIPTOR_SET_FREQUENT, binding = 8) uniform texture3D densityImageMin;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 9) uniform texture3D densityImageMax;

layout(push_constant, std430) uniform Config {
    uvec3 lodExtents;
    uint lod;
    uvec3 minBox;
    uint thresholdsCount;
};


// Nvidia recommends up to 64 vertices and 126 primitives.
// AMD implements mesh shaders on primitive shaders and thus recommends 1:1 primitives and threads
const uint vertexSlotCount = 64u;
layout(triangles, max_vertices = vertexSlotCount, max_primitives = 32u) out;

shared uint shared_primitiveCount;

struct TaskPayload {
    uint8_t voxelKeys[32];
    uvec4 workgroupBase; // TODO: turn into morton code
};
taskPayloadSharedEXT TaskPayload payload;









uvec3 indexOffset(uint idx) {
    return uvec3(
            ((idx >> 1u) ^ idx) & 1u,
            (idx >> 2u) & 1u,
            (idx >> 1u) & 1u
    );
}

uint vertexKey(uvec3 pos1, uvec3 pos2) {
    uvec3 pos = pos1 + pos2;

    uvec3 sizes = uvec3(512u * 2u + 1u);
    pos = min(sizes - uvec3(1u), pos);

    uint key = pos.z * sizes.x * sizes.y +
    pos.y * sizes.x +
    pos.x;

    return key;
}

uint vertexKeyFromIndexOffsets(uint idx1, uint idx2) {
    uvec3 workgroupBase = payload.workgroupBase.xyz;
    uvec3 base = workgroupBase + gl_LocalInvocationID;
    uvec3 vertexPos1 = base + indexOffset(idx1);
    uvec3 vertexPos2 = base + indexOffset(idx2);
    uint vtxKey = vertexKey(vertexPos1, vertexPos2);
    return vtxKey;
}

uint buildVertexKey(uint index) {
    // Naming of those two is confusing because I named them when I translated the loop and built the array index
    // from the loop index.
    // This function goes the other way around (array index -> loop index).
    // Code from the loop variant:
    // uint iDiv3 = i / 3u;
    // uint iMod3 = i % 3u;
    // uint index = iDiv3 + iMod3 * 4u;
    uint iMod3 = index / 4u;
    uint iDiv3 = index % 4u;

    uint idx1 = iDiv3 + uint(iMod3 == 1u) * 4u;
    uint idx2 = (iDiv3 + uint(iMod3 != 2u)) % 4u + uint(iMod3 != 0u) * 4u;

    uint vtxKey = vertexKeyFromIndexOffsets(idx1, idx2);
    return vtxKey;
}

const uint maxIndices = (512u * 512u * 512u) / 100u * 15u;


layout(constant_id = 1) const bool renderDebugCube = false;
const uvec3 cubePositions[8] = uvec3[8](
        uvec3(0, 0, 0), uvec3(1, 0, 0),
        uvec3(1, 1, 0), uvec3(0, 1, 0),
        uvec3(0, 0, 1), uvec3(1, 0, 1),
        uvec3(1, 1, 1), uvec3(0, 1, 1)
);
const uvec3 cubeIndices[12] = uvec3[12](
        uvec3(2, 1, 0), uvec3(0, 3, 2), // Front
        uvec3(6, 5, 1), uvec3(1, 2, 6), // Right
        uvec3(7, 4, 5), uvec3(5, 6, 7), // Back
        uvec3(3, 0, 4), uvec3(4, 7, 3), // Left
        uvec3(6, 2, 3), uvec3(3, 7, 6), // Top
        uvec3(1, 5, 4), uvec3(4, 0, 1)  // Bottom
);

void main() {
    //gl_SubgroupID

    uint indexCount;
    if (!renderDebugCube) {
        indexCount = tris[voxelKey][0u];
        indexCount = min(indexCount, 15u);

        if (indexCount == 0u) {
            if (gl_LocalInvocationIndex == 0u)
                SetMeshOutputsEXT(vertexSlotCount, min(maxPrimitives, totalPrimitiveCount));

            return;
        }

    } else {
        indexCount = 12u * 3u;
    }

    uint subgroupTotalIndices = subgroupAdd(indexCount);

    subgroupFirstIndex = subgroupBroadcastFirst(subgroupFirstIndex);
    uint firstIndex = subgroupFirstIndex + subgroupExclusiveAdd(indexCount);

    uint voxelKey = payload.voxelKeys[gl_SubgroupID];

    uint triangleIndex = gl_WorkGroupID.x;
    uint baseIndex = triangleIndex * 3u;
    if (!renderDebugCube) {
        indicesBuffers[j].indices[firstIndex + baseIndex + 0u] = buildVertexKey(tris[voxelKey][1u + baseIndex + 0u]);
        indicesBuffers[j].indices[firstIndex + baseIndex + 1u] = buildVertexKey(tris[voxelKey][1u + baseIndex + 1u]);
        indicesBuffers[j].indices[firstIndex + baseIndex + 2u] = buildVertexKey(tris[voxelKey][1u + baseIndex + 2u]);

        vertexOut[slot].out_densityMapUV = (pos + 0.5) / densityMapSize;
        vertexOut[slot].out_worldPosition = worldPos.xyz;
        vertexOut[slot].out_density = posAndDensity.w;
        gl_MeshVerticesEXT[slot].gl_Position = camera.viewProj * worldPos;

        gl_PrimitiveTriangleIndicesEXT[firstPrimitive + i] = indices;


    } else {
        uvec3 vtx = cubePositions[cubeIndices[triangleIndex][0]] + payload.workgroupBase.xyz + gl_LocalInvocationID;

        for (uint i = 0u; i < 12u * 3u && firstIndex + indexCount < maxIndices; i++) {
            indicesBuffers[j].indices[firstIndex + i] = vertexKey(vtx, vtx);
        }
    }

    // SetMeshOutputsEXT(vertexSlotCount, min(maxPrimitives, totalPrimitiveCount));

}
