#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "descriptor_sets.inc.glsl"

const float PI = 3.14159265359;

layout(location = 0) in vec3 in_worldPosition;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_lightmap_uv;

layout(location = 0) out vec4 out_color;

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 6) uniform sampler2D lightmap;
layout(set = DESCRIPTOR_SET_PER_MATERIAL, binding = 0) uniform sampler2D albedo;
layout(set = DESCRIPTOR_SET_PER_MATERIAL, binding = 1) uniform sampler2D roughness_map;
layout(set = DESCRIPTOR_SET_PER_MATERIAL, binding = 2) uniform sampler2D metalness_map;
layout(set = DESCRIPTOR_SET_PER_MATERIAL, binding = 3) uniform Material {
  vec4 albedo_color;
  float roughness_factor;
  float metalness_factor;
  uint albedoTextureIndex;
} material;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 7) uniform sampler albedoSampler;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 8) uniform sampler2D shadows;

struct Cluster {
  vec4 minPoint;
  vec4 maxPoint;
};

layout(std140, set = DESCRIPTOR_SET_PER_FRAME, binding = 0, std140) uniform CameraUbo {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
  mat4 invView;
  vec4 position;
} camera;

struct PointLight {
  vec3 position;
  float intensity;
};
layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 1, std430) readonly buffer pointLightsBuffer {
  PointLight pointLights[];
};

layout (std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 2) readonly buffer lightBitmasksBuffer {
  uint lightBitmasks[];
};

struct DirectionalLight {
  vec3 direction;
  float intensity;
};
layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 5, std430) readonly buffer directionalLightsBuffer {
  DirectionalLight directionalLights[];
};

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 3) uniform PerFrameUbo {
  mat4 swapchainTransform;
  vec2 jitterPoint;
  float zNear;
  float zFar;
  uvec2 rtSize;
  float clusterZBias;
  float clusterZScale;
  uvec3 clusterCount;
  uint pointLightCount;
  uint directionalLightCount;
};

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 4) uniform sampler2D ssao;

/*layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 9, std430) readonly buffer clusterAABB {
  Cluster clusters[];
};*/

#define FS
#include "util.inc.glsl"

vec3 pbr(vec3 lightDir, vec3 viewDir, vec3 normal, vec3 f0, vec3 albedo, vec3 radiance, float roughness, float metalness);
float distributionGGX(vec3 normal, vec3 halfway, float roughness);
float schlickGGX(float nDotV, float roughness);
float geometrySmith(vec3 normal, vec3 viewDir, vec3 lightDir, float k);
vec3 fresnelSchlick(float cosTheta, vec3 f0);

vec2 unjitterTextureUv(vec2 uv, vec2 jitterPx);

void main(void) {
  vec2 uv = in_uv;
  vec2 albedoUV = unjitterTextureUv(in_uv, jitterPoint * vec2(rtSize));

  vec3 normal = reconstructNormalFS(gl_FragCoord.xy / vec2(rtSize), gl_FragCoord.z, camera.invView * camera.invProj);

  vec2 tileSize = vec2(rtSize) / vec2(clusterCount.xy);

  float z = linearizeDepth(gl_FragCoord.z, zNear, zFar);
  uvec3 clusterIndex3d = uvec3(
    uint(gl_FragCoord.x / tileSize.x),
    uint(gl_FragCoord.y / tileSize.y),
    uint(max(0.0, log2(z) * clusterZScale + clusterZBias))
  );

  uint clusterIndex = clusterIndex3d.x +
                    clusterIndex3d.y * clusterCount.x +
                    clusterIndex3d.z * (clusterCount.x * clusterCount.y);

  uint maxClusterCount = clusterCount.x * clusterCount.y * clusterCount.z;

  /*
  vec3 viewPos = (camera.view * vec4(in_worldPosition, 1)).xyz;
  if (abs(z - viewPos.z) > 0.01) {
    debugPrintfEXT("Wrong z: %f, expected: %f", z, viewPos.z);
  }

  Cluster c = clusters[clusterIndex];
  if (viewPos.x > c.maxPoint.x + 0.01 || viewPos.x < c.minPoint.x - 0.01
  || viewPos.y > c.maxPoint.y + 0.01 || viewPos.y < c.minPoint.y - 0.01
  || viewPos.z > c.maxPoint.z + 0.01 || viewPos.z < c.minPoint.z - 0.01) {
    debugPrintfEXT("Wrong cluster: %d, view pos: %f, %f, %f, cluster min: %f, %f, %f, cluster max: %f, %f, %f", clusterIndex, viewPos.x, viewPos.y, viewPos.z, c.minPoint.x, c.minPoint.y, c.minPoint.z, c.maxPoint.x, c.maxPoint.y, c.maxPoint.z);
  }
  */

  float roughness = material.roughness_factor * texture(roughness_map, uv).r;
  float metalness = material.metalness_factor * texture(metalness_map, uv).r;
  vec3 albedo = material.albedo_color.rgb * texture(albedo, uv).rgb;

  vec3 viewDir = normalize(camera.position.xyz - in_worldPosition.xyz);
  vec3 f0 = vec3(0.04);
  f0 = mix(f0, albedo, metalness);

  vec3 lighting = vec3(0);
  lighting += 0.3;
  lighting += texture(lightmap, in_lightmap_uv).xyz;
  lighting *= texture(ssao, vec2(gl_FragCoord.x / rtSize.x, gl_FragCoord.y / rtSize.y)).rrr;
  lighting *= texture(shadows, vec2(gl_FragCoord.x / rtSize.x, gl_FragCoord.y / rtSize.y)).rrr;
  lighting += 0.3;

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
        vec3 fragToLight = light.position - in_worldPosition;
        vec3 lightDir = normalize(fragToLight);
        float lightSquaredDist = dot(fragToLight, fragToLight);
        lighting += pbr(lightDir, viewDir, normal, f0, albedo, vec3(light.intensity / lightSquaredDist), roughness, metalness);
      }
    }
  }
  out_color = vec4(lighting * albedo, 1);
  //out_color = vec4(in_worldPosition, 1.0);
  //out_color = vec4(texture(shadows, vec2(gl_FragCoord.x / rtSize.x, gl_FragCoord.y / rtSize.y)).rrr, 1.0);
}

vec2 unjitterTextureUv(vec2 uv, vec2 jitterPx) {
  return uv - vec2(
    -dFdxFine(uv.x) * jitterPx.x,
    dFdyFine(uv.y) * jitterPx.y
  );
}

vec3 pbr(vec3 lightDir, vec3 viewDir, vec3 normal, vec3 f0, vec3 albedo, vec3 radiance, float roughness, float metalness) {
  vec3 halfway = normalize(viewDir + lightDir);

  float ndf = distributionGGX(normal, halfway, roughness);
  float g = geometrySmith(normal, viewDir, lightDir, roughness);
  vec3 f = fresnelSchlick(max(dot(halfway, viewDir), 0.0), f0);

  vec3 kS = f;
  vec3 kD = vec3(1.0) - kS;
  kD *= 1.0 - metalness;

  vec3 specular = (ndf * g * f) / (4.0 * max(dot(normal, viewDir), 0.0) * max(dot(normal, lightDir), 0.0) + 0.0001);
  float nDotL = max(dot(normal, lightDir), 0.0);
  return (kD * albedo / PI + specular) * radiance * nDotL;
}

float distributionGGX(vec3 normal, vec3 halfway, float roughness) {
  float a = roughness * roughness;
  float a2 = a * a;
  float nDotH = max(dot(normal, halfway), 0.0);
  float nDotH2 = nDotH * nDotH;
  float x = nDotH2 * (a2 - 1.0) + 1.0;
  return a2 / (PI * x * x);
}

float geometrySchlickGGX(float nDotV, float roughness) {
  float r = roughness + 1.0;
  float k = (r*r) / 8.0;
  return nDotV / (nDotV * (1.0 - k) + k);
}

float geometrySmith(vec3 normal, vec3 viewDir, vec3 lightDir, float roughness) {
  float nDotV = max(dot(normal, viewDir), 0.0);
  float nDotL = max(dot(normal, lightDir), 0.0);
  float ggx2 = geometrySchlickGGX(nDotV, roughness);
  float ggx1 = geometrySchlickGGX(nDotL, roughness);
  return ggx1 * ggx2;
}

vec3 fresnelSchlick(float cosTheta, vec3 f0) {
  return f0 + (1.0 - f0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}
