#version 450
#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

#include "gpu_scene.inc.glsl"
#include "vertex.inc.glsl"

layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 0, std430) readonly restrict buffer verticesSSBO {
  Vertex vertices[];
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 1, std430) readonly restrict buffer indicesSSBO {
  uint indices[];
};

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 2, r32ui) readonly uniform uimage2D primitiveIds;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 3, rg16) readonly uniform image2D barycentrics;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 4) writeonly uniform image2D outputTexture;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 5, std140) uniform CameraUBO {
  Camera camera;
};
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 6, std430) readonly restrict buffer sceneBuffer {
  GPUScene scene;
};

layout(set = DESCRIPTOR_SET_TEXTURES_BINDLESS, binding = 7) uniform texture2D albedo_global[];
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 8) uniform sampler albedoSampler;

layout (std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 9) readonly buffer lightBitmasksBuffer {
  uint lightBitmasks[];
};

struct PointLight {
  vec3 position;
  float intensity;
};
layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 10, std430) readonly buffer pointLightsBuffer {
  PointLight pointLights[];
};

struct DirectionalLight {
  vec3 direction;
  float intensity;
};
layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 11, std430) readonly buffer directionalLightsBuffer {
  DirectionalLight directionalLights[];
};

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 12) uniform PerFrameUbo {
  float clusterZBias;
  float clusterZScale;
  uvec3 clusterCount;
  uint pointLightCount;
  uint directionalLightCount;
};

#ifdef DEBUG
struct Cluster {
  vec4 minPoint;
  vec4 maxPoint;
};

layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 13, std430) readonly buffer clusterAABB {
  Cluster clusters[];
};
#endif

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 14) uniform sampler2D ssao;

#include "util.inc.glsl"

#include "pbr.inc.glsl"

#include "vis_buf.inc.glsl"
#include "clustered_shading.inc.glsl"


// REFERENCE:
// http://john-chapman-graphics.blogspot.com/2013/01/ssao-tutorial.html
// https://learnopengl.com/Advanced-Lighting/SSAO
// https://github.com/SaschaWillems/Vulkan/blob/master/data/shaders/glsl/ssao/ssao.frag

void main() {
  ivec2 texSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
  ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);

  uint id = imageLoad(primitiveIds, iTexCoord).x;
  vec2 barycentrics = imageLoad(barycentrics, iTexCoord).xy;
  Vertex vertex = getVertex(scene, id, barycentrics);

  vec3 viewPos = (camera.view * vec4(vertex.position, 1.0)).xyz;
  vec3 normal = vertex.normal;
  vec2 uv = vertex.uv;
  vec2 albedoUV = uv;

  uint clusterIndex = getClusterIndex(texCoord, viewPos.z, clusterCount, uvec2(texSize), clusterZScale, clusterZBias);
  uint maxClusterCount = clusterCount.x * clusterCount.y * clusterCount.z;

  #ifdef DEBUG
    Cluster c = clusters[clusterIndex];
    if (validateCluster(viewPos, Clusters)) {
      debugPrintfEXT("Wrong cluster: %d, view pos: %f, %f, %f, cluster min: %f, %f, %f, cluster max: %f, %f, %f", clusterIndex, viewPos.x, viewPos.y, viewPos.z, c.minPoint.x, c.minPoint.y, c.minPoint.z, c.maxPoint.x, c.maxPoint.y, c.maxPoint.z);
    }
  #endif

  GPUMaterial material = getMaterial(scene, id);

  float roughness = material.roughnessFactor;
  float metalness = material.metalnessFactor;
  vec3 albedo = material.albedoColor.rgb * texture(sampler2D(albedo_global[material.albedoTextureIndex], albedoSampler), albedoUV).rgb;

  vec3 viewDir = normalize(camera.position.xyz - vertex.position.xyz);
  vec3 f0 = vec3(0.04);
  f0 = mix(f0, albedo, metalness);

  vec3 lighting = vec3(0);
  //lighting += texture(lightmap, vertex.lightmapUv).xyz;
  lighting *= texture(ssao, texCoord).rrr;
  //lighting *= texture(shadows, texCoord).rrr;

  for (uint i = 0; i < directionalLightCount; i++) {
    DirectionalLight light = directionalLights[i];
    lighting += pbr(-light.direction, viewDir, normal, f0, albedo, vec3(light.intensity), roughness, metalness);
  }

  uint lightBitmaskCount = (pointLightCount + 31) / 32;
  uint bitmaskOffset = lightBitmaskCount * clusterIndex;
  for (uint i = 0; i < lightBitmaskCount; i++) {
    uint bitmaskIndex = bitmaskOffset + i;
    uint bitmask;
    if (clusterIndex < maxClusterCount)
      bitmask = lightBitmasks[bitmaskIndex];
    else
      bitmask = 0;

    while (bitmask != 0) {
      uint bitIndex = findLSB(bitmask);
      uint singleBitMask = 1 << bitIndex;
      bool lightActive = (bitmask & singleBitMask) == singleBitMask;
      bitmask &= ~singleBitMask;
      if (lightActive) {
        PointLight light = pointLights[i * 32 + bitIndex];
        vec3 fragToLight = light.position - vertex.position;
        vec3 lightDir = normalize(fragToLight);
        float lightSquaredDist = dot(fragToLight, fragToLight);
        lighting += pbr(lightDir, viewDir, normal, f0, albedo, vec3(light.intensity / lightSquaredDist), roughness, metalness);
      }
    }
  }

  imageStore(outputTexture, iTexCoord, vec4(lighting * albedo, 1));
}
