#version 460
#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable

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
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1, rgba8) uniform coherent writeonly image2D image;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform sampler2D noise;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform sampler linearSampler;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4, rgba8) uniform readonly image2D historyImage;
layout(set = DESCRIPTOR_SET_TEXTURES_BINDLESS, binding = 0) uniform texture2D albedo_global[1024];

#define PI 3.1415926538
#define SUN_ANGLE 0.53
#define SUN_RADIANCE 80000
#define SKY_RANDIANCE 10000
#define ALBEDO_ONLY false
#define USE_PROCEDURAL_NOISE true
vec3 sunDirection = vec3(-0.3, 0.5, -0.3);

struct RayHitResult {
    vec3 radiance;
    vec3 nextRayOrigin;
    vec3 nextRayDirection;
    vec3 nextFactor;
    vec4 oldColor;
};

const uint LIGHT_BOUNCES = 64;


float radToDeg(float rad);
float degToRad(float deg);
vec3 random(uint iteration);
vec3 random_pcg3d(uvec3 v);
vec3 randomBlueNoise(uint iteration);
bool traceRay(vec3 rayOrigin, vec3 rayDirection, uint iteration, out RayHitResult result);
void rayHit(uint drawableIndex, uint partIndex, uint primitiveIndex, vec3 viewDir, vec2 barycentricsYZ, mat4x3 transform, uint iteration, out RayHitResult result);
void rayMiss(vec3 rayDirection, uint iteration, out RayHitResult result);
bool getHistoryColor(GPUDrawable drawable, Vertex vertex, vec3 transformedPosition, out vec4 oldColor);
mat3 getNormalSpace(in vec3 normal);

void main() {
    ivec2 texSize = imageSize(image);
    if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
        return;
    }
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) - 0.5) / float(texSize.y));
    ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);

    vec3 rayOrigin = camera.position.xyz;
    float fovy = 2.0 * atan(tan(camera.fov * 0.5) * (1.0 / camera.aspectRatio));
    float d = 1.0 / tan(fovy * 0.5);
    vec4 rayDirView = vec4(
        camera.aspectRatio * (2.0 * texCoord.x - 1.0),
        -(2.0 * texCoord.y - 1.0),
        d,
        0.0
    );
    vec3 rayDirection = normalize(camera.invView * rayDirView).xyz;

    vec3 contribution = vec3(1.0);
    vec4 oldColor = vec4(0.0);
    vec3 color = vec3(0.0);
    for (uint i = 0; i < LIGHT_BOUNCES; i++) {
        RayHitResult result;
        bool hit = traceRay(rayOrigin, rayDirection, i, result);

        oldColor += result.oldColor;
        color += contribution * result.radiance;
        contribution *= result.nextFactor;

        if (subgroupAll(!hit) || subgroupMax(length(contribution)) <= 0.01) {
            break;
        }
        rayOrigin = result.nextRayOrigin;
        rayDirection = result.nextRayDirection;
    }
    color = max(color, vec3(0.0));

    bool hasOldColor = length(oldColor.xyz) > 0.01;
    hasOldColor = true;

    // revert eye adaptation
    oldColor.xyz *= 85000.0;

    color = mix(
            color,
            oldColor.xyz,
            hasOldColor ? 0.999 : 0.85
            //clamp((oldColor.w * 0.1) + 0.99, 0.0, 0.99)
            //oldColor.w
        );

    vec4 finalColor = vec4(color, min(1.0, oldColor.w + epsilon));
    finalColor.xyz /= 85000.0; // eye adaptation
    imageStore(image, iTexCoord, finalColor);
}

bool traceRay(vec3 rayOrigin, vec3 rayDirection, uint iteration, out RayHitResult result) {
    rayQueryEXT rayQuery;
    rayQueryInitializeEXT(rayQuery, topLevelAS,
                      0,
                      0xFF, rayOrigin, 0.001, rayDirection, 10000.0);

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
        rayMiss(rayDirection, iteration, result);
        return false;
    } else {
        vec2 barycentricsYZ = rayQueryGetIntersectionBarycentricsEXT(rayQuery, true);
        int drawableIndex = rayQueryGetIntersectionInstanceIdEXT(rayQuery, true);
        int partIndex = rayQueryGetIntersectionGeometryIndexEXT(rayQuery, true);
        int primitiveId = rayQueryGetIntersectionPrimitiveIndexEXT(rayQuery, true);
        mat4x3 transform = rayQueryGetIntersectionObjectToWorldEXT(rayQuery, true);
        vec3 viewDir = -rayQueryGetWorldRayDirectionEXT(rayQuery);

        rayHit(drawableIndex, partIndex, primitiveId, viewDir, barycentricsYZ, transform, iteration, result);
        return true;
    }
}

void rayHit(uint drawableIndex, uint partIndex, uint primitiveIndex, vec3 viewDir, vec2 barycentricsYZ, mat4x3 transform, uint iteration, out RayHitResult result) {
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
    vec3 transformedPosition = (transform * vec4(vertex.position, 1.0)).xyz;
    vec3 transformedNormal = normalize((transform * vec4(vertex.normal, 0.0)).xyz);

    result.oldColor = vec4(0.0);
    if (iteration == 0) {
        // Reproject the point we hit with the camera ray to get the past frame color
        getHistoryColor(drawable, vertex, transformedPosition, result.oldColor);
    }

    GPUMaterial material = GPU_SCENE_MATERIALS_NAME[part.materialIndex];
    material.metalnessFactor *= 0.25;
    vec3 albedo = material.albedoColor.rgb * texture(sampler2D(albedo_global[nonuniformEXT(material.albedoTextureIndex)], linearSampler), vertex.uv).rgb;
    vec3 color = albedo;
    vec3 emission = vec3(0.0);

    vec3 rand = random(iteration);
    float phi = 2.0 * PI * rand.x;
    vec3 lightDir = vec3(0.0);
    result.nextRayOrigin = transformedPosition;

    vec3 nextFactor = vec3(0.0);

    if (rand.z < 0.5) {
        // importance sampling of ggx
        float a = material.roughnessFactor * material.roughnessFactor;
        float theta = acos(sqrt((1.0 - rand.y) / (1.0 + (a * a - 1.0) * rand.y)));
        vec3 localDir = vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
        vec3 worldDir = getNormalSpace(transformedNormal) * localDir;
        lightDir = reflect(-viewDir, worldDir);
        vec3 halfway = worldDir;

        float viewDotHalf = clamp(dot(viewDir, halfway), 0.0, 1.0);
        vec3 f0 = vec3(0.04);
        f0 = mix(f0, albedo, material.metalnessFactor);
        vec3 fresnel = fresnelSchlick(viewDotHalf, f0);
        float distribution = distributionGGX(transformedNormal, halfway, material.roughnessFactor);
        float geometry = geometrySmith(transformedNormal, viewDir, lightDir, material.roughnessFactor, false);
        nextFactor = fresnel * geometry * viewDotHalf / max(clamp(dot(transformedNormal, halfway), 0.0, 1.0) * clamp(dot(transformedNormal, viewDir), 0.0, 1.0), 0.001);
        nextFactor *= 2.0;
    } else {
        // importance sampling diffuse
        float theta = asin(sqrt(rand.y));
        vec3 normalizedSunDirection = normalize(sunDirection);
        if ((rand.z > 0.75 && dot(transformedNormal, normalizedSunDirection) > 0.1)) {
            float thetaLight = acos(1.0 - 2.0 * rand.y);
            float phiLight = 2.0 * PI * rand.x;
            vec3 spherePos = vec3(sin(thetaLight) * cos(phiLight), sin(thetaLight) * sin(phiLight), cos(thetaLight));
            lightDir = normalize(normalizedSunDirection + spherePos * degToRad(SUN_ANGLE));
        } else {
            vec3 localDir = vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
            lightDir = getNormalSpace(transformedNormal) * localDir;
        }
        vec3 halfway = normalize(viewDir + lightDir);
        float viewDotHalf = clamp(dot(viewDir, halfway), 0.0, 1.0);
        vec3 f0 = vec3(0.04);
        f0 = mix(f0, albedo, material.metalnessFactor);
        vec3 fresnel = fresnelSchlick(viewDotHalf, f0);
        vec3 notSpec = vec3(1.0) - fresnel;
        notSpec *= 1.0 - material.metalnessFactor;
        nextFactor = notSpec * color;
        nextFactor *= 2.0;
    }

    result.nextRayDirection = lightDir;
    result.nextFactor = nextFactor;
    result.radiance = emission;

    // DEBUG
    if (ALBEDO_ONLY) {
        result.radiance = color;
        result.nextFactor = vec3(0.0);
    }
}

vec3 random(uint iteration) {
    if (USE_PROCEDURAL_NOISE) {
        return random_pcg3d(uvec3(uint(gl_GlobalInvocationID.x), uint(gl_GlobalInvocationID.y), frameIdx * LIGHT_BOUNCES + iteration));
    } else {
        return randomBlueNoise(iteration);
    }
}

vec3 randomBlueNoise(uint iteration) {
    ivec2 texSize = imageSize(image);
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
    texCoord += vec2((iteration % 7) * 0.3, float(iteration / 7) * 0.3);
    texCoord = mod(texCoord, vec2(1.0));

    return texture(noise, texCoord).xyz;
}

// Hash Functions for GPU Rendering, Jarzynski et al.
// http://www.jcgt.org/published/0009/03/02/
vec3 random_pcg3d(uvec3 v) {
  v = v * 1664525u + 1013904223u;
  v.x += v.y*v.z; v.y += v.z*v.x; v.z += v.x*v.y;
  v ^= v >> 16u;
  v.x += v.y*v.z; v.y += v.z*v.x; v.z += v.x*v.y;
  return vec3(v) * (1.0/float(0xffffffffu));
}

mat3 getNormalSpace(in vec3 normal) {
   vec3 someVec = vec3(1.0, 0.0, 0.0);
   float dd = dot(someVec, normal);
   vec3 tangent = vec3(0.0, 1.0, 0.0);
   if(1.0 - abs(dd) > 1e-6) {
     tangent = normalize(cross(someVec, normal));
   }
   vec3 bitangent = cross(normal, tangent);
   return mat3(tangent, bitangent, normal);
}

float radToDeg(float rad) {
    return rad * 180.0 / PI;
}

float degToRad(float deg) {
    return deg * (PI / 180.0);
}

void rayMiss(vec3 rayDirection, uint iteration, out RayHitResult result) {
    sunDirection = normalize(sunDirection);
    rayDirection = normalize(rayDirection);
    float angle = radToDeg(acos(dot(sunDirection, rayDirection)));

    if (angle <= SUN_ANGLE) {
        // Sun
        result.nextFactor = vec3(1.0);
        result.radiance = vec3(SUN_RADIANCE);
        result.nextRayDirection = vec3(0.0);
        result.nextRayOrigin = vec3(0.0);
    } else {
        // Sky
        float y = max(0.0, rayDirection.y);
        vec3 skyBlue = vec3(0.529, 0.808, 0.922);
        vec3 color = mix(vec3(1.0), skyBlue, clamp(y * y + 0.4, 0.0, 1.0));

        result.nextFactor = vec3(1.0);
        result.radiance = color * SKY_RANDIANCE; //(iteration == 0 ? 0 : 0);
        result.nextRayDirection = vec3(0.0);
        result.nextRayOrigin = vec3(0.0);
    }
    result.oldColor = vec4(0.0);

    if (iteration == 0 && false) {
        ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);
        imageStore(image, iTexCoord, vec4(1.0, 0.0, 1.0, 1.0));
    }
}

bool getHistoryColor(GPUDrawable drawable, Vertex vertex, vec3 transformedPosition, out vec4 oldColor) {
    vec4 lastFramePosition = drawable.oldTransform * vec4(vertex.position, 1.0);
    vec4 lastFramePositionClipspace = oldCamera.viewProj * lastFramePosition;
    vec4 lastFramePositionNDC = lastFramePositionClipspace;
    lastFramePositionNDC.xyz /= lastFramePositionNDC.w;
    vec2 lastFrameTexcoord = lastFramePositionNDC.xy * 0.5 + vec2(0.5);
    ivec2 texSize = imageSize(image);
    ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);

    bool withinOfBounds = lastFrameTexcoord.x >= 0.0 && lastFrameTexcoord.x < 1.0 && lastFrameTexcoord.y > 0.0 && lastFrameTexcoord.y <= 1.0;
    bool vertexCloseEnough = length(transformedPosition - lastFramePosition.xyz) < 0.1;
    bool reject = !withinOfBounds || !vertexCloseEnough;

    // DEBUG
    // I originally wanted to allow moving around by reprojecting the pixel in the previous frame.
    // However, while it mostly worked, I hit a few issues where it didn't work as expected
    // and I didn't have time to figure it out.
    ivec2 oldITexCoord = ivec2(vec2(lastFrameTexcoord.x, 1.0 - lastFrameTexcoord.y) * vec2(texSize));
    oldITexCoord.y += 1;
    vec4 test = camera.view * vec4(0.0, 0.0, 0.0, 1.0);
    vec4 oldTest = oldCamera.view * vec4(0.0, 0.0, 0.0, 1.0);
    vec3 diff = test.xyz - oldTest.xyz;
    reject = length(diff) > 0.01;

    if (!reject) {
        lastFrameTexcoord.y = 1.0 - lastFrameTexcoord.y;
        vec2 lastFrameTexCoordPixels = lastFrameTexcoord.xy * vec2(texSize);
        lastFrameTexCoordPixels.y += 1.0;
        //oldColor = imageLoad(historyImage, ivec2(lastFrameTexCoordPixels));
        oldColor = imageLoad(historyImage, iTexCoord);
        return true;
    } else {
        oldColor = vec4(0.0);
        return false;
    }

    return false;
}
