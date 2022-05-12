#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 0) uniform sampler2D inputTexture;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 1, r32f) uniform writeonly image2D outputTexture;

layout(push_constant) uniform PushConstantData {
  uint baseWidth;
  uint baseHeight;
  uint mipLevel;
};

void main() {
  ivec2 texSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
  float maxValue = textureLod(inputTexture, texCoord, 0).x;
  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  imageStore(outputTexture, storageTexCoord, vec4(maxValue, 0.0, 0.0, 0.0));
}
