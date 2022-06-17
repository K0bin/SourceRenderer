layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, r16f) uniform writeonly image2D outputTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler2D inputTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D history;
#ifndef VISIBILITY_BUFFER
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform sampler2D motionTex;
#else
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3, r32ui) readonly uniform uimage2D primitiveIds;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 4, rg16) readonly uniform image2D barycentrics;
#include "frame_set.inc.glsl"
#include "vis_buf.inc.glsl"
#endif

void main() {
  ivec2 inputTexSize = textureSize(inputTexture, 0);
  ivec2 outputTexSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= outputTexSize.x || gl_GlobalInvocationID.y >= outputTexSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(outputTexSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(outputTexSize.y));
  vec2 texel = vec2(1.0 / float(inputTexSize.x), 1.0 / float(inputTexSize.y));
  float sum = 0.0;
  const int kernelSize = 4;
  // TODO: reduce samples using shared memory
  for (int x = 0; x < kernelSize; x++) {
    for (int y = 0; y < kernelSize; y++) {
      vec2 offset = vec2(float(x - kernelSize / 2), float(y - kernelSize / 2));
      sum += texture(inputTexture, texCoord + offset * texel).r;
    }
  }
  sum /= kernelSize * kernelSize;

  sum *= 0.3;

  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));

  #ifndef VISIBILITY_BUFFER
  vec2 motion = texture(motionTex, texCoord).xy;
  #else
  uint id = imageLoad(primitiveIds, storageTexCoord).x;
  vec2 barycentrics = imageLoad(barycentrics, storageTexCoord).xy;
  vec2 motion = getMotionVector(id, barycentrics, camera, oldCamera);
  #endif
  vec2 historyTexCoord = texCoord - motion;
  sum += texture(history, historyTexCoord).r * 0.7;

  imageStore(outputTexture, storageTexCoord, vec4(sum, 0.0, 0.0, 0.0));
}
