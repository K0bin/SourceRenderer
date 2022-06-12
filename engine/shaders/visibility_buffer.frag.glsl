#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable
#extension GL_NV_fragment_shader_barycentric : require
#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

layout(location = 0) in flat uint in_drawIndex;
layout(location = 1) in flat uint in_firstIndex;

layout(location = 0) out uint out_primitiveId;
layout(location = 1) out vec2 out_barycentrics;

void main(void) {
  out_primitiveId = uint(((in_drawIndex & 0xffff) << 16) | (gl_PrimitiveID & 0xffff));
  out_barycentrics = gl_BaryCoordNV.xy;
}
