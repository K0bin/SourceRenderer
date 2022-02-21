#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.h"

const float PI = 3.14159265359;

layout(location = 0) in vec3 in_worldPosition;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_lightmap_uv;

layout(location = 0) out vec4 out_color;

layout(set = DESCRIPTOR_SET_PER_MATERIAL, binding = 0) uniform sampler2D albedo;

void main(void) {
  //out_color = vec4(texture(albedo, in_uv).xyz + vec3(1.0, 0.0, 0.2), 1.0);
  out_color = vec4(in_uv, 0.0, 1.0);
  //out_color = vec4(in_worldPosition, 1.0);
  //out_color = vec4(in_normal / 2.0 - 0.5, 1.0);
}
