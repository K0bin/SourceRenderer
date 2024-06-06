#ifndef PBR_H
#define PBR_H

#include "consts.inc.glsl"

vec3 pbr(vec3 lightDir, vec3 viewDir, vec3 normal, vec3 f0, vec3 albedo, vec3 radiance, float roughness, float metalness);
float distributionGGX(vec3 normal, vec3 halfway, float roughness);
float schlickGGX(float nDotV, float roughness, bool direct);
float geometrySmith(vec3 normal, vec3 viewDir, vec3 lightDir, float k, bool direct);
vec3 fresnelSchlick(float cosTheta, vec3 f0);

vec3 pbr(vec3 lightDir, vec3 viewDir, vec3 normal, vec3 f0, vec3 albedo, vec3 radiance, float roughness, float metalness) {
  vec3 halfway = normalize(viewDir + lightDir);

  float ndf = distributionGGX(normal, halfway, roughness);
  float g = geometrySmith(normal, viewDir, lightDir, roughness, false);
  vec3 f = fresnelSchlick(max(dot(halfway, viewDir), 0.0), f0);

  vec3 kS = f;
  vec3 kD = vec3(1.0) - kS;
  kD *= 1.0 - metalness;

  vec3 specular = (ndf * g * f) / (4.0 * max(dot(normal, viewDir), 0.0) * max(dot(normal, lightDir), 0.0) + 0.0001);
  float nDotL = max(dot(normal, lightDir), 0.0);
  return (kD * albedo / PI + specular) * radiance * nDotL;
}

float distributionGGX(vec3 normal, vec3 halfway, float roughness) {
  float a = roughness * roughness;
  float a2 = a * a;
  float nDotH = max(dot(normal, halfway), 0.0);
  float nDotH2 = nDotH * nDotH;
  float x = nDotH2 * (a2 - 1.0) + 1.0;
  return a2 / (PI * x * x);
}

float geometrySchlickGGX(float nDotV, float roughness, bool direct) {
  float r = roughness + 1.0;
  float k;
  if (direct) {
    k = (r*r) / 8.0;
  } else {
    k = (roughness * roughness) / 2;
  }
  return nDotV / (nDotV * (1.0 - k) + k);
}

float geometrySmith(vec3 normal, vec3 viewDir, vec3 lightDir, float roughness, bool direct) {
  float nDotV = max(dot(normal, viewDir), 0.0);
  float nDotL = max(dot(normal, lightDir), 0.0);
  float ggx2 = geometrySchlickGGX(nDotV, roughness, direct);
  float ggx1 = geometrySchlickGGX(nDotL, roughness, direct);
  return ggx1 * ggx2;
}

vec3 fresnelSchlick(float cosTheta, vec3 f0) {
  return f0 + (1.0 - f0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

#endif
