#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec3 in_worldPosition;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_lightmap_uv;

layout(location = 0) out vec4 out_color;

#include "frame_set.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform sampler2D albedo;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler2D roughness_map;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D metalness_map;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform MaterialBuffer {
  vec4 albedo_color;
  float roughness_factor;
  float metalness_factor;
  uint albedoTextureIndex;
} material;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler2D lightmap;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform sampler albedoSampler;
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform sampler2D shadows;

struct Cluster {
  vec4 minPoint;
  vec4 maxPoint;
};

layout (std430, set = DESCRIPTOR_SET_FREQUENT, binding = 3) readonly buffer lightBitmasksBuffer {
  uint lightBitmasks[];
};

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 4) uniform sampler2D ssao;

#ifdef DEBUG
layout(std430, set = DESCRIPTOR_SET_FREQUENT, binding = 5, std430) readonly buffer clusterAABB {
  Cluster clusters[];
};
#endif

#define FS
#include "util.inc.glsl"

#include "pbr.inc.glsl"

#include "clustered_shading.inc.glsl"

void main(void) {
  vec2 uv = in_uv;
  vec2 albedoUV = unjitterTextureUv(in_uv, jitterPoint * vec2(rtSize));

  vec3 normal = reconstructNormalFS(gl_FragCoord.xy / vec2(rtSize), gl_FragCoord.z, camera.invView * camera.invProj);

  uint clusterIndex = getClusterIndexWithDepth(gl_FragCoord.xy, gl_FragCoord.z, camera.zNear, camera.zFar, clusterCount, rtSize, clusterZScale, clusterZBias);
  uint maxClusterCount = clusterCount.x * clusterCount.y * clusterCount.z;

  #ifdef DEBUG
    vec3 viewPos = (camera.view * vec4(in_worldPosition, 1)).xyz;
    Cluster c = clusters[clusterIndex];
    if (validateCluster(viewPos, Clusters)) {
      debugPrintfEXT("Wrong cluster: %d, view pos: %f, %f, %f, cluster min: %f, %f, %f, cluster max: %f, %f, %f", clusterIndex, viewPos.x, viewPos.y, viewPos.z, c.minPoint.x, c.minPoint.y, c.minPoint.z, c.maxPoint.x, c.maxPoint.y, c.maxPoint.z);
    }
  #endif

  float roughness = material.roughness_factor * texture(roughness_map, uv).r;
  float metalness = material.metalness_factor * texture(metalness_map, uv).r;
  vec3 albedo = material.albedo_color.rgb * texture(albedo, uv).rgb;

  vec3 viewDir = normalize(camera.position.xyz - in_worldPosition.xyz);
  vec3 f0 = vec3(0.04);
  f0 = mix(f0, albedo, metalness);

  vec2 fullscreenTexCoord = vec2(gl_FragCoord.x / rtSize.x, gl_FragCoord.y / rtSize.y);
  vec3 lighting = vec3(0);
  lighting += vec3(0.3); // ambient
  lighting += texture(lightmap, in_lightmap_uv).xyz;
  lighting *= texture(ssao, fullscreenTexCoord).rrr;

  for (uint i = 0; i < directionalLightCount; i++) {
    DirectionalLight light = directionalLights[i];
    vec3 lightContribution = pbr(-light.directionAndIntensity.xyz, viewDir, normal, f0, albedo, vec3(light.directionAndIntensity.w), roughness, metalness);
    if (i == 0) {
      lightContribution *= texture(shadows, fullscreenTexCoord).rrr;
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
        vec3 fragToLight = light.positionAndIntensity.xyz - in_worldPosition;
        vec3 lightDir = normalize(fragToLight);
        float lightSquaredDist = dot(fragToLight, fragToLight);
        lighting += pbr(lightDir, viewDir, normal, f0, albedo, vec3(light.positionAndIntensity.w / lightSquaredDist), roughness, metalness);
      }
    }
  }
  out_color = vec4(lighting * albedo, 1);
}
