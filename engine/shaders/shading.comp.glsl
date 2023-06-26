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

layout(set = DESCRIPTOR_SET_TEXTURES_BINDLESS, binding = 0) uniform texture2D albedo_global[];

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1, r32ui) readonly uniform uimage2D primitiveIds;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2, rg16) readonly uniform image2D barycentrics;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) writeonly uniform image2D outputTexture;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 4) uniform sampler albedoSampler;

layout (set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 5) readonly buffer lightBitmasksBuffer {
  uint lightBitmasks[];
};

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 6) uniform sampler2D lightmap;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 7) uniform sampler2D shadows;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 8) uniform sampler2D ssao;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 9) uniform sampler2DArrayShadow shadowMaps;

#include "frame_set.inc.glsl"

#ifdef DEBUG
struct Cluster {
  vec4 minPoint;
  vec4 maxPoint;
};

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 10, std430) readonly buffer clusterAABB {
  Cluster clusters[];
};
#endif

#include "util.inc.glsl"

#include "pbr.inc.glsl"

#include "vis_buf.inc.glsl"
#include "clustered_shading.inc.glsl"

void main() {
  ivec2 texSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
  ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);

  uint id = imageLoad(primitiveIds, iTexCoord).x;
  vec2 barycentrics = imageLoad(barycentrics, iTexCoord).xy;
  Vertex vertex = getVertex(id, barycentrics);

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

  GPUMaterial material = getMaterial(id);

  float roughness = material.roughnessFactor;
  float metalness = material.metalnessFactor;
  vec3 albedo = material.albedoColor.rgb * texture(sampler2D(albedo_global[material.albedoTextureIndex], albedoSampler), albedoUV).rgb;

  vec3 viewDir = normalize(camera.position.xyz - vertex.position.xyz);
  vec3 f0 = vec3(0.04);
  f0 = mix(f0, albedo, metalness);

  vec3 lighting = vec3(0);
  lighting += vec3(0.3); // ambient
  lighting += texture(lightmap, vertex.lightmapUv).xyz;
  lighting *= texture(ssao, texCoord).rrr;

  for (uint i = 0; i < directionalLightCount; i++) {
    DirectionalLight light = directionalLights[i];
    vec3 lightContribution = pbr(-light.directionAndIntensity.xyz, viewDir, normal, f0, albedo, vec3(light.directionAndIntensity.w), roughness, metalness);
    if (i == 0) {

      lightContribution *= texture(shadows, texCoord).rrr;
      uint cascadeIndex = cascadeCount;
      for (uint j = 0; j < cascadeCount; j++) {
        ShadowCascade cascade = cascades[j];
        if (viewPos.z >= cascade.zMin && viewPos.z < cascade.zMax) {
          cascadeIndex = j;
          break;
        }
      }
      if (cascadeIndex < cascadeCount) {

        ShadowCascade cascade = cascades[cascadeIndex];
        vec4 lightSpacePos = (cascade.lightMatrix * vec4(vertex.position, 1.0));
        lightSpacePos.xyz /= lightSpacePos.w;
        lightSpacePos.xyz = lightSpacePos.xyz * 0.5 + 0.5;
        lightSpacePos.y = 1 - lightSpacePos.y;

        vec3 coord = vec3(lightSpacePos.xy, cascadeIndex);
        if (coord.x >= 0.0 && coord.x < 1.0 && coord.y >= 0.0 && coord.y < 1.0) {
          vec4 shadowGather = textureGather(shadowMaps, coord, lightSpacePos.z);
          //lightContribution *= dot(shadowGather, vec4(0.25, 0.25, 0.25, 0.25));
        }
      }
    }
    lighting += lightContribution;
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
        vec3 fragToLight = light.positionAndIntensity.xyz - vertex.position;
        vec3 lightDir = normalize(fragToLight);
        float lightSquaredDist = dot(fragToLight, fragToLight);
        lighting += pbr(lightDir, viewDir, normal, f0, albedo, vec3(light.positionAndIntensity.w / lightSquaredDist), roughness, metalness);
      }
    }
  }

  imageStore(outputTexture, iTexCoord, vec4(lighting * albedo, 1));
  //imageStore(outputTexture, iTexCoord, vec4(texture(shadowMap, lightSpacePos.xy).r, 0, 0, 1));
}
