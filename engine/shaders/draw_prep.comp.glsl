#version 450
#extension GL_GOOGLE_include_directive : enable
#extension GL_KHR_shader_subgroup_basic : enable
//#extension GL_EXT_debug_printf : enable
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

#include "util.inc.glsl"
#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"

struct VkDrawIndexedIndirectCommand {
  uint indexCount;
  uint instanceCount;
  uint firstIndex;
  uint vertexOffset;
  uint firstInstance;
};

layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 0, std430) readonly restrict buffer sceneBuffer {
  GPUScene scene;
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 1, std430) readonly restrict buffer visibleBuffer {
  uint visibleBitmasks[];
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 2, std430) writeonly restrict buffer drawBuffer {
  uint drawCount;
  VkDrawIndexedIndirectCommand draws[];
};

shared uint[2] visible;

void main() {
  drawCount = 0;
  barrier();

  uint partIndex = gl_GlobalInvocationID.x;
  if (partIndex < scene.partCount) {
    GPUDrawableRange part = scene.parts[partIndex];
    uint drawableIndex = part.drawableIndex;
    if ((visibleBitmasks[drawableIndex / 32] & (1 << (part.drawableIndex % 32))) != 0) {
      uint drawIndex = atomicAdd(drawCount, 1);
      draws[drawIndex].firstIndex = part.meshFirstIndex;
      draws[drawIndex].indexCount = part.meshIndexCount;
      draws[drawIndex].vertexOffset = part.meshVertexOffset;
      draws[drawIndex].instanceCount = 1;
      draws[drawIndex].firstInstance = partIndex;
    }
  }
}
