#ifndef UTIL_H
#define UTIL_H

float linearizeDepth(float d, float zNear, float zFar) {
  return 2.0 * zNear * zFar / (zFar + zNear - d * (zFar - zNear));
}

vec3 worldSpacePosition(vec2 uv, float depth, mat4 invViewProj) {
  vec4 clipSpacePosition = vec4(uv * 2.0 - 1.0, depth, 1.0);
  clipSpacePosition.y = -clipSpacePosition.y;
  vec4 worldSpacePosTemp = (camera.invView * camera.invProj) * clipSpacePosition;
  return worldSpacePosTemp.xyz / worldSpacePosTemp.w;
}

#ifdef FS
vec3 reconstructNormalFS(vec2 uv, float depth, mat4 invViewProj) {
  vec3 position = worldSpacePosition(uv, depth, invViewProj);
  return normalize(cross(dFdx(position), dFdy(position)));
}
#endif

#endif
