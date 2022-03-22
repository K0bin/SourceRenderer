#version 450
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.h"

layout(local_size_x = 16,
       local_size_y = 16,
       local_size_z = 1) in;

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 0, rgba8) uniform image2D frame;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 1, rgba8) uniform writeonly image2D outputTexture;

void main() {
    ivec2 texCoord = ivec2(gl_GlobalInvocationID.xy);

    /*
      0 -1  0
      -1  5 -1
      0 -1  0
    */
    uvec2 frameSize = imageSize(frame);
    vec3 color = imageLoad(frame, texCoord).xyz;
    uint sampleCount = 0;
    vec3 sharpened = vec3(0.0);
    for (int kernelX = 0; kernelX < 2; kernelX++) {
      ivec2 coord = texCoord + ivec2(kernelX * 2 - 1, 0);
      if (coord.x < 0 || coord.x > frameSize.x || coord.y < 0 || coord.y > frameSize.y) {
        continue;
      }
      sampleCount++;
      sharpened -= imageLoad(frame, coord).xyz;
    }
    for (int kernelY = 0; kernelY < 2; kernelY++) {
      ivec2 coord = texCoord + ivec2(0, kernelY * 2 - 1);
      if (coord.x < 0 || coord.x > frameSize.x || coord.y < 0 || coord.y > frameSize.y) {
        continue;
      }
      sampleCount++;
      sharpened -= imageLoad(frame, coord).xyz;
    }
    sharpened += float(sampleCount) * color;

    float sharpeningIntensity = 0.3;
    vec3 finalColor = mix(color, sharpened, sharpeningIntensity);
    imageStore(outputTexture, texCoord, vec4(finalColor, 1.0));
}
