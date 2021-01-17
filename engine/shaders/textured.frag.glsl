#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_normal;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_lightmap_uv;

layout(location = 0) out vec4 out_color;

layout(set = 1, binding = 0) uniform sampler2D tex;
layout(set = 1, binding = 1) uniform sampler2D lightmap;

void main(void) {
  vec4 lighting = texture(lightmap, in_lightmap_uv);
  vec4 tex = texture(tex, in_uv);
  out_color = vec4(lighting.x * tex.x, lighting.y * tex.y, lighting.z * tex.z, 1);
}
