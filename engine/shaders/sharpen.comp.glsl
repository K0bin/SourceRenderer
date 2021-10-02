#version 450

layout(local_size_x = 16,
       local_size_y = 16,
       local_size_z = 1) in;

layout(set = 0, binding = 0) uniform sampler2D frame;
layout(set = 0, binding = 1, rgba8) uniform writeonly image2D outputTexture;

void main() {
    ivec2 textureSize = textureSize(frame, 0);
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(textureSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(textureSize.y));
    ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));

    /*
      0 -1  0
      -1  5 -1
      0 -1  0
    */
    vec2 pixel = vec2(1.0 / float(textureSize.x), 1.0 / float(textureSize.y));
    vec3 color = texture(frame, texCoord).xyz;
    vec3 sharpened = 5.0 * color;
    for (int kernelX = 0; kernelX < 2; kernelX++) {
      vec2 coord = texCoord + pixel * vec2(float(kernelX * 2 - 1), 0.0);
      if (coord.x < 0.0 || coord.x > 1.0 || coord.y < 0.0 || coord.y > 1.0) {
        continue;
      }
      sharpened -= texture(frame, coord).xyz;
    }
    for (int kernelY = 0; kernelY < 2; kernelY++) {
      vec2 coord = texCoord + pixel * vec2(0.0, float(kernelY * 2 - 1));
      if (coord.x < 0.0 || coord.x > 1.0 || coord.y < 0.0 || coord.y > 1.0) {
        continue;
      }
      sharpened -= texture(frame, coord).xyz;
    }

    float sharpeningIntensity = 0.3;
    vec3 finalColor = mix(color, sharpened, sharpeningIntensity);
    imageStore(outputTexture, storageTexCoord, vec4(finalColor, 1.0));
}
