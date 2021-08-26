#version 450

layout(set = 0, binding = 0, std140) uniform kernel {
  vec4 samples[64];
};
layout(set = 0, binding = 1) uniform sampler2D noise;
layout(set = 0, binding = 2) uniform sampler2D depthMap;
layout(set = 0, binding = 3) uniform sampler2D normals;
layout(set = 0, binding = 4, std140) uniform Camera {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
} camera;
layout(set = 0, binding = 5, r16f) uniform writeonly image2D outputTexture;

// REFERENCE:
// http://john-chapman-graphics.blogspot.com/2013/01/ssao-tutorial.html
// https://learnopengl.com/Advanced-Lighting/SSAO

void main() {
  ivec2 texSize = textureSize(depthMap, 0);
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
  float depth = texture(depthMap, texCoord).r;
  vec4 screenSpacePosition = vec4(texCoord * 2.0 - 1.0, depth, 1.0);
  vec4 viewPosTemp = camera.invProj * screenSpacePosition;
  vec3 viewPos = viewPosTemp.xyz / viewPosTemp.w;

  vec2 noiseScale = texSize / textureSize(noise, 0);
  vec4 normal = vec4(texture(normals, texCoord).xyz, 0.0);
  vec3 viewNormal = (camera.view * normal).xyz;
  viewNormal = normalize(viewNormal);
  viewNormal.xz = -viewNormal.xz; // NO IDEA WHY
  vec3 randomVec = texture(noise, texCoord * noiseScale).xyz;

  vec3 tangent = normalize(randomVec - viewNormal * dot(randomVec, viewNormal));
  vec3 biTangent = cross(viewNormal, tangent);
  mat3 TBN = mat3(tangent, biTangent, viewNormal);

  float bias = 0.025;

  float occlusion = 0.0;

  const uint kernelSize = 16;
  //const float radius = 0.5;
  const float radius = 2.5;

  for (uint i = 0; i < kernelSize; i++) {
    vec3 samplePos = TBN * samples[i].xyz;
    samplePos = viewPos + samplePos * radius;

    vec4 offset = vec4(samplePos, 1.0);
    offset = camera.proj * offset;
    offset.xyz /= offset.w;
    offset.xy = offset.xy * 0.5 + 0.5;

    float sampleDepth = texture(depthMap, offset.xy).r;
    vec4 sampleSS = vec4(offset.xy * 2.0 - 1.0, sampleDepth, 1.0);
    vec4 sampleViewTemp = camera.invProj * sampleSS;
    vec3 sampleView = sampleViewTemp.xyz / sampleViewTemp.w;
    // TODO: linearize depth instead of calculating view space position
    // we only need the depth
    
    float rangeCheck = smoothstep(0.0, 1.0, radius / abs(viewPos.z - sampleView.z));
    occlusion += (sampleView.z >= samplePos.z + bias ? 1.0 : 0.0) * rangeCheck;
  }
  occlusion = 1.0 - (occlusion / kernelSize);
  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  imageStore(outputTexture, storageTexCoord, vec4(occlusion, 0.0, 0.0, 0.0));
}
