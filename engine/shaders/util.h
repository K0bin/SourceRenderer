#ifndef UTIL_H
#define UTIL_H

float linearizeDepth(float d, float zNear, float zFar) {
  return 2.0 * zNear * zFar / (zFar + zNear - d * (zFar - zNear));
}

vec3 worldSpacePosition(vec2 uv, float depth, mat4 invViewProj) {
  vec4 clipSpacePosition = vec4(uv * 2.0 - 1.0, depth, 1.0);
  clipSpacePosition.y = -clipSpacePosition.y;
  vec4 worldSpacePosTemp = invViewProj * clipSpacePosition;
  return worldSpacePosTemp.xyz / worldSpacePosTemp.w;
}

vec3 viewSpacePosition(vec2 uv, float depth, mat4 invProj) {
  vec4 screenSpacePosition = vec4(uv * 2.0 - 1.0, depth, 1.0);
  screenSpacePosition.y = -screenSpacePosition.y;
  vec4 viewSpaceTemp = invProj * screenSpacePosition;
  return viewSpaceTemp.xyz / viewSpaceTemp.w;
}

vec3 worldSpaceNormalToViewSpace(vec3 worldSpaceNormal, mat4 view) {
  vec3 viewSpaceNormal = (transpose(inverse(view)) * vec4(worldSpaceNormal, 0.0)).xyz;
  return viewSpaceNormal;
}

#ifdef FS
vec3 reconstructNormalFS(vec2 uv, float depth, mat4 invViewProj) {
  vec3 position = worldSpacePosition(uv, depth, invViewProj);
  return normalize(cross(dFdx(position), dFdy(position)));
}
#endif

#ifdef CS
// TODO: https://wickedengine.net/2019/09/22/improved-normal-reconstruction-from-depth/
vec3 reconstructNormalCS(sampler2D depth, vec2 uv, mat4 invViewProj) {
  vec2 depthSize = textureSize(depth, 0);
  vec2 depthTexelSize = 1.0 / depthSize;
  vec2 uv0 = uv;
  vec2 uv1 = uv + vec2(1.0, 0.0) * depthTexelSize;
  vec2 uv2 = uv + vec2(0.0, 1.0) * depthTexelSize;
  float depth0 = textureLod(depth, uv0, 0).x;
  float depth1 = textureLod(depth, uv1, 0).x;
  float depth2 = textureLod(depth, uv1, 0).x;

  vec3 pos0 = worldSpacePosition(uv0, depth0, invViewProj);
  vec3 pos1 = worldSpacePosition(uv1, depth1, invViewProj);
  vec3 pos2 = worldSpacePosition(uv2, depth2, invViewProj);
  return normalize(cross(pos2 - pos0, pos1 - pos0));
}
#endif

#endif
