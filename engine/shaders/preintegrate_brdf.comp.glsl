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
#include "pbr.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, rg16) uniform coherent writeonly image2D outputTexture;

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

const uint NumSamples = 1024;
const float InvNumSamples = 1.0 / float(NumSamples);
vec2 sampleHammersley(uint i)
{
	return vec2(i * InvNumSamples, radicalInverse(i));
}

// Returns a halfway vector that was sampled according to GGX
vec3 importantSampleGGX(vec2 uniformSample, float roughness) {
    float phi = 2.0 * PI * uniformSample.x;
    float u = uniformSample.y;
    float a = roughness * roughness;
    float theta = acos(sqrt((1.0 - u) / (1.0 + (a * a - 1.0) * u)));
    return vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
}

vec2 integrateBRDF(float roughness, float normalDotViewDir) {
    vec3 viewDir = vec3(
        sqrt(1.0 - normalDotViewDir * normalDotViewDir),
        0.0,
        normalDotViewDir
    );

    float a = 0;
    float b = 0;
    for (uint i = 0; i < NumSamples; i++) {
        vec2 uniformSample = sampleHammersley(i);
		vec3 halfwayLocal = importantSampleGGX(uniformSample, roughness);
		vec3 lightDir = 2.0 * dot(viewDir, halfwayLocal) * halfwayLocal - viewDir;

		float normalDotLightDir = clamp(lightDir.z, 0.0, 1.0);
		float normalDotHalfway = clamp(halfwayLocal.z, 0.0, 1.0);
		float viewDotHalfway = clamp(dot(viewDir, halfwayLocal), 0.0, 1.0);

		if (normalDotLightDir > 0.0) {
		    float geometry = geometrySmithBasic(normalDotViewDir, normalDotLightDir, roughness);
		    float geometryVis = geometry * viewDotHalfway / (normalDotHalfway * normalDotViewDir);
		    float fc = pow(1.0 - viewDotHalfway, 5.0);
		    a += (1.0 - fc) * geometryVis;
		    b += fc * geometryVis;
		}
    }

    return vec2(a, b) / NumSamples;
}

// Adapted from "Real Shading in Unreal Engine 4", presented by Brian Karis (Epic Games) at Siggraph 2013
void main(void) {
    ivec2 imgSize = imageSize(outputTexture);
    vec2 pos = gl_GlobalInvocationID.xy / vec2(imgSize);
    vec2 value = integrateBRDF(pos.x, pos.y);

    if (gl_GlobalInvocationID.x < imgSize.x && gl_GlobalInvocationID.y < imgSize.y) {
	    imageStore(outputTexture, ivec2(gl_GlobalInvocationID.x, 1.0 - gl_GlobalInvocationID.y), vec4(value, 0.0, 0.0));
	}
}
