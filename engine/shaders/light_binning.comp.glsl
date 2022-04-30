// https://github.com/pezcode/Cluster/blob/master/src/Renderer/Shaders/cs_clustered_lightculling.sc

#version 450
// #extension GL_EXT_debug_printf : enable
#extension GL_GOOGLE_include_directive : enable
layout(local_size_x = 64) in;

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 0, std140) uniform CameraUBO {
  Camera camera;
};

struct Cluster {
  vec4 minPoint;
  vec4 maxPoint;
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 1, std430) readonly buffer clusterAABB {
  Cluster clusters[];
};

layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 2, std430) readonly buffer setupBuffer {
  uint clusterCount;
  uint pointLightCount;
};

struct PointLight {
  vec3 position;
  float radius;
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 3, std430) readonly buffer pointLightsBuffer {
  PointLight pointLights[];
};

layout (std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 4) buffer lightBitmasksBuffer {
  uint lightBitmasks[];
};

bool pointLightIntersectsCluster(PointLight light, Cluster cluster);

shared vec3 viewSpacePointLights[64];

void main() {
  uint clusterIndex = gl_GlobalInvocationID.x;

  uint lightCount = pointLightCount;
  uint lightOffset = 0;
  uint bitmaskCount = (lightCount + 31) / 32;

  // clear bitmask
  // this is shit, clear them outside of the shaders
  if (clusterIndex < clusterCount) {
    for (uint i = 0; i < bitmaskCount; i++) {
      lightBitmasks[clusterIndex * bitmaskCount + i] = 0;
    }
  }

  while (lightOffset < lightCount) {
    uint batchSize = min(gl_WorkGroupSize.x, lightCount - lightOffset);
    uint lightIndex = lightOffset + gl_LocalInvocationIndex;
    if (uint(gl_LocalInvocationIndex) < batchSize) {
      PointLight light = pointLights[lightIndex];
      viewSpacePointLights[gl_LocalInvocationIndex] = (camera.view * vec4(light.position, 1)).xyz;
    }

    barrier();

    if (clusterIndex < clusterCount) {
      for (uint i = 0; i < batchSize; i++) {
        uint lightIndex = lightOffset + i;
        uint bitmaskIndex = lightIndex / 32;
        uint bitIndex = lightIndex % 32;
        Cluster cluster = clusters[clusterIndex];
        PointLight light = pointLights[lightIndex];
        light.position = viewSpacePointLights[i];
        if (pointLightIntersectsCluster(light, cluster)) {
          // debugPrintfEXT("Light %d visible in cluster %d.", lightIndex, clusterIndex);
          atomicOr(lightBitmasks[bitmaskCount * clusterIndex + bitmaskIndex], 1 << bitIndex);
        }
      }
    }
    lightOffset += batchSize;
  }
}

// check if light radius extends into the cluster
// light position has to be in view space
bool pointLightIntersectsCluster(PointLight light, Cluster cluster) {
  // get closest point to sphere center
  vec3 closest = max(cluster.minPoint.xyz, min(light.position, cluster.maxPoint.xyz));
  // check if point is inside the sphere
  vec3 dist = closest - light.position;
  return dot(dist, dist) <= (light.radius * light.radius);
}
