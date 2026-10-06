#ifndef FRAME_SET_COMMON_INC_GLSL
#define FRAME_SET_COMMON_INC_GLSL

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(set = DESCRIPTOR_SET_FRAME, binding = 0, std140) uniform CameraUBO {
  Camera camera;
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 1) uniform sampler samplerLinear;
layout(set = DESCRIPTOR_SET_FRAME, binding = 2) uniform sampler samplerNearest;

#endif
