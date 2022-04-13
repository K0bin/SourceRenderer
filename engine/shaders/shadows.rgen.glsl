#version 460
#extension GL_EXT_ray_tracing : require
#extension GL_GOOGLE_include_directive : enable
 #extension GL_EXT_debug_printf : enable

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 0) uniform accelerationStructureEXT topLevelAS;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 1, rgba8) uniform image2D image;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 2) uniform Camera {
    mat4 viewProj;
    mat4 invProj;
    mat4 view;
    mat4 proj;
    mat4 invView;
    vec4 position;
} camera;

layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 3) uniform PerFrameUbo {
  uint frame;
  uint directionalLightCount;
};

/*struct DirectionalLight {
  vec3 direction;
  float intensity;
};
layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 4, std430) readonly buffer directionalLightsBuffer {
  DirectionalLight directionalLights[];
};*/
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 5) uniform sampler2D depthMap;
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 6) uniform sampler2D noise;

layout(location = 0) rayPayloadEXT float hitValue;

#include "util.inc.glsl"

uint seed;
uint hash(uint s) {
  s ^= 2747636419u;
  s *= 2654435769u;
  s ^= s >> 16;
  s *= 2654435769u;
  s ^= s >> 16;
  s *= 2654435769u;
  return s;
}
float random() {
  uint s = seed;
  seed++;
  return float(hash(seed)) / 4294967295.0; // 2^32-1
}

mat4 rotationMatrix(vec3 axis, float angle) {
  axis = normalize(axis);
  float s = sin(angle);
  float c = cos(angle);
  float oc = 1.0 - c;

  return mat4(oc * axis.x * axis.x + c,           oc * axis.x * axis.y - axis.z * s,  oc * axis.z * axis.x + axis.y * s,  0.0,
              oc * axis.x * axis.y + axis.z * s,  oc * axis.y * axis.y + c,           oc * axis.y * axis.z - axis.x * s,  0.0,
              oc * axis.z * axis.x - axis.y * s,  oc * axis.y * axis.z + axis.x * s,  oc * axis.z * axis.z + c,           0.0,
              0.0,                                0.0,                                0.0,                                1.0);
}

#define PI 3.1415926538
#define SUN_ANGLE 0.53

vec3 randomRotateDirection(vec3 dir, float randomDegrees) {
  vec3 noiseSample = textureLod(noise, vec2(gl_LaunchIDEXT.xy) / vec2(textureSize(noise, 0)) + vec2(0.5), 0).xyz;
  vec3 rotationVec = normalize(noiseSample * 2.0 - 1.0);
  rotationVec *= randomDegrees * (PI / 180.0);
  mat4 rotation = rotationMatrix(vec3(1, 0, 0), rotationVec.x) * rotationMatrix(vec3(0, 1, 0), rotationVec.y) * rotationMatrix(vec3(0, 0, 1), rotationVec.z);
  return (rotation * vec4(dir, 0)).xyz;
}

void main() {
  seed = (gl_LaunchIDEXT.z * gl_LaunchSizeEXT.x * gl_LaunchSizeEXT.y
  + gl_LaunchIDEXT.y * gl_LaunchSizeEXT.x
  + gl_LaunchIDEXT.x) * frame;

	const vec2 pixelCenter = vec2(gl_LaunchIDEXT.xy) + vec2(0.5);
  const vec2 inUV = pixelCenter / vec2(gl_LaunchSizeEXT.xy);
  vec2 d = inUV * 2.0 - 1.0;

  vec3 origin = worldSpacePosition(inUV, texture(depthMap, inUV).r, camera.invView * camera.invProj);

  uint rayFlags = gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT;
  uint cullMask = 0xff;
  float tmin = 0.001;
  float tmax = 10000.0;

  vec3 lightDir = normalize(vec3(-0.1, -0.9, -0.5));
  origin += 0.1 * -lightDir;

  // TODO: make this not ridiculously slow for no reason

  vec3 rayDir = randomRotateDirection(-lightDir, SUN_ANGLE);
  traceRayEXT(topLevelAS, rayFlags, cullMask, 0, 0, 0, origin, tmin, rayDir, tmax, 0);

  float shadow = hitValue;

  imageStore(image, ivec2(gl_LaunchIDEXT.xy), vec4(shadow, shadow, shadow, 1.0));
}
