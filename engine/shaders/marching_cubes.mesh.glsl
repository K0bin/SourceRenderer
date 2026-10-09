#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable
#extension GL_EXT_shader_explicit_arithmetic_types : enable

#extension GL_EXT_mesh_shader : require

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

// Nvidia recommends up to 64 vertices and 126 primitives.
// AMD implements mesh shaders on primitive shaders and thus recommends 1:1 primitives and threads

// 32 primitives * 3 vertices = 96
const uint vertexCount = 96u;
layout(triangles, max_vertices = vertexCount, max_primitives = 32u) out;

layout(location = 0) out VertexData {
    float out_density;
    vec3 out_worldPosition;
    vec3 out_densityMapUV;
} vertexOut[];


struct TaskPayload {
    uint8_t[32] voxelKeys;
    uvec3 workgroupBasePos; // TODO: turn into morton code
};
taskPayloadSharedEXT TaskPayload payload;


vec4 interpolateVertices(uvec3 pos1, uvec3 pos2) {
    vec3 fpos1 = vec3(pos1) + 0.5;
    vec3 fpos2 = vec3(pos2) + 0.5;
    float value1 = texelFetch(sampler3D(densityMap, samplerNearest), ivec3(pos1), int(lod)).x;
    float value2 = texelFetch(sampler3D(densityMap, samplerNearest), ivec3(pos2), int(lod)).x;
    if (abs(value1 - threshold) < 0.00001 || abs(value1 - value2) < 0.00001) {
        return vec4(fpos1, value1);
    }
    if (abs(value2 - threshold) < 0.00001) {
        return vec4(fpos2, value2);
    }
    float a = (threshold - value1) / (value2 - value1);
    return mix(vec4(fpos1, value1), vec4(fpos2, value2), a);
    //return (vec4(fpos1, value1) + vec4(fpos2, value2)) / 2.0; // debug with simple average
}


vec3 calculateNormal(vec3 densityMapUV, uint normalLod) {
    vec3 normal = vec3(0.0);

    vec3 imgSize = vec3(lodExtents);
    vec3 singlePixel = vec3(1.0) / imgSize;

    normal.x = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - vec3(singlePixel.x, 0, 0), normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + vec3(singlePixel.x, 0, 0), normalLod).x;
    normal.y = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - vec3(0, singlePixel.y, 0), normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + vec3(0, singlePixel.y, 0), normalLod).x;
    normal.z = textureLod(sampler3D(densityMap, samplerLinear), densityMapUV - vec3(0, 0, singlePixel.z), normalLod).x
    - textureLod(sampler3D(densityMap, samplerLinear), densityMapUV + vec3(0, 0, singlePixel.z), normalLod).x;
    return normalize(normal);
}

uvec3 indexOffset(uint idx) {
    return uvec3(
            ((idx >> 1u) ^ idx) & 1u,
            (idx >> 2u) & 1u,
            (idx >> 1u) & 1u
    );
}

vec4 vertexPosFromIndexOffsets(uvec3 voxelPosition, uint idx1, uint idx2) {
    uvec3 vertexPos1 = voxelPosition + indexOffset(idx1);
    uvec3 vertexPos2 = voxelPosition + indexOffset(idx2);
    return interpolateVertices(vertexPos1, vertexPos2);
}

vec4 vertexPos(uvec3 voxelPosition, uint triLookupTableValue) {
    // Naming of those two is confusing because I named them when I translated the loop and built the array index
    // from the loop index.
    // This function goes the other way around (array index -> loop index).
    // Code from the loop variant:
    // uint iDiv3 = i / 3u;
    // uint iMod3 = i % 3u;
    // uint triLookupTableValue = iDiv3 + iMod3 * 4u;
    uint iMod3 = triLookupTableValue / 4u;
    uint iDiv3 = triLookupTableValue % 4u;

    uint idx1 = iDiv3 + uint(iMod3 == 1u) * 4u;
    uint idx2 = (iDiv3 + uint(iMod3 != 2u)) % 4u + uint(iMod3 != 0u) * 4u;
    return vertexPosFromIndexOffsets(voxelPosition, idx1, idx2);
}

void writeVertex(uvec3 voxelPosition, uint voxelKey, uint outFirstVertexIndex, uint triangleVertexIndex) {
    uint inputPrimitiveIndex = gl_WorkGroupID.x;
    uint baseIndex = inputPrimitiveIndex * 3u;

    uint lookupValue = tris[voxelKey][1u + baseIndex + triangleVertexIndex];
    vec4 posAndDensity = vertexPos(voxelPosition, lookupValue);
    vec3 pos = posAndDensity.xyz;
    float density = posAndDensity.w;

    vec4 worldPos = model * vec4(pos, 1.0);

    uint outIndex = outFirstVertexIndex + triangleVertexIndex;
    vertexOut[outIndex].out_densityMapUV = pos / vec3(lodExtents);
    vertexOut[outIndex].out_worldPosition = worldPos.xyz;
    vertexOut[outIndex].out_density = density;
    gl_MeshVerticesEXT[outIndex].gl_Position = camera.viewProj * worldPos;
}


layout(constant_id = 0) const bool renderDebugCube = false;
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
void writeCubeVertex(uvec3 voxelPosition, uint outFirstVertexIndex, uint triangleVertexIndex) {
    uint inputPrimitiveIndex = gl_WorkGroupID.x;
    uint baseIndex = inputPrimitiveIndex * 3u;

    uvec3 intVtx = cubePositions[cubeIndices[inputPrimitiveIndex][triangleVertexIndex]] + voxelPosition;
    vec3 pos = vec3(intVtx);

    vec4 worldPos = model * vec4(pos, 1.0);

    uint outIndex = outFirstVertexIndex + triangleVertexIndex;
    vertexOut[outIndex].out_densityMapUV = pos / vec3(lodExtents);
    vertexOut[outIndex].out_worldPosition = worldPos.xyz;
    vertexOut[outIndex].out_density = 1.0;
    gl_MeshVerticesEXT[outIndex].gl_Position = camera.viewProj * worldPos;
}


void main() {
    uint voxelKey = payload.voxelKeys[gl_LocalInvocationIndex];
    bool hasGeometry = voxelKey != 0u && voxelKey != 255u;
    hasGeometry = hasGeometry && (renderDebugCube || tris[voxelKey][0] > gl_WorkGroupID.x * 3u);
    uint primitiveIndex = subgroupExclusiveAdd(uint(hasGeometry));

    uint totalPrimitiveCount = subgroupAdd(uint(hasGeometry));
    SetMeshOutputsEXT(totalPrimitiveCount * 3u, totalPrimitiveCount);

    uvec3 voxelPosition = payload.workgroupBasePos.xyz + gl_LocalInvocationID;
    if (hasGeometry) {
        uint firstVertex = primitiveIndex * 3u;
        gl_PrimitiveTriangleIndicesEXT[primitiveIndex] = uvec3(firstVertex, firstVertex + 1u, firstVertex + 2u);
        if (!renderDebugCube) {
            writeVertex(voxelPosition, voxelKey, firstVertex, 0u);
            writeVertex(voxelPosition, voxelKey, firstVertex, 1u);
            writeVertex(voxelPosition, voxelKey, firstVertex, 2u);
        } else {
            writeCubeVertex(voxelPosition, firstVertex, 0u);
            writeCubeVertex(voxelPosition, firstVertex, 1u);
            writeCubeVertex(voxelPosition, firstVertex, 2u);
        }
    }
}
