#version 460

// Adapted from:
// Physically Based Rendering
// Copyright (c) 2017-2018 Michał Siejak
// MIT Licensed

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_KHR_shader_subgroup_vote : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable

#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#define CS
#include "descriptor_sets.inc.glsl"
#include "util.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform samplerCube envMap;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1, rgba8) uniform coherent writeonly imageCube outputTexture;

// from http://holger.dammertz.org/stuff/notes_HammersleyOnHemisphere.html
// Hacker's Delight, Henry S. Warren, 2001
float radicalInverse(uint bits) {
  bits = (bits << 16u) | (bits >> 16u);
  bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
  bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
  bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
  bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
  return float(bits) * 2.3283064365386963e-10; // / 0x100000000
}

// Sample i-th point from Hammersley point set of NumSamples points total.
//const uint NumSamples = 4096;
const uint NumSamples = 128;
const float InvNumSamples = 1.0 / float(NumSamples);
vec2 sampleHammersley(uint i)
{
	return vec2(i * InvNumSamples, radicalInverse(i));
}

// Uniformly sample point on a hemisphere.
// Cosine-weighted sampling would be a better fit for Lambertian BRDF but since this
// compute shader runs only once as a pre-processing step performance is not *that* important.
// See: "Physically Based Rendering" 2nd ed., section 13.6.1.
const float PI = 3.141592;
const float TwoPI = 2 * PI;
vec3 sampleHemisphere(float u1, float u2)
{
	const float theta = asin(sqrt(u1));
	const float phi = TwoPI * u2;
	return vec3(cos(phi) * sin(theta), sin(theta) * sin(phi), cos(theta));
	//const float u1p = sqrt(max(0.0, 1.0 - u1*u1));
	//return vec3(cos(phi) * u1p, sin(phi) * u1p, u1);
}

// Calculate normalized sampling direction vector based on current fragment coordinates (gl_GlobalInvocationID.xyz).
// This is essentially "inverse-sampling": we reconstruct what the sampling vector would be if we wanted it to "hit"
// this particular fragment in a cubemap.
// See: OpenGL core profile specs, section 8.13.
vec3 getSamplingVector()
{
    vec2 st = gl_GlobalInvocationID.xy/vec2(imageSize(outputTexture));
    vec2 uv = 2.0 * vec2(st.x, 1.0-st.y) - vec2(1.0);

    vec3 ret;
    // Sadly 'switch' doesn't seem to work, at least on NVIDIA.
    if(gl_GlobalInvocationID.z == 0)      ret = vec3(1.0,  uv.y, -uv.x);
    else if(gl_GlobalInvocationID.z == 1) ret = vec3(-1.0, uv.y,  uv.x);
    else if(gl_GlobalInvocationID.z == 2) ret = vec3(uv.x, 1.0, -uv.y);
    else if(gl_GlobalInvocationID.z == 3) ret = vec3(uv.x, -1.0, uv.y);
    else if(gl_GlobalInvocationID.z == 4) ret = vec3(uv.x, uv.y, 1.0);
    else if(gl_GlobalInvocationID.z == 5) ret = vec3(-uv.x, uv.y, -1.0);
    return normalize(ret);
}

mat3 getNormalMat(vec3 normal) {
  vec3 someVec = vec3(1.0, 0.0, 0.0);
  float dd = dot(someVec, normal);
  float normalSomeVecDiffers = step(1e-6, 1.0 - abs(dd));
  vec3 tangent = normalize(mix(vec3(0.0, 1.0, 0.0), cross(someVec, normal), normalSomeVecDiffers));
  vec3 bitangent = cross(normal, tangent);
  return mat3(tangent, bitangent, normal);
}

void main(void) {
    vec3 normal = getSamplingVector();

	mat3 normalMat = getNormalMat(normal);

	// Monte Carlo integration of hemispherical irradiance.
	vec3 irradiance = vec3(0);
	for(uint i=0; i<NumSamples; ++i) {
		vec2 uniformRandomSamples  = sampleHammersley(i);
		vec3 hemisphereDir = sampleHemisphere(uniformRandomSamples.x, uniformRandomSamples.y);
		vec3 worldSpaceDir = normalMat * hemisphereDir;

		irradiance += textureLod(envMap, worldSpaceDir, 0).rgb;
	}
	irradiance /= vec3(NumSamples);

	imageStore(outputTexture, ivec3(gl_GlobalInvocationID), vec4(irradiance, 1.0));
}
