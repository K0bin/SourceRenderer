#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_normal;
//layout(location = 3) in vec2 in_lightmap_uv;
//layout(location = 4) in float in_alpha;

layout(location = 0) out vec3 out_normal;
layout(location = 1) out float out_density;
layout(location = 2) out vec3 out_worldPosition;

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
  vec3 size;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform sampler3D densityMap;

void main(void) {
  vec4 pos = vec4(in_pos, 1);

  mat4 mvp = camera.viewProj * model;

  out_normal = normalize((model * vec4(in_normal, 0)).xyz);
  out_density = texture(densityMap, in_pos / size).x;
  out_density = max(out_density, texture(densityMap, (in_pos - in_normal) / size).x);
  //out_density = max(out_density, texture(densityMap, (in_pos - in_normal * 2.0) / size).x);

  gl_Position = mvp * pos;
  out_worldPosition = gl_Position.xyz;
}
