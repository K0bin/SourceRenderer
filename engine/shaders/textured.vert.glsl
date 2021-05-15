#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_lightmap_uv;
layout(location = 4) in float in_alpha;

layout(location = 0) out vec3 out_normal;
layout(location = 1) out vec2 out_uv;
layout(location = 2) out vec2 out_lightmap_uv;

layout(set = 2, binding = 0) uniform LowFrequencyUbo {
  mat4 viewProjection;
};
layout(set = 2, binding = 1) uniform PerFrameUbo {
  mat4 swapchainTransform;
  vec2 jitterPoint;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
  mat4 model;
};

void main(void) {
  vec4 pos = vec4(in_pos, 1);;

  out_uv = in_uv;
  out_lightmap_uv = in_lightmap_uv;
  out_normal = in_normal;
  mat4 mvp = (swapchainTransform * (viewProjection * model));
  mat4 jitterMat;
  jitterMat[0] = vec4(1.0, 0.0, 0.0, 0.0);
  jitterMat[1] = vec4(0.0, 1.0, 0.0, 0.0);
  jitterMat[2] = vec4(0.0, 0.0, 1.0, 0.0);
  jitterMat[3] = vec4(jitterPoint.x, jitterPoint.y, 0.0, 1.0);
  vec4 jitteredPoint = (jitterMat * mvp) * pos;
  jitteredPoint.y = -jitteredPoint.y;
  gl_Position = jitteredPoint;
}
