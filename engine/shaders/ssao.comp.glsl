#version 450

layout(set = 0, binding = 0, std140) uniform kernel {
  vec4 samples[16];
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

vec3 viewSpacePosition(vec2 uv) {
  float depth = texture(depthMap, uv).r;
  vec4 screenSpacePosition = vec4(uv * 2.0 - 1.0, depth, 1.0);
  vec4 viewSpaceTemp = camera.invProj * screenSpacePosition;
  return viewSpaceTemp.xyz / viewSpaceTemp.w;
}

vec3 worldSpaceNormalToViewSpace(vec2 uv) {
  vec3 worldSpaceNormal = texture(normals, uv).xyz;
  vec3 viewSpaceNormal = (transpose(inverse(camera.view)) * vec4(worldSpaceNormal, 0.0)).xyz;
  viewSpaceNormal.y = -viewSpaceNormal.y;
  return viewSpaceNormal;
}

// REFERENCE:
// http://john-chapman-graphics.blogspot.com/2013/01/ssao-tutorial.html
// https://learnopengl.com/Advanced-Lighting/SSAO
// https://github.com/SaschaWillems/Vulkan/blob/master/data/shaders/glsl/ssao/ssao.frag

void main() {
  ivec2 texSize = imageSize(outputTexture);
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));

  vec3 fragPos = viewSpacePosition(texCoord);
  vec3 normal = worldSpaceNormalToViewSpace(texCoord);

  vec2 noiseScale = textureSize(depthMap, 0) / textureSize(noise, 0);

  vec3 randomVec = texture(noise, texCoord * noiseScale).xyz;

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
    offset = camera.proj * offset;
    offset.xyz /= offset.w;
    offset.xy = offset.xy * 0.5 + 0.5;

    // TODO: linearize depth instead of calculating view space position
    // we only need the depth
    float sampleDepth = viewSpacePosition(offset.xy).z;
    
    float rangeCheck = smoothstep(0.0, 1.0, radius / abs(fragPos.z - sampleDepth));
    occlusion += (sampleDepth >= samplePos.z + bias ? 1.0 : 0.0) * rangeCheck;
  }
  occlusion = 1.0 - (occlusion / kernelSize);
  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  imageStore(outputTexture, storageTexCoord, vec4(occlusion, 0.0, 0.0, 0.0));
}
