#version 450
#extension GL_GOOGLE_include_directive : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) writeonly uniform image2D outputTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1, r32ui) readonly uniform uimage2D primitiveIds;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2, rg16) readonly uniform image2D barycentrics;
#include "frame_set.inc.glsl"
#include "vis_buf.inc.glsl"

void main() {
  ivec2 outputTexSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= outputTexSize.x || gl_GlobalInvocationID.y >= outputTexSize.y) {
    return;
  }

  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  uint id = imageLoad(primitiveIds, storageTexCoord).x;
  vec2 barycentricsXY = imageLoad(barycentrics, storageTexCoord).xy;
  vec3 barycentrics = vec3(barycentricsXY, 1.0 - barycentricsXY.x - barycentricsXY.y);
  vec2 motion = getMotionVector(id, barycentrics, camera, oldCamera);

  imageStore(outputTexture, storageTexCoord, vec4(motion, 0.0, 0.0));
}
