#version 450

layout(set = 0, binding = 0, r16f) uniform writeonly image2D outputTexture;
layout(set = 0, binding = 1) uniform sampler2D inputTexture;

void main() {
  ivec2 texSize = textureSize(inputTexture, 0);
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
  vec2 texel = vec2(1.0 / float(texSize.x), 1.0 / float(texSize.y));
  float sum = 0.0;
  const int kernelSize = 5;
  // TODO: reduce samples using shared memory
  for (int x = 0; x < kernelSize; x++) {
    for (int y = 0; y < kernelSize; y++) {
      vec2 offset = vec2(float(x - kernelSize / 2), float(y - kernelSize / 2));
      sum += texture(inputTexture, texCoord + offset * texel).r;
    }
  }
  sum /= kernelSize * kernelSize;

  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  imageStore(outputTexture, storageTexCoord, vec4(sum, 0.0, 0.0, 0.0));
}
