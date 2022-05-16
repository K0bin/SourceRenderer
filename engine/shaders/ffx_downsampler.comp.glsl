#version 450
#extension GL_GOOGLE_include_directive : enable

layout(local_size_x = 256, local_size_y = 1, local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 0) uniform sampler2D inputTexture;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 1, r32f) uniform coherent image2D outputTexture[12];
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 2, std430) restrict buffer counterBuffer {
  uint spdCounterGlobal;
};

 layout(push_constant) uniform SpdConstants {
  uint mips; // needed to opt out earlier if mips are < 12
  uint numWorkGroups; // number of total thread groups, so numWorkGroupsX * numWorkGroupsY * 1
                      // it is important to NOT take the number of slices (z dimension) into account here
                      // as each slice has its own counter!
  vec2 workGroupOffset; // optional - use SpdSetup() function to calculate correct workgroup offset
} spdConstants;

#define A_GPU 1
#define A_GLSL 1


#include "ffx_a.h"

shared AU1 spdCounter;
shared AF1 spdIntermediate[16][16];

vec2 inputSize;
vec2 invInputSize;

AF4 SpdLoadSourceImage(ASU2 p, AU1 slice) {
  AF2 texCoord = p * invInputSize + invInputSize;
  return textureLod(inputTexture, texCoord, 0);
}

AF4 SpdLoad(ASU2 p, AU1 slice) {
  vec2 bounds = inputSize;
  if (p.x > bounds.x || p.y > bounds.y) {
    return vec4(0);
  }
  return imageLoad(outputTexture[5], p);
}

void SpdStore(ASU2 p, AF4 value, AU1 mip, AU1 slice) {
  vec2 bounds = max(vec2(1), vec2(float(uint(inputSize.x) >> (mip + 1)), float(uint(inputSize.y) >> (mip + 1))));
  if (p.x > bounds.x || p.y > bounds.y) {
    return;
  }
  imageStore(outputTexture[mip], p, value);
}

void SpdIncreaseAtomicCounter(AU1 slice){spdCounter = atomicAdd(spdCounterGlobal, 1);}
AU1 SpdGetAtomicCounter() {return spdCounter;}
void SpdResetAtomicCounter(AU1 slice){spdCounterGlobal = 0;}

AF4 SpdLoadIntermediate(AU1 x, AU1 y){return vec4(spdIntermediate[x][y], 0, 0, 1);}
void SpdStoreIntermediate(AU1 x, AU1 y, AF4 value){spdIntermediate[x][y] = value.x;}

AF4 SpdReduce4(AF4 v0, AF4 v1, AF4 v2, AF4 v3){return max(max(v0, v1), max(v2, v3));}

#define SPD_LINEAR_SAMPLER

// #define SPD_NO_WAVE_OPERATIONS // DEBUG

#include "ffx_spd.h"

void main() {
  inputSize = textureSize(inputTexture, 0);
  invInputSize = 1 / inputSize;

  // Call the downsampling function
  // WorkGroupId.z should be 0 if you only downsample a Texture2D!
  SpdDownsample(
    AU2(gl_WorkGroupID.xy),
    AU1(gl_LocalInvocationIndex),
    AU1(spdConstants.mips),
    AU1(spdConstants.numWorkGroups),
    AU1(0)
  );
}