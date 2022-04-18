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
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 2, std430) restrict buffer drawBuffer {
  uint drawCount;
  VkDrawIndexedIndirectCommand draws[];
};

shared uint[2] visible;

void main() {
  drawCount = 0;
  barrier();

  uint drawIndex = gl_GlobalInvocationID.x;
  if (drawIndex < scene.drawCount) {
    GPUDraw draw = scene.draws[drawIndex];
    GPUMeshPart part = scene.parts[draw.partIndex];
    uint drawableIndex = draw.drawableIndex;
    if ((visibleBitmasks[drawableIndex / 32] & (1 << (drawableIndex % 32))) != 0) {
      uint outDrawIndex = atomicAdd(drawCount, 1);
      draws[outDrawIndex].firstIndex = part.meshFirstIndex;
      draws[outDrawIndex].indexCount = part.meshIndexCount;
      draws[outDrawIndex].vertexOffset = part.meshVertexOffset;
      draws[outDrawIndex].instanceCount = 1;
      draws[outDrawIndex].firstInstance = drawIndex;
    }
  }
}
