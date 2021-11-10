#version 450
#extension GL_ARB_separate_shader_objects : enable
// #extension GL_EXT_debug_printf : enable

layout(location = 0) in vec3 in_worldPosition;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_lightmap_uv;

layout(location = 0) out vec4 out_color;

layout(set = 1, binding = 0) uniform sampler2D tex;
layout(set = 1, binding = 1) uniform sampler2D lightmap;

struct Cluster {
  vec4 minPoint;
  vec4 maxPoint;
};

layout(std140, set = 2, binding = 0, std140) uniform CameraUbo {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
} camera;

struct PointLight {
  vec3 position;
  float intensity;
};
layout(std430, set = 2, binding = 1, std430) readonly buffer pointLightsBuffer {
  PointLight pointLights[];
};

layout (std430, set = 2, binding = 2) buffer lightBitmasksBuffer {
  uint lightBitmasks[];
};

struct DirectionalLight {
  vec3 direction;
  float intensity;
};
layout(std430, set = 2, binding = 5, std430) readonly buffer directionalLightsBuffer {
  DirectionalLight directionalLights[];
};

layout(set = 2, binding = 3) uniform PerFrameUbo {
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

layout(set = 2, binding = 4) uniform sampler2D ssao;

/*layout(std430, set = 2, binding = 4, std430) readonly buffer clusterAABB {
  Cluster clusters[];
};*/

float linearizeDepth(float d, float zNear,float zFar);

void main(void) {
  vec2 tileSize = vec2(rtSize) / vec2(clusterCount.xy);

  float z = linearizeDepth(gl_FragCoord.z, zNear, zFar);
  uvec3 clusterIndex3d = uvec3(
    uint(gl_FragCoord.x / tileSize.x),
    uint((rtSize.y - gl_FragCoord.y) / tileSize.y),
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

  vec3 lighting = vec3(0);
  lighting += 0.3;
  //lighting += texture(lightmap, in_lightmap_uv).xyz;
  lighting = min(vec3(1.0, 1.0, 1.0), lighting);
  lighting *= texture(ssao, vec2(gl_FragCoord.x / rtSize.x, gl_FragCoord.y / rtSize.y)).rrr;

  for (uint i = 0; i < directionalLightCount; i++) {
    DirectionalLight light = directionalLights[i];
    lighting += max(0.0, dot(in_normal, -normalize(light.direction)) * light.intensity);
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
        vec3 fragToLight = in_worldPosition - light.position;
        vec3 lightDir = normalize(fragToLight);
        float lightSquaredDist = dot(fragToLight, fragToLight);
        lighting += max(0.0, dot(in_normal, normalize(lightDir)) * (light.intensity * 1.0 / lightSquaredDist));
      }
    }
  }
  vec4 tex = texture(tex, in_uv);
  out_color = vec4(lighting * tex.xyz, 1);
}

float linearizeDepth(float d, float zNear,float zFar)
{
  return 2.0 * zNear * zFar / (zFar + zNear - d * (zFar - zNear));
}
