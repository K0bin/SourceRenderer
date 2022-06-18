#version 450
#extension GL_GOOGLE_include_directive : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform writeonly image2D outputTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler2D frame;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D ssr;
// layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform sampler2D ssao;

#define CS
#include "util.inc.glsl"

// #include "frame_set.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3, std140) uniform ConfigUBO {
  float gamma;
  float exposure;
};

vec3 aces(vec3 x) {
    float a = 2.51;
    float b = 0.03;
    float c = 2.43;
    float d = 0.59;
    float e = 0.14;
    return clamp((x*(a*x+b))/(x*(c*x+d)+e), 0.0, 1.0);
}

void main() {
  ivec2 texSize = imageSize(outputTexture);
  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  if (storageTexCoord.x >= texSize.x || storageTexCoord.y >= texSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(storageTexCoord.x) + 0.5) / float(texSize.x), (float(storageTexCoord.y) + 0.5) / float(texSize.y));
  vec3 color = texture(frame, texCoord).xyz;
  vec4 reflection = texture(ssr, texCoord);
  color = mix(color, reflection.xyz, reflection.w);

  color *= exposure;
  vec3 toneMapped = aces(color);
  vec3 gammaCorrected = pow(toneMapped, vec3(1.0 / gamma));

  imageStore(outputTexture, storageTexCoord, vec4(gammaCorrected, 1.0));
}
