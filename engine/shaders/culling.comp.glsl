#version 450
#extension GL_GOOGLE_include_directive : enable
#extension GL_KHR_shader_subgroup_basic : enable
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

#include "util.inc.glsl"
#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"

layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 0, std430) readonly restrict buffer sceneBuffer {
  GPUScene scene;
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 1, std430) writeonly restrict buffer visibleBuffer {
  uint visibleBitmasks[];
};

shared uint[2] visible;

void main() {
  if (subgroupElect()) {
    visible[0] = 0;
    visible[1] = 0;
  }
  barrier();
  uint drawableIndex = gl_GlobalInvocationID.x;
  bool isVisible = false;
  if (drawableIndex < scene.drawableCount) {
    // TODO: Check visibility for the drawable
    isVisible = true;
  }
  if (isVisible) {
    atomicOr(visible[gl_LocalInvocationID.x / 32], 1 << (gl_LocalInvocationID.x % 32));
  }
  barrier();
  if (subgroupElect()) {
    visibleBitmasks[gl_WorkGroupID.x * 2] = visible[0];
    visibleBitmasks[gl_WorkGroupID.x * 2 + 1] = visible[1];
  }
}
