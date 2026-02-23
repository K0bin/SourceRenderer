#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable
#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_clustered : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_shuffle : enable

#define HAS_SUBGROUPS

// Needs to emit 3d image size * 2
layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0, std140) buffer readonly EdgeTable {
  uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, std140) buffer readonly TriTable {
  uint[16u][256u] tris;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2, r32f) uniform readonly image3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 3, std430) buffer verticesBuffer {
    vec3[] vertices;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, std430) buffer indicesBuffer {
    uint[] indices;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, std430) buffer bufferatomics {
    uint vertexCount;
    uint indexCount;
};

layout(push_constant) uniform Config {
    float threshold;
    float scale;
};

uvec3 indexToCubePos(uint idx) {
    return uvec3(
        (gl_GlobalInvocationID.x / 2u) + (((~(idx >> 1u) & (idx & 1u)) | ((idx >> 1u) & ~(idx & 1u))) & 1u),
        (gl_GlobalInvocationID.y / 2u) + ((idx >> 2u) & 1u),
        (gl_GlobalInvocationID.z / 2u) + ((idx >> 1u) & 1u)
    );
}

vec3 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    float value1 = imageLoad(densityImage, ivec3(pos1)).x;
    float value2 = imageLoad(densityImage, ivec3(pos2)).x;
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec3(pos1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec3(pos2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return vec3(pos1) + a * (vec3(pos2)- vec3(pos1));
}

void main() {
    uvec3 base = uvec3(gl_GlobalInvocationID.x, gl_GlobalInvocationID.y, gl_GlobalInvocationID.z);

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

    uint[12u] cubeVertexIndices;

#ifdef HAS_SUBGROUPS
    uint i = gl_SubgroupInvocationID % 4u;
    uint j = (gl_SubgroupInvocationID % 12u) / 4u;

    uvec3[3u] idxPos;
    uvec3[3u] idxPos1;
    idxPos[0u] = indexToCubePos(i);
    idxPos[1u] = indexToCubePos(i + 4u);
    idxPos[2u] = idxPos[0u];
    idxPos1[0u] = indexToCubePos((i + 1u) % 4u);
    idxPos1[1u] = indexToCubePos((i + 1u) % 4u);

    uint cubeVertexIndex;
    if (subgroupAny((edges[key] & ((1u << j) << i)) != 0u) && gl_SubgroupInvocationID < 12u) {
        uvec3 usedIdxPos = idxPos[j];
        uvec3 usedIdxPos1 = idxPos1[j];

        vec3 vertex = interpolateVertices(usedIdxPos, usedIdxPos1) * scale;
        uint index = atomicAdd(vertexCount, 1u);
        vertices[index] = vertex;
        cubeVertexIndex = index;
    }
#else
    for (uint i = 0u; i < 4u; i++) {
        if ((edges[key] & (1u << i)) != 0u) {
            vec3 vertex = interpolateVertices(indexToCubePos(i), indexToCubePos((i + 1u) % 4u)) * scale;
            uint index = atomicAdd(vertexCount, 1u);
            vertices[index] = vertex;
            cubeVertexIndices[i] = index;
        }
        if ((edges[key] & (16u << i)) != 0u) {
            vec3 vertex = interpolateVertices(indexToCubePos(i + 4u), indexToCubePos((i + 1u) % 4u + 4u)) * scale;
            uint index = atomicAdd(vertexCount, 1u);
            vertices[index] = vertex;
            cubeVertexIndices[i + 4u] = index;
        }
        if ((edges[key] & (256u << i)) != 0u) {
            vec3 vertex = interpolateVertices(indexToCubePos(i), indexToCubePos(i + 4u)) * scale;
            uint index = atomicAdd(vertexCount, 1u);
            vertices[index] = vertex;
            cubeVertexIndices[i + 8u] = index;
        }
    }
#endif

#ifdef HAS_SUBGROUPS
    for (uint i = 0u; tris[key][i] != -1; i++) {
        indices[atomicAdd(indexCount, 1u)] = subgroupShuffle(cubeVertexIndex, tris[key][i]);
        indices[atomicAdd(indexCount, 1u)] = subgroupShuffle(cubeVertexIndex, tris[key][i + 1u]);
        indices[atomicAdd(indexCount, 1u)] = subgroupShuffle(cubeVertexIndex, tris[key][i + 2u]);
    }
#else
    for (uint i = 0u; tris[key][i] != -1; i++) {
        indices[atomicAdd(indexCount, 1u)] = cubeVertexIndices[tris[key][i]];
        indices[atomicAdd(indexCount, 1u)] = cubeVertexIndices[tris[key][i + 1u]];
        indices[atomicAdd(indexCount, 1u)] = cubeVertexIndices[tris[key][i + 2u]];
    }
#endif
}