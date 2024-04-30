#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

layout(location = 0) out vec2 out_uv;

void main(void) {
  vec2 coord = vec2(
    float(gl_VertexIndex & 2),
    float(gl_VertexIndex & 1) * 2.0
  );

  out_uv = coord;
  gl_Position = vec4(coord * 2.0 - 1.0, 0.0, 1.0);
}
