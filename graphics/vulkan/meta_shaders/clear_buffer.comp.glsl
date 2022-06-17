#version 450
#extension GL_GOOGLE_include_directive : enable
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

#define DESCRIPTOR_SET_VERY_FREQUENT 0

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, std430) writeonly buffer Buffer {
  uint[] data;
};

layout(push_constant) uniform PushConstantData {
  uint size;
  uint value;
};

void main() {
  if (gl_GlobalInvocationID.x < size) {
    data[gl_GlobalInvocationID.x] = value;
  }
}
