#version 450
#extension GL_GOOGLE_include_directive : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 0) uniform sampler2D frame;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 1) uniform sampler2D history; // NEEDS LINEAR SAMPLER!
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 2, rgba8) uniform writeonly image2D outputTexture;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 3) uniform sampler2D motionTex;
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 4) uniform sampler2D depthMap;

#define CS
#include "util.inc.glsl"

// TODO: improve https://www.elopezr.com/temporal-aa-and-the-quest-for-the-holy-trail/
// reference: https://sugulee.wordpress.com/2021/06/21/temporal-anti-aliasingtaa-tutorial/
// https://github.com/playdeadgames/temporal/blob/master/Assets/Shaders/TemporalReprojection.shader#L212

vec3 historyClamp(vec3 color, vec2 texCoord, ivec2 textureSize, vec3 historyColor) {
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

vec2 chooseTexCoordClosestToCamera(sampler2D depthMap, vec2 texCoord) {
  uvec2 texSize = textureSize(depthMap, 0);
  float minDepth = 1;
  vec2 minTexCoord = texCoord;
  for (uint x = -1; x <= 1; x++) {
    for (uint y = -1; y <= 1; y++) {
      vec2 samplePos = texCoord + vec2(float(x), float(y)) * vec2(texSize);
      float depthSample = textureLod(depthMap, samplePos, 0).x;
      if (depthSample < minDepth) {
        minDepth = depthSample;
        minTexCoord = samplePos;
      }
    }
  }
  return minTexCoord;
}

#define CATMULL_ROM_IGNORE_CORNERS
// https://gist.github.com/TheRealMJP/c83b8c0f46b63f3a88a5986f4fa982b1
vec3 catmullRom(sampler2D tex, vec2 texCoord) {
  // We're going to sample a a 4x4 grid of texels surrounding the target UV coordinate. We'll do this by rounding
  // down the sample location to get the exact center of our "starting" texel. The starting texel will be at
  // location [1, 1] in the grid, where [0, 0] is the top left corner.
  vec2 texSize = textureSize(tex, 0);
  vec2 samplePos = texCoord * texSize;
  vec2 texPosCenter = floor(samplePos - 0.5) + 0.5;

  // Compute the fractional offset from our starting texel to our original sample location, which we'll
  // feed into the Catmull-Rom spline function to get our filter weights.
  vec2 f = samplePos - texPosCenter;

  // Compute the Catmull-Rom weights using the fractional offset that we calculated earlier.
  // These equations are pre-expanded based on our knowledge of where the texels will be located,
  // which lets us avoid having to evaluate a piece-wise function.
  vec2 w0 = f * (-0.5f + f * (1.0f - 0.5f * f));
  vec2 w1 = 1.0f + f * f * (-2.5f + 1.5f * f);
  vec2 w2 = f * (0.5f + f * (2.0f - 1.5f * f));
  vec2 w3 = f * f * (-0.5f + 0.5f * f);

  // Work out weighting factors and sampling offsets that will let us use bilinear filtering to
  // simultaneously evaluate the middle 2 samples from the 4x4 grid.
  vec2 w12 = w1 + w2;
  vec2 offset12 = w2 / (w1 + w2);

  // Compute the final UV coordinates we'll use for sampling the texture
  vec2 texPos0 = texPosCenter - 1;
  vec2 texPos3 = texPosCenter + 2;
  vec2 texPos12 = texPosCenter + offset12;

  texPos0 /= texSize;
  texPos3 /= texSize;
  texPos12 /= texSize;

  vec3 result = vec3(0.0);
  result += textureLod(tex, vec2(texPos12.x, texPos0.y), 0.0).xyz * w12.x * w0.y;

  result += textureLod(tex, vec2(texPos0.x, texPos12.y), 0.0).xyz * w0.x * w12.y;
  result += textureLod(tex, vec2(texPos12.x, texPos12.y), 0.0).xyz * w12.x * w12.y;
  result += textureLod(tex, vec2(texPos3.x, texPos12.y), 0.0).xyz * w3.x * w12.y;

  result += textureLod(tex, vec2(texPos12.x, texPos3.y), 0.0).xyz * w12.x * w3.y;

  #ifndef CATMULL_ROM_IGNORE_CORNERS
  result += textureLod(tex, vec2(texPos0.x, texPos0.y), 0.0).xyz * w0.x * w0.y;
  result += textureLod(tex, vec2(texPos3.x, texPos0.y), 0.0).xyz * w3.x * w0.y;
  result += textureLod(tex, vec2(texPos0.x, texPos3.y), 0.0).xyz * w0.x * w3.y;
  result += textureLod(tex, vec2(texPos3.x, texPos3.y), 0.0).xyz * w3.x * w3.y;
  #endif

  // Ignore the corner samples. (Filmic SMAA: Sharp Morphological and Temporal Antialiasing, Jorge Jimenez)

  return result;
}

void main() {
    ivec2 texSize = textureSize(frame, 0);
    if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
      return;
    }
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
    ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
    vec3 color = textureLod(frame, texCoord, 0).xyz;

    vec2 motionTexCoord = chooseTexCoordClosestToCamera(depthMap, texCoord);

    vec2 motion = textureLod(motionTex, motionTexCoord, 0).xy;
    vec2 historyTexCoord = texCoord - motion;
    if (historyTexCoord.x < 0.0 || historyTexCoord.x > 1.0 || historyTexCoord.y < 0.0 || historyTexCoord.y > 1.0) {
      imageStore(outputTexture, storageTexCoord, vec4(color, 1.0));
      return;
    }

    vec3 historyColor = catmullRom(history, historyTexCoord);
    vec3 clampedHistoryColor = historyClamp(color, texCoord, texSize, historyColor);
    vec3 clampDiff = abs(clampedHistoryColor) / abs(historyColor);

    float lum = luminance(color);
    float historyLum = luminance(clampedHistoryColor);
    float luminanceFactor = abs(lum - historyLum) / max(lum, max(historyLum, 0.2));
    luminanceFactor = luminanceFactor * luminanceFactor;

    float historyFactor = mix(0.8, 0.999, luminanceFactor);
    vec3 finalColor = mix(color, clampedHistoryColor, historyFactor);
    imageStore(outputTexture, storageTexCoord, vec4(finalColor, 1.0));
}
