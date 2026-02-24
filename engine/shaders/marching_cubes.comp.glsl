#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0, std430) buffer readonly EdgeTable {
  uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, std430) buffer readonly TriTable {
  int[256u][16u] tris;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2, r32f) uniform readonly image3D densityImage;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 3, std430) buffer verticesBuffer {
    vec4[] vertices;
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

layout(push_constant) uniform Config {
    vec3 scale;
    float threshold;
};

uvec3 indexToCubePos(uint idx) {
    return gl_GlobalInvocationID + uvec3(
        (((~(idx >> 1u) & (idx & 1u)) | ((idx >> 1u) & ~(idx & 1u))) & 1u),
        ((idx >> 2u) & 1u),
        ((idx >> 1u) & 1u)
    );
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
    return vec4(pos1, value1) + a * (vec4(pos2, value2) - vec4(pos1, value1));
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

    if (key == 0u || key == 255u) {
        return;
    }

    instanceCount = 1u;
    firstIndex = 0u;
    vertexOffset = 0;
    firstInstance = 0u;

    uint[12u] cubeVertexIndices;
    for (uint i = 0u; i < 4u; i++) {
        if ((edges[key] & (1u << i)) != 0u) {
            vec4 vertex = interpolateVertices(indexToCubePos(i), indexToCubePos((i + 1u) % 4u)) * vec4(scale, 1.0);
            //vertex = vec4(0.2);
            uint index = atomicAdd(vertexCount, 1u);
            vertices[index] = vertex;
            cubeVertexIndices[i] = index;
        } else {
            cubeVertexIndices[i] = 0u;
        }
        if ((edges[key] & (16u << i)) != 0u) {
            vec4 vertex = interpolateVertices(indexToCubePos(i + 4u), indexToCubePos((i + 1u) % 4u + 4u)) * vec4(scale, 1.0);
            //vertex = vec4(0.5);
            uint index = atomicAdd(vertexCount, 1u);
            vertices[index] = vertex;
            cubeVertexIndices[i + 4u] = index;
        } else {
            cubeVertexIndices[i + 4u] = 0u;
        }
        if ((edges[key] & (256u << i)) != 0u) {
            vec4 vertex = interpolateVertices(indexToCubePos(i), indexToCubePos(i + 4u)) * vec4(scale, 1.0);
            //vertex = vec4(1.0);
            uint index = atomicAdd(vertexCount, 1u);
            vertices[index] = vertex;
            cubeVertexIndices[i + 8u] = index;
        } else {
            cubeVertexIndices[i + 8u] = 0u;
        }
    }

    for (uint i = 0u; i < 16u && tris[key][i] != -1; i += 3u) {
        uint firstIndex = atomicAdd(indexCount, 3u);

        indices[firstIndex] = cubeVertexIndices[tris[key][i]];
        indices[firstIndex + 1u] = cubeVertexIndices[tris[key][i + 1u]];
        indices[firstIndex + 2u] = cubeVertexIndices[tris[key][i + 2u]];
    }
}