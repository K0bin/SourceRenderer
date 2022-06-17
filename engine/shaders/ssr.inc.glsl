#include "util.inc.glsl"

struct SSRConfig {
  float maxDistance;
  float resolution;
  uint steps;
  float thickness;
};

float reflectScreenspace(sampler2D depthTex, vec2 texCoord, Camera camera, SSRConfig config, out vec2 outReflectionTexCoords) {
  vec2 texSize = textureSize(depthTex, 0);

  float startDepth = textureLod(depthTex, texCoord, 0).x;
  vec3 positionFrom = viewSpacePosition(texCoord, startDepth, camera.invProj);
  vec3 unitPositionFrom = normalize(positionFrom);
  #ifdef CS
  vec3 normal = reconstructViewSpaceNormalCS(depthTex, texCoord, camera.invProj);
  #else
  vec3 normal = reconstructViewSpaceNormalFS(texCoord, startDepth, camera.invProj);
  #endif
  vec3 pivot = normalize(reflect(unitPositionFrom, normal));

  vec4 startView = vec4(positionFrom.xyz, 1.0);
  vec4 endView = vec4(positionFrom.xyz + (pivot * config.maxDistance), 1.0);

  vec4 startFrag = camera.proj * startView;
  startFrag.xyz /= startFrag.w;
  startFrag.xy = startFrag.xy * 0.5 + 0.5;
  startFrag.y = 1.0 - startFrag.y;
  startFrag.xy *= texSize;

  vec4 endFrag = camera.proj * endView;
  endFrag.xyz /= endFrag.w;
  endFrag.xy = endFrag.xy * 0.5 + 0.5;
  endFrag.xy = clamp(endFrag.xy, vec2(0), vec2(1));
  endFrag.y = 1.0 - endFrag.y;
  endFrag.xy *= texSize;

  vec2 delta = endFrag.xy - startFrag.xy;
  bool useX      = abs(delta.x) >= abs(delta.y);
  float deltaVal = max(abs(delta.x), abs(delta.y)) * clamp(config.resolution, 0.0, 1.0);
  vec2 increment = delta / max(deltaVal, 0.001);

  vec2 frag = startFrag.xy;
  vec2 uv;
  float depth;
  float lastMissFrac = 0.0;
  float frac = 0.0;
  float sampleDepth;
  float sampleZDiff;
  bool firstPassFoundHit = false;
  bool secondPassFoundHit = false;

  for (uint i = 0; i < uint(deltaVal); i++) {
    frag += increment;
    uv.xy = frag / texSize;
    sampleDepth = textureLod(depthTex, uv, 0.0).x;
    float sampleZ = linearizeDepth(sampleDepth, camera.zNear, camera.zFar);

    frac = useX
      ? ((frag.x - startFrag.x) / delta.x)
      : ((frag.y - startFrag.y) / delta.y);
    frac = clamp(frac, 0.0, 1.0);

    float rayZ = (startView.z * endView.z) / mix(endView.z, startView.z, frac);
    sampleZDiff = rayZ - sampleZ;

    if (sampleZDiff > 0 && sampleZDiff < config.thickness) {
      firstPassFoundHit = true;
      break;
    } else {
      lastMissFrac = frac;
    }
  }

  if (!firstPassFoundHit) {
    outReflectionTexCoords = vec2(0);
    return 0.0;
  }
  float lastHitFrac = frac;
  for (uint i = 0; i < config.steps; i++) {
    frac = lastMissFrac + ((lastHitFrac - lastMissFrac) * 0.5);
    frag = mix(startFrag.xy, endFrag.xy, frac);
    uv = frag / texSize;
    sampleDepth = textureLod(depthTex, uv, 0.0).x;
    float sampleZ = linearizeDepth(sampleDepth, camera.zNear, camera.zFar);

    float rayZ = (startView.z * endView.z) / mix(endView.z, startView.z, frac);
    sampleZDiff = rayZ - sampleZ;

    if (sampleZDiff > 0 && sampleZDiff < config.thickness) {
      secondPassFoundHit = true;
      lastHitFrac = frac;
      outReflectionTexCoords = uv;
    } else {
      lastMissFrac = frac;
    }
  }

  if ((!secondPassFoundHit && config.steps != 0) || (uv.x < 0 || uv.x > 1 || uv.y < 0 || uv.y > 1)) {
    outReflectionTexCoords = vec2(0);
    return 0.0;
  }

  vec3 positionTo = viewSpacePosition(uv, sampleDepth, camera.invProj);
  outReflectionTexCoords = uv;
  return (1 - max(dot(-unitPositionFrom, pivot), 0))
    * (1 - clamp(sampleZDiff / config.thickness, 0, 1))
    * (1 - clamp(length(positionTo - positionFrom) / config.maxDistance, 0, 1));
}

// References:
// https://lettier.github.io/3d-game-shaders-for-beginners/screen-space-reflection.html
// https://sugulee.wordpress.com/2021/01/16/performance-optimizations-for-screen-space-reflections-technique-part-1-linear-tracing-method/
// https://sugulee.wordpress.com/2021/01/19/screen-space-reflections-implementation-and-optimization-part-2-hi-z-tracing-method/
