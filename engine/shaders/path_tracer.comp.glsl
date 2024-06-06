#version 460
#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : enable

#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#define CS
#include "util.inc.glsl"

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

#include "frame_set.inc.glsl"
#include "gpu_scene.inc.glsl"
#include "vis_buf.inc.glsl"
#include "vertex.inc.glsl"
#include "pbr.inc.glsl"

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform accelerationStructureEXT topLevelAS;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, rgba8) uniform image2D image;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform sampler2D noise;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform sampler linearSampler;
layout(set = DESCRIPTOR_SET_TEXTURES_BINDLESS, binding = 0) uniform texture2D albedo_global[1024];

void main() {
    ivec2 texSize = imageSize(image);
    if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
        return;
    }
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
    ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);


    /*bindless debug
    uint index = max(8, min(iTexCoord.x, 8));
    vec3 color1 = texture(sampler2D(albedo_global[index], linearSampler), texCoord).xyz;
    //vec3 color1 = texture(sampler2D(tex15, linearSampler), texCoord).xyz;
    //vec3 color1 = vec3(1.0);
    imageStore(image, iTexCoord, vec4(color1, 1.0));

    return;*/

    vec3 rayOrigin = camera.position.xyz;
    vec4 rayDirectionViewSpace = camera.invProj * vec4(vec2(texCoord.x, 1.0 - texCoord.y) * 2.0 - 1.0, 1.0, 1.0);
    rayDirectionViewSpace.xyz /= rayDirectionViewSpace.w;
    vec4 rayDirection = camera.invView * (rayDirectionViewSpace);
    rayDirection = normalize(rayDirection);

    rayQueryEXT rayQuery;
    rayQueryInitializeEXT(rayQuery, topLevelAS,
                      0,
                      0xFF, rayOrigin, 0.01, rayDirection.xyz, 100.0);

    while (rayQueryProceedEXT(rayQuery)) {
        if (rayQueryGetIntersectionTypeEXT(rayQuery, false) ==
        gl_RayQueryCandidateIntersectionTriangleEXT)
        {
            rayQueryConfirmIntersectionEXT(rayQuery);
        }
    }

    vec3 color;
    if (rayQueryGetIntersectionTypeEXT(rayQuery, true) ==
        gl_RayQueryCommittedIntersectionNoneEXT) {
        color = vec3(0.0);
    } else {
        vec2 barycentricsYZ = rayQueryGetIntersectionBarycentricsEXT(rayQuery, true);
        int drawableIndex = rayQueryGetIntersectionInstanceIdEXT(rayQuery, true);
        int partIndex = rayQueryGetIntersectionGeometryIndexEXT(rayQuery, true);
        int primitiveId = rayQueryGetIntersectionPrimitiveIndexEXT(rayQuery, true);

        GPUDrawable drawable = GPU_SCENE_DRAWABLES_NAME[drawableIndex];
        GPUMeshPart part = GPU_SCENE_PARTS_NAME[drawable.partStart + partIndex];

        uint firstIndex = part.meshFirstIndex + primitiveId * 3;
        uint index0 = INDICES_ARRAY_NAME[firstIndex];
        uint index1 = INDICES_ARRAY_NAME[firstIndex + 1];
        uint index2 = INDICES_ARRAY_NAME[firstIndex + 2];

        Vertex triangle_verts[3];
        triangle_verts[0] = VERTICES_ARRAY_NAME[part.meshVertexOffset + index0];
        triangle_verts[1] = VERTICES_ARRAY_NAME[part.meshVertexOffset + index1];
        triangle_verts[2] = VERTICES_ARRAY_NAME[part.meshVertexOffset + index2];

        vec3 barycentrics = vec3(1.0 - barycentricsYZ.x - barycentricsYZ.y, barycentricsYZ.x, barycentricsYZ.y);
        Vertex vertex = interpolateVertex(barycentrics, triangle_verts);

        GPUMaterial material = GPU_SCENE_MATERIALS_NAME[part.materialIndex];
        vec3 albedo = material.albedoColor.rgb * texture(sampler2D(albedo_global[material.albedoTextureIndex], linearSampler), vertex.uv).rgb;
        color = albedo;
    }

    imageStore(image, iTexCoord, vec4(color, 1));
}
