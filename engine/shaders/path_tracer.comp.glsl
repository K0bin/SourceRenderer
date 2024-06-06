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

#define PI 3.1415926538
#define SUN_ANGLE 0.53
#define ALBEDO_ONLY false

struct RayHitResult {
    vec3 radiance;
    vec3 nextRayOrigin;
    vec3 nextRayDirection;
    vec3 nextFactor;
};

const uint RAYS_PER_PIXEL = 30;

vec3 random(uint iteration);
bool traceRay(vec3 rayOrigin, vec3 rayDirection, uint iteration, out RayHitResult result);
void rayHit(uint drawableIndex, uint partIndex, uint primitiveIndex, vec2 barycentricsYZ, mat4x3 transform, uint iteration, out RayHitResult result);
void rayMiss(vec3 rayDirection, out RayHitResult result);

void main() {
    ivec2 texSize = imageSize(image);
    if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
        return;
    }
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
    ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);

    vec3 rayOrigin = camera.position.xyz;
    vec4 rayDirectionViewSpace = camera.invProj * vec4(vec2(texCoord.x, 1.0 - texCoord.y) * 2.0 - 1.0, 1.0, 1.0);
    rayDirectionViewSpace.xyz /= rayDirectionViewSpace.w;
    vec4 cameraRayDirection = camera.invView * (rayDirectionViewSpace);
    vec3 rayDirection = normalize(cameraRayDirection.xyz);

    vec3 contribution = vec3(1.0);
    vec3 color = vec3(0.0);
    for (uint i = 0; i < RAYS_PER_PIXEL; i++) {
        RayHitResult result;
        bool hit = traceRay(rayOrigin, rayDirection.xyz, i, result);

        color += contribution * result.radiance;
        contribution *= result.nextFactor;

        if (!hit || result.nextFactor.x + result.nextFactor.y + result.nextFactor.z <= 0.1) {
            break;
        }
        rayOrigin = result.nextRayOrigin;
        rayDirection = result.nextRayDirection;
    }

    imageStore(image, iTexCoord, vec4(color, 1));
}

bool traceRay(vec3 rayOrigin, vec3 rayDirection, uint iteration, out RayHitResult result) {
    rayQueryEXT rayQuery;
    rayQueryInitializeEXT(rayQuery, topLevelAS,
                      gl_RayFlagsCullBackFacingTrianglesEXT,
                      0xFF, rayOrigin, 0.01, rayDirection, 100.0);

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
        rayMiss(rayDirection, result);
        return false;
    } else {
        vec2 barycentricsYZ = rayQueryGetIntersectionBarycentricsEXT(rayQuery, true);
        int drawableIndex = rayQueryGetIntersectionInstanceIdEXT(rayQuery, true);
        int partIndex = rayQueryGetIntersectionGeometryIndexEXT(rayQuery, true);
        int primitiveId = rayQueryGetIntersectionPrimitiveIndexEXT(rayQuery, true);
        mat4x3 transform = rayQueryGetIntersectionObjectToWorldEXT(rayQuery, true);

        rayHit(drawableIndex, partIndex, primitiveId, barycentricsYZ, transform, iteration, result);
        return true;
    }
}

// vec3 pbr(vec3 lightDir, vec3 viewDir, vec3 normal, vec3 f0, vec3 albedo, vec3 radiance, float roughness, float metalness);

void rayHit(uint drawableIndex, uint partIndex, uint primitiveIndex, vec2 barycentricsYZ, mat4x3 transform, uint iteration, out RayHitResult result) {
    GPUDrawable drawable = GPU_SCENE_DRAWABLES_NAME[drawableIndex];
    GPUMeshPart part = GPU_SCENE_PARTS_NAME[drawable.partStart + partIndex];

    uint firstIndex = part.meshFirstIndex + primitiveIndex * 3;
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
    vec3 color = albedo;
    vec3 emission = vec3(0.0);

    vec3 random = random(iteration);
    // uniform sampling of hemisphere
    float theta = 0.5 * PI * random.y;
    float phi = 2.0 * PI * random.x;

    vec3 localDiffuseDir = vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
    vec3 diffuseDir = vertex.normal * localDiffuseDir;
    result.nextRayOrigin = (transform * vec4(vertex.position, 1.0)).xyz;
    result.nextRayDirection = diffuseDir;
    result.nextFactor = color * PI * cos(theta) * sin(theta);
    result.radiance = emission;

    // DEBUG
    if (ALBEDO_ONLY) {
        result.radiance = color;
        result.nextFactor = vec3(0.0);
    }
}

vec3 random(uint iteration) {
    ivec2 texSize = imageSize(image);
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
    texCoord += vec2(float(iteration / 3) * 0.3, float(iteration / 5) * 0.3);
    texCoord = mod(texCoord, vec2(1.0));

    return texture(noise, texCoord).xyz;
}

float radToDeg(float rad) {
    return rad * 180.0 / PI;
}

void rayMiss(vec3 rayDirection, out RayHitResult result) {
    vec3 sunDirection = vec3(0.3, 1, 0.2);
    sunDirection = normalize(sunDirection);
    rayDirection = normalize(rayDirection);
    float angle = radToDeg(acos(dot(sunDirection, rayDirection)));

    if (angle <= SUN_ANGLE) {
        // Sun
        result.nextFactor = vec3(0.0);
        result.radiance = vec3(200.0);
        result.nextRayDirection = vec3(0.0);
        result.nextRayOrigin = vec3(0.0);
    } else {
        // Sky
        float y = max(0.0, rayDirection.y);
        vec3 skyBlue = vec3(0.529, 0.808, 0.922);
        vec3 color = mix(vec3(1.0), skyBlue, clamp(y * y + 0.4, 0.0, 1.0));

        result.nextFactor = vec3(0.0);
        result.radiance = color;
        result.nextRayDirection = vec3(0.0);
        result.nextRayOrigin = vec3(0.0);
    }
}
