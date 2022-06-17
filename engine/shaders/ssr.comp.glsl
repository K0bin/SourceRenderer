#version 450
#extension GL_GOOGLE_include_directive : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) writeonly uniform image2D outputTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform sampler2D colorTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D depthTexture;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3, std140) uniform CameraUBO {
  Camera camera;
};

#define CS
#include "ssr.inc.glsl"

void main() {
  ivec2 texSize = imageSize(outputTexture);
  if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
    return;
  }
  vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
  ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
  SSRConfig config = SSRConfig(30, 0.5, 10, 0.2);
  vec2 reflectionTexCoord;
  float reflectionIntensity = reflectScreenspace(depthTexture, texCoord, camera, config, reflectionTexCoord);
  if (reflectionIntensity > 0.01) {
    vec3 reflection = textureLod(colorTexture, reflectionTexCoord, 0).xyz;
    imageStore(outputTexture, storageTexCoord, vec4(reflection, 1.0));
  } else {
    imageStore(outputTexture, storageTexCoord, vec4(0.0, 0.0, 0.0, 1.0));
  }
}
