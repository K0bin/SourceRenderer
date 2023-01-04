#version 450
#extension GL_GOOGLE_include_directive : enable
//#extension GL_EXT_debug_printf : enable
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

#include "util.inc.glsl"
#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"

// DEBUG uses 0 count draws instead of atomically adding them to ensure stable sorting.
// Useful for debugging in Renderdoc!
//#define DEBUG

struct VkDrawIndexedIndirectCommand {
  uint indexCount;
  uint instanceCount;
  uint firstIndex;
  uint vertexOffset;
  uint firstInstance;
};

#include "frame_set.inc.glsl"

layout(std430, set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, std430) readonly restrict buffer visibleBuffer {
  uint visibleBitmasks[];
};
layout(std430, set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1, std430) restrict buffer drawBuffer {
  uint drawCount;
  VkDrawIndexedIndirectCommand draws[];
};

shared uint[2] visible;

void main() {
  #ifdef DEBUG
  if (gl_LocalInvocationIndex == 0) {
    drawCount = scene.drawCount;
  }
  barrier();
  #endif

  uint drawIndex = gl_GlobalInvocationID.x;
  if (drawIndex < scene.drawCount) {
    GPUDraw draw = scene_draws[drawIndex];
    GPUMeshPart part = scene_parts[draw.partIndex];
    uint drawableIndex = draw.drawableIndex;
    bool drawableVisible = (visibleBitmasks[drawableIndex / 32] & (1 << (drawableIndex % 32))) != 0;
    #ifndef DEBUG
    bool emitDraw = drawableVisible;
    #else
    bool emitDraw = true;
    #endif
    if (emitDraw) {
    #ifndef DEBUG
      uint outDrawIndex = atomicAdd(drawCount, 1);
      uint instanceCount = 1;
    #else
      uint outDrawIndex = drawIndex;
      uint instanceCount = drawableVisible ? 1 : 0;
    #endif
      draws[outDrawIndex].firstIndex = part.meshFirstIndex;
      draws[outDrawIndex].indexCount = part.meshIndexCount;
      draws[outDrawIndex].vertexOffset = part.meshVertexOffset;
      draws[outDrawIndex].instanceCount = instanceCount;
      draws[outDrawIndex].firstInstance = drawIndex;
    }
  }
}
