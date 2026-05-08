#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable

#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

#include "descriptor_sets.inc.glsl"
#include "util.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
  Camera camera;
};

layout (set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform samplerCube envMapSpecular;

void main(void) {
	vec4 ndc = vec4(in_uv.x * 2.0 - 1.0, (1.0 - in_uv.y) * 2.0 - 1.0, 1.0, 1.0);
	vec4 viewPos = camera.invProj * ndc;
    mat4 reducedCam = mat4(mat3(camera.invView));
	vec3 worldDir = (reducedCam * viewPos).xyz;
    out_color = textureLod(envMapSpecular, worldDir, 0.0);
}