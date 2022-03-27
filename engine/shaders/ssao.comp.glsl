#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 16,
       local_size_y = 4,
       local_size_z = 1) in;

#include "descriptor_sets.h"

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 0, std140) uniform kernel {
  vec4 samples[16];
};
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 1) uniform sampler2D noise;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 2) uniform sampler2D depthMap;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 3) uniform sampler2D normals;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 4, std140) uniform Camera {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
} camera;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 5, r16f) uniform writeonly image2D outputTexture;

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 6, std140) uniform Setup {
  float zNear;
  float zFar;
};

#include "util.h"

// REFERENCE:
// http://john-chapman-graphics.blogspot.com/2013/01/ssao-tutorial.html
// https://learnopengl.com/Advanced-Lighting/SSAO
// https://github.com/SaschaWillems/Vulkan/blob/master/data/shaders/glsl/ssao/ssao.frag

void main() {
  ivec2 texSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));

  float depth = textureLod(depthMap, texCoord, 0).x;
  vec3 fragPos = viewSpacePosition(texCoord, depth, camera.invProj);
  vec3 worldNormal = textureLod(normals, texCoord, 0).xyz;
  vec3 normal = worldSpaceNormalToViewSpace(worldNormal, camera.view);

  vec2 noiseScale = textureSize(depthMap, 0) / textureSize(noise, 0);
  vec3 randomVec = texture(noise, texCoord * noiseScale).xyz * 2.0 - 1.0;

  vec3 tangent = normalize(randomVec - normal * dot(randomVec, normal));
  vec3 bitangent = cross(tangent, normal);
  mat3 TBN = mat3(tangent, bitangent, normal);

  float bias = 0.025;
  float occlusion = 0.0;

  const uint kernelSize = 64;
  const float radius = 0.5;

  for (uint i = 0; i < kernelSize; i++) {
    vec3 samplePos = TBN * samples[i].xyz;
    samplePos = fragPos + samplePos * radius;

    vec4 offset = vec4(samplePos, 1.0);
    offset.y = -offset.y;
    offset = camera.proj * offset;
    offset.xy /= offset.w;
    offset.xy = offset.xy * 0.5 + 0.5;

    float sampleDepth = textureLod(depthMap, offset.xy, 0).x;
    float sampleZ = -linearizeDepth(sampleDepth, zNear, zFar);

    float rangeCheck = smoothstep(0.0, 1.0, radius / abs(fragPos.z - sampleZ));
    occlusion += (sampleZ >= samplePos.z + bias ? 1.0 : 0.0) * rangeCheck;
  }
  occlusion = 1.0 - (occlusion / kernelSize);
  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  imageStore(outputTexture, storageTexCoord, vec4(occlusion, 0.0, 0.0, 0.0));
}
