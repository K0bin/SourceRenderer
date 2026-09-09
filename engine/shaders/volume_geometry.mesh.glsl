#version 450
#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_mesh_shader : require
// #extension GL_EXT_debug_printf : enable

#extension GL_EXT_scalar_block_layout : enable

#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_ballot : enable
#extension GL_KHR_shader_subgroup_shuffle : enable
#extension GL_EXT_maximal_reconvergence : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

// Pretty much everything that can do mesh shaders is wave32. (RDNA2+ can do wave32)
layout(local_size_x = 4, local_size_y = 4, local_size_z = 2) in;

// Nvidia recommends up to 64 vertices and 126 primitives.
const uint vertexSlotCount = 64u;
const uint maxPrimitives = 126u;
layout(triangles, max_vertices = vertexSlotCount, max_primitives = maxPrimitives) out;

layout(location = 0) out VertexData {
    float out_density;
    vec3 out_worldPosition;
    vec3 out_densityMapUV;
} vertexOut[];

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform texture3D densityImage;

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 5) uniform sampler linearSampler;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 6) uniform sampler nearestSampler;


layout(set = DESCRIPTOR_SET_FREQUENT, binding = 7, std430) uniform EdgeTable {
  uint[256u] edges;
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 8, std430) uniform TriTable {
  int[256u][17u] tris;
};

layout(push_constant, std430) uniform Config {
    mat4 modelMat;
    uvec3 lodExtents;
    float threshold;
    uint lod;
    uint _padding0;
    uint _padding1;
    uint _padding2;
};

shared uint shared_primitiveCount;

uvec3 indexOffset(uint idx) {
    return uvec3(
         ((idx >> 1u) ^ idx) & 1u,
         (idx >> 2u) & 1u,
         (idx >> 1u) & 1u
    );
}

vec4 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    vec3 imgSize = vec3(lodExtents);
    float value1 = textureLod(sampler3D(densityImage, linearSampler), (vec3(pos1) + vec3(0.5)) / imgSize, lod).x;
    float value2 = textureLod(sampler3D(densityImage, linearSampler), (vec3(pos2) + vec3(0.5)) / imgSize, lod).x;
    if (value1 < threshold && value2 < threshold) {
        return vec4(0.0);
    }
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec4(vec3(pos1), value1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec4(vec3(pos2), value2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return mix(vec4(pos1, value1), vec4(pos2, value2), a);
}

const uvec3 localKeySizes = gl_WorkGroupSize * 2u + uvec3(1u);
vec4 vertexPosFromLocalKey(uint vertexKey) {
    uvec3 pos = uvec3(vertexKey % localKeySizes.x,
        (vertexKey / localKeySizes.x) % localKeySizes.y,
        vertexKey / (localKeySizes.x * localKeySizes.y));

    uvec3 workgroupBase = gl_WorkGroupID * gl_WorkGroupSize;
    uvec3 pos1 = workgroupBase + pos / 2u;
    uvec3 pos2 = pos1 + (pos % 2u);

    return interpolateVertices(pos1, pos2);
}

uint buildLocalVertexKey(uint marchingCubesOffsetIndex) {
    // Naming of those two is confusing because I named them when I translated the loop and built the array index
    // from the loop index.
    // This function goes the other way around (array index -> loop index).
    // Code from the loop variant:
    // uint iDiv3 = i / 3u;
    // uint iMod3 = i % 3u;
    // uint index = iDiv3 + iMod3 * 4u;
    uint iMod3 = marchingCubesOffsetIndex / 4u;
    uint iDiv3 = marchingCubesOffsetIndex % 4u;

    uint idx1 = iDiv3 + uint(iMod3 == 1u) * 4u;
    uint idx2 = (iDiv3 + uint(iMod3 != 2u)) % 4u + uint(iMod3 != 0u) * 4u;
    uvec3 pos1 = indexOffset(idx1);
    uvec3 pos2 = indexOffset(idx2);

    uvec3 pos = gl_LocalInvocationID * 2u + pos1 + pos2;
    pos = min(localKeySizes - uvec3(1u), pos);
    return pos.z * localKeySizes.x * localKeySizes.y +
         pos.y * localKeySizes.x +
         pos.x;
}


const uint vertexSlotUnused = ~0u;
shared uint vertexKeys[vertexSlotCount];

uint hash(uint key) {
    uint hash = key;
    hash ^= hash >> 16;
    hash *= 0x85ebca6b;
    hash ^= hash >> 13;
    hash *= 0xc2b2ae35;
    hash ^= hash >> 16;
    return hash;
}

void insertWorkgroupVertex(uint vertexKey, uint slot) {
    if (slot >= vertexSlotCount)
        return;

    vec3 densityMapSize = vec3(lodExtents);
    vec4 posAndDensity = vertexPosFromLocalKey(vertexKey);
    vec3 pos = posAndDensity.xyz;
    vec4 worldPos = modelMat * vec4(pos, 1.0); // w of pos is density, we need 1.0 here.
    vertexOut[slot].out_densityMapUV = (pos + 0.5) / densityMapSize;
    vertexOut[slot].out_worldPosition = worldPos.xyz;
    vertexOut[slot].out_density = posAndDensity.w;
    gl_MeshVerticesEXT[slot].gl_Position = camera.viewProj * worldPos;
}

uint addVertex(uint key) {
    uint slot = hash(key) % vertexSlotCount;
    uint startSlot = slot;

    while (true) {
        uint prev = atomicCompSwap(vertexKeys[slot], vertexSlotUnused, key);
        if (prev == key) {
            return slot;
        }
        if (prev == vertexSlotUnused) {
            insertWorkgroupVertex(key, slot);
            return slot;
        }
        slot = (slot + 1u) % vertexSlotCount;
        if (slot == startSlot) {
            break;
        }
    }
    return 0u;
}
void initVertexHashmap() {
    for (uint i = 0u; i < vertexSlotCount; i++) {
        vertexKeys[i] = vertexSlotUnused;
    }
}


void debugWorkgroupCube();

void main() [[maximally_reconverges]] {
    //debugWorkgroupCube();
    //return;

    if (gl_LocalInvocationIndex == 0u) {
        shared_primitiveCount = 0u;
        initVertexHashmap();
    }

    barrier();

    uvec3 workgroupBase = gl_WorkGroupID * gl_WorkGroupSize;
    uvec3 base = workgroupBase + gl_LocalInvocationID;

    if (gl_SubgroupSize == gl_WorkGroupSize.x * gl_WorkGroupSize.y * gl_WorkGroupSize.z
        && subgroupAll(any(greaterThanEqual(base + uvec3(1u), lodExtents)))) {
        if (subgroupElect())
            SetMeshOutputsEXT(0u, 0u);

        return;
    }

    uint voxelKey = 0u;
    for (uint z = 0u; z < 2u; z++) {
        for (uint y = 0u; y < 2u; y++) {
            for (uint x = 0u; x < 2u; x++) {
                uvec3 offset = uvec3(x, y, z);

                uvec3 pos = base + offset;
                float density = texelFetch(sampler3D(densityImage, nearestSampler), ivec3(pos), int(lod)).x;

                uint index = ((x + z) & 1u) + z * 2u + y * 4u;

                bool passes = density >= threshold;
                voxelKey |= uint(passes) << index;
            }
        }
    }

    if (gl_SubgroupSize == gl_WorkGroupSize.x * gl_WorkGroupSize.y * gl_WorkGroupSize.z
        && subgroupAll(voxelKey == 0u || voxelKey == 255u)) {
        if (subgroupElect())
            SetMeshOutputsEXT(0u, 0u);

        return;
    }

    uint indexCount = tris[voxelKey][0u];
    indexCount = min(indexCount, 15u);
    uint primitiveCount = indexCount / 3;

    uint firstPrimitive = 0u;
    if (indexCount != 0u)
        firstPrimitive = atomicAdd(shared_primitiveCount, primitiveCount);

    uint totalPrimitiveCount;

    if (gl_SubgroupSize == gl_WorkGroupSize.x * gl_WorkGroupSize.y * gl_WorkGroupSize.z)
        totalPrimitiveCount = subgroupAdd(primitiveCount);
    else {
        barrier();
        totalPrimitiveCount = atomicAdd(shared_primitiveCount, 0u);
    }

    if (gl_LocalInvocationIndex == 0u)
        SetMeshOutputsEXT(vertexSlotCount, min(maxPrimitives, totalPrimitiveCount));

    barrier();

    for (uint i = 0u; i < primitiveCount && firstPrimitive + primitiveCount <= maxPrimitives; i++) {
        uvec3 indices;
        for (uint j = 0u; j < 3u; j++) {
            uint vtxKey = buildLocalVertexKey(tris[voxelKey][1u + i * 3u + j]);
            indices[j] = addVertex(vtxKey);
        }

        gl_PrimitiveTriangleIndicesEXT[firstPrimitive + i] = indices;
    }
}

// Debug code

const vec3 positions[8] = vec3[8](
    vec3(-1.0, -1.0, -1.0), vec3( 1.0, -1.0, -1.0),
    vec3( 1.0,  1.0, -1.0), vec3(-1.0,  1.0, -1.0),
    vec3(-1.0, -1.0,  1.0), vec3( 1.0, -1.0,  1.0),
    vec3( 1.0,  1.0,  1.0), vec3(-1.0,  1.0,  1.0)
);
const uvec3 indices[12] = uvec3[12](
    uvec3(0, 1, 2), uvec3(2, 3, 0), // Front
    uvec3(1, 5, 6), uvec3(6, 2, 1), // Right
    uvec3(5, 4, 7), uvec3(7, 6, 5), // Back
    uvec3(4, 0, 3), uvec3(3, 7, 4), // Left
    uvec3(3, 2, 6), uvec3(6, 7, 3), // Top
    uvec3(4, 5, 1), uvec3(1, 0, 4)  // Bottom
);
void debugWorkgroupCube() {
    if (gl_LocalInvocationIndex == 0) {
        SetMeshOutputsEXT(8, 12);
    }
    if (gl_LocalInvocationIndex < 8) {
        uvec3 workgroupBase = gl_WorkGroupID * gl_WorkGroupSize;

        vec3 pos = positions[gl_LocalInvocationIndex];
        pos *= 0.5;
        pos += 0.5;
        pos *= vec3(gl_WorkGroupSize);
        pos += vec3(workgroupBase);

        vec4 worldPos = modelMat * vec4(pos.xyz, 1.0); // w of pos is density, we need 1.0 here.
        gl_MeshVerticesEXT[gl_LocalInvocationIndex].gl_Position = camera.viewProj * worldPos;
        vertexOut[gl_LocalInvocationIndex].out_worldPosition = worldPos.xyz;
        vertexOut[gl_LocalInvocationIndex].out_density = float((gl_WorkGroupID.x % 2u) ^ (gl_WorkGroupID.y % 2u) ^ (gl_WorkGroupID.z % 2u));
        gl_MeshVerticesEXT[gl_LocalInvocationIndex].gl_Position = camera.viewProj * worldPos;
    }
    if (gl_LocalInvocationIndex < 12) {
        uvec3 primitiveIndices = indices[gl_LocalInvocationIndex];
        primitiveIndices = uvec3(primitiveIndices.z, primitiveIndices.y, primitiveIndices.x);
        gl_PrimitiveTriangleIndicesEXT[gl_LocalInvocationIndex] = primitiveIndices;
    }
}
