#version 460
#extension GL_EXT_ray_tracing : require
#extension GL_GOOGLE_include_directive : enable

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

layout(location = 0) rayPayloadEXT float hitValue;

void main() {
	const vec2 pixelCenter = vec2(gl_LaunchIDEXT.xy) + vec2(0.5);
    const vec2 inUV = pixelCenter/vec2(gl_LaunchSizeEXT.xy);
    vec2 d = inUV * 2.0 - 1.0;

    vec4 origin = camera.invView * vec4(0,0,0,1);
    vec4 target = camera.invProj * vec4(d.x, d.y, 1, 1) ;
    vec4 direction = camera.invView * vec4(normalize(target.xyz / target.w), 0) ;

    uint rayFlags = gl_RayFlagsOpaqueEXT;
    uint cullMask = 0xff;
    float tmin = 0.001;
    float tmax = 10000.0;

    traceRayEXT(topLevelAS, rayFlags, cullMask, 0, 0, 0, origin.xyz, tmin, direction.xyz, tmax, 0);

    imageStore(image, ivec2(gl_LaunchIDEXT.xy), vec4(hitValue, hitValue, hitValue, 0.0));
}
