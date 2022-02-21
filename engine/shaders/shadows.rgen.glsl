#version 460
#extension GL_EXT_ray_tracing : require
#extension GL_GOOGLE_include_directive : enable
 #extension GL_EXT_debug_printf : enable

#include "descriptor_sets.h"

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

/*layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 3) uniform PerFrameUbo {
  uint directionalLightCount;
};

struct DirectionalLight {
  vec3 direction;
  float intensity;
};
layout(std430, set = DESCRIPTOR_SET_PER_FRAME, binding = 4, std430) readonly buffer directionalLightsBuffer {
  DirectionalLight directionalLights[];
};*/
layout(set = DESCRIPTOR_SET_PER_FRAME, binding = 5) uniform sampler2D depthMap;

layout(location = 0) rayPayloadEXT float hitValue;

vec3 worldSpacePosition(vec2 uv);

void main() {
	const vec2 pixelCenter = vec2(gl_LaunchIDEXT.xy) + vec2(0.5);
    const vec2 inUV = pixelCenter / vec2(gl_LaunchSizeEXT.xy);
    vec2 d = inUV * 2.0 - 1.0;

    vec3 origin = worldSpacePosition(inUV);

    uint rayFlags = gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT;
    uint cullMask = 0xff;
    float tmin = 0.001;
    float tmax = 10000.0;

    vec3 lightDir = normalize(vec3(-0.1, -0.9, -0.2));
    origin += 0.1 * -lightDir;

    traceRayEXT(topLevelAS, rayFlags, cullMask, 0, 0, 0, origin, tmin, -lightDir, tmax, 0);

    imageStore(image, ivec2(gl_LaunchIDEXT.xy), vec4(hitValue, hitValue, hitValue, 1.0));
}

vec3 worldSpacePosition(vec2 uv) {
  float depth = texture(depthMap, uv).r;
  vec4 clipSpacePosition = vec4(uv * 2.0 - 1.0, depth, 1.0);
  clipSpacePosition.y = -clipSpacePosition.y;
  vec4 worldSpacePosTemp = (camera.invView * camera.invProj) * clipSpacePosition;
  return worldSpacePosTemp.xyz / worldSpacePosTemp.w;
}
