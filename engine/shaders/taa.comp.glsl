#version 450

layout(set = 0, binding = 0) uniform sampler2D frame;
layout(set = 0, binding = 1) uniform sampler2D history;
layout(set = 0, binding = 2, rgba8) uniform writeonly image2D outputTexture;
layout(set = 0, binding = 3) uniform sampler2D motion;

const int HISTORY_FRAMES = 8;

vec3 clamp(vec3 color, vec2 texCoord, ivec2 textureSize, vec3 historyColor) {
  vec2 pixel = vec2(1.0 / float(textureSize.x), 1.0 / float(textureSize.y));
  vec3 neighborMin = color;
  vec3 neighborMax = color;
  for (int i = 0; i < 3; i++) {
    vec2 coord = texCoord + vec2(float(-1 + i), -1.0) * pixel;
    if (coord.x < 0.0 || coord.x > 1.0 || coord.y < 0.0 || coord.y > 1.0) {
      continue;
    }
    vec3 sampleColor = texture(frame, coord).xyz;
    neighborMax = max(neighborMax, sampleColor);
    neighborMin = min(neighborMin, sampleColor);
  }
  for (int i = 0; i < 3; i++) {
    vec2 coord = texCoord + vec2(float(-1 + i), 1.0) * pixel;
    if (coord.x < 0.0 || coord.x > 1.0 || coord.y < 0.0 || coord.y > 1.0) {
      continue;
    }
    vec3 sampleColor = texture(frame, coord).xyz;
    neighborMax = max(neighborMax, sampleColor);
    neighborMin = min(neighborMin, sampleColor);
  }
  for (int i = 0; i < 2; i++) {
    vec2 coord = texCoord + vec2(float(-1 + i * 2), 0.0) * pixel;
    if (coord.x < 0.0 || coord.x > 1.0 || coord.y < 0.0 || coord.y > 1.0) {
      continue;
    }
    vec3 sampleColor = texture(frame, coord).xyz;
    neighborMax = max(neighborMax, sampleColor);
    neighborMin = min(neighborMin, sampleColor);
  }
  return clamp(historyColor, neighborMin, neighborMax);
}

void main() {
    ivec2 textureSize = textureSize(frame, 0);
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(textureSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(textureSize.y));
    ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
    vec3 color = texture(frame, texCoord).xyz;

    vec2 historyTexCoord = texCoord - texture(motion, texCoord).xy;
    vec3 historyColor = clamp(color, texCoord, textureSize, texture(history, historyTexCoord).xyz);
    bool useHistory = historyTexCoord.x >= 0.0 && historyTexCoord.x <= 1.0 && historyTexCoord.y >= 0.0 && historyTexCoord.y <= 1.0;
    float taaFactor = useHistory ? (1.0 - 1.0 / float(HISTORY_FRAMES)) : 0.0;
    imageStore(outputTexture, storageTexCoord, vec4(color * (1.0 - taaFactor) + historyColor * taaFactor, 1.0));
}
