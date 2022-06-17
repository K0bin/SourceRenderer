#extension GL_GOOGLE_include_directive : enable

#ifdef DEBUG
#extension GL_EXT_debug_printf : enable
#endif

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

#include "util.inc.glsl"
#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"
#include "camera.inc.glsl"

struct Frustum {
  float nearHalfWidth;
  float nearHalfHeight;
  uint _padding;
  uint _padding1;
  vec4 planes;
};

bool checkVisibilityAgainstFrustum(Frustum frustum, GPUBoundingBox aabb, Camera camera, mat4 modelTransform);
bool checkSphereVisibilityAgainstFrustum(Frustum frustum, GPUBoundingSphere sphere, Camera camera, mat4 modelTransform);
bool checkOcclusion(GPUBoundingBox aabb, Camera camera, mat4 modelTransform);

layout(std430, set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) writeonly restrict buffer visibleBuffer {
  uint visibleBitmasks[];
};
layout(std140, set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform frustumUBO {
  Frustum frustum;
};
#ifdef OCCLUSION_CULLING
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D hiZ;
#endif

#include "frame_set.inc.glsl"

shared uint[2] visible;

void main() {
  if (gl_LocalInvocationIndex == 0) {
    visible[0] = 0;
    visible[1] = 0;
  }
  barrier();
  uint drawableIndex = gl_GlobalInvocationID.x;
  bool isVisible = false;

  if (drawableIndex < scene.drawableCount) {
    GPUDrawable drawable = scene.drawables[drawableIndex];
    GPUMesh mesh = scene.meshes[drawable.meshIndex];
    GPUBoundingBox aabb = mesh.aabb;
    isVisible = true;

    #ifdef SPHERE_FRUSTUM_CULLING
    isVisible = isVisible && checkSphereVisibilityAgainstFrustum(frustum, mesh.sphere, oldCamera, drawable.transform);
    #ifdef DEBUG
    if (!isVisible) {
      debugPrintfEXT("Drawable failed spehere frustum check: %u", drawableIndex);
    }
    #endif
    #else
    isVisible = isVisible && checkVisibilityAgainstFrustum(frustum, mesh.sphere, oldCamera, drawable.transform);
    #ifdef DEBUG
    if (!isVisible) {
      debugPrintfEXT("Drawable failed AABB frustum check: %u", drawableIndex);
    }
    #endif
    #endif

    #ifdef OCCLUSION_CULLING
    bool wasVisible = isVisible;
    isVisible = isVisible && checkOcclusion(aabb, oldCamera, drawable.transform);
    #ifdef DEBUG
    if (wasVisible && !isVisible) {
      debugPrintfEXT("Drawable failed occlusion check: %u", drawableIndex);
    }
    #endif
    #endif
  }
  if (isVisible) {
    atomicOr(visible[gl_LocalInvocationIndex / 32], 1 << (drawableIndex % 32));
  }
  barrier();
  if (gl_LocalInvocationIndex == 0) {
    visibleBitmasks[gl_WorkGroupID.x * 2] = visible[0];
    visibleBitmasks[gl_WorkGroupID.x * 2 + 1] = visible[1];
  }
}

bool checkOcclusion(GPUBoundingBox aabb, Camera camera, mat4 modelTransform) {
  mat4 mvp = camera.viewProj * modelTransform;
  vec4 corners[8] = {
    mvp * vec4(aabb.bbmin.x, aabb.bbmin.y, aabb.bbmin.z, 1),
    mvp * vec4(aabb.bbmax.x, aabb.bbmin.y, aabb.bbmin.z, 1),
    mvp * vec4(aabb.bbmax.x, aabb.bbmax.y, aabb.bbmin.z, 1),
    mvp * vec4(aabb.bbmax.x, aabb.bbmax.y, aabb.bbmax.z, 1),
    mvp * vec4(aabb.bbmax.x, aabb.bbmin.y, aabb.bbmax.z, 1),
    mvp * vec4(aabb.bbmin.x, aabb.bbmax.y, aabb.bbmin.z, 1),
    mvp * vec4(aabb.bbmin.x, aabb.bbmax.y, aabb.bbmax.z, 1),
    mvp * vec4(aabb.bbmin.x, aabb.bbmin.y, aabb.bbmax.z, 1),
  };

  uint invalidCount[6] = { 0, 0, 0, 0, 0, 0 };
  vec3 minCorner = vec3(1);
  vec2 maxCorner = vec2(0);
  for (uint i = 0; i < 8; i++) {
    invalidCount[0] += (corners[i].x > corners[i].w) ? 1 : 0;
    invalidCount[1] += (corners[i].x < -corners[i].w) ? 1 : 0;
    invalidCount[2] += (corners[i].y > corners[i].w) ? 1 : 0;
    invalidCount[3] += (corners[i].y < -corners[i].w) ? 1 : 0;
    invalidCount[4] += (corners[i].z > corners[i].w) ? 1 : 0;
    invalidCount[5] += (corners[i].z < -corners[i].w) ? 1 : 0;

    corners[i].z = max(corners[i].z, 0);
    corners[i].xyz /= corners[i].w;
    minCorner = vec3(
      min(minCorner.x, corners[i].x),
      min(minCorner.y, corners[i].y),
      min(minCorner.z, corners[i].z)
    );
    maxCorner = vec2(
      max(maxCorner.x, corners[i].x),
      max(maxCorner.y, corners[i].y)
    );
  }

  for (uint i = 0; i < 6; i++) {
    if (invalidCount[i] == 8) {
      // Object is not in the frustum
      return false;
    }
  }

  minCorner.xy = clamp(minCorner.xy, vec2(-1), vec2(1));
  minCorner.z = clamp(minCorner.z, 0, 1);
  minCorner.xy = minCorner.xy * 0.5 + 0.5;
  minCorner.y = 1 - minCorner.y;

  maxCorner.xy = clamp(maxCorner.xy, vec2(-1), vec2(1));
  maxCorner.xy = maxCorner.xy * 0.5 + 0.5;
  maxCorner.y = 1 - maxCorner.y;

  vec2 mip0texSize = vec2(textureSize(hiZ, 0));
  vec2 dist = (maxCorner.xy - minCorner.xy) * mip0texSize;
  float maxDist = max(dist.x, dist.y);
  float mip = ceil(log2(maxDist));
  mip = max(mip - 1, 0);

  #ifndef MIN_MAX_SAMPLER
  vec4 depths = vec4(
    textureLod(hiZ, vec2(minCorner.x, minCorner.y), mip).x,
    textureLod(hiZ, vec2(maxCorner.x, minCorner.y), mip).x,
    textureLod(hiZ, vec2(maxCorner.x, maxCorner.y), mip).x,
    textureLod(hiZ, vec2(minCorner.x, maxCorner.y), mip).x
  );

  float maxDepth = max(max(depths.x, depths.y), max(depths.z, depths.w));
  #else
  // Sample the center between the 4 pixels and let the sampler handle it.
  float maxDepth = textureLod(hiZ, (minCorner.xy + maxCorner.xy) / 2, mip).x;
  #endif
  return minCorner.z <= maxDepth;
}


bool checkSphereVisibilityAgainstFrustum(Frustum frustum, GPUBoundingSphere sphere, Camera camera, mat4 modelTransform) {
  mat4 viewModel = camera.view * modelTransform;
  vec3 center = (viewModel * vec4(sphere.center, 1)).xyz;

  vec3 scale = vec3(length(modelTransform[0].xyz), length(modelTransform[1].xyz), length(modelTransform[2].xyz)); // suboptimal but no idea how to do this otherwise.
  float radius = sphere.radius * max(max(scale.x, scale.y), scale.z);

  bool isVisible = center.z + radius >= camera.zNear || center.z - radius <= camera.zFar;
  isVisible = isVisible && center.z * frustum.planes.y - abs(center.x) * frustum.planes.x > -radius;
  isVisible = isVisible && center.z * frustum.planes.w - abs(center.y) * frustum.planes.z > -radius;

  return isVisible;
}

struct OrientedBoundingBox {
  vec3 center;
  vec3 extents;
  vec3 axes[3];
};

bool checkVisibilityAgainstFrustum(Frustum frustum, GPUBoundingBox aabb, Camera camera, mat4 modelTransform) {
  // TODO check bounding sphere instead? that would be much cheaper.
  float zNear = -camera.zNear;
  float zFar = -camera.zFar;

  mat4 viewModel = camera.view * modelTransform;
  vec3 corners[4] = {
    (viewModel * vec4(aabb.bbmin.x, aabb.bbmin.y, aabb.bbmin.z, 1)).xyz,
    (viewModel * vec4(aabb.bbmax.x, aabb.bbmin.y, aabb.bbmin.z, 1)).xyz,
    (viewModel * vec4(aabb.bbmin.x, aabb.bbmax.y, aabb.bbmin.z, 1)).xyz,
    (viewModel * vec4(aabb.bbmin.x, aabb.bbmin.y, aabb.bbmax.z, 1)).xyz,
  };
  for (uint i = 0; i < 4; i++) {
    corners[i].z = -corners[i].z;
  }

  vec3 axes[3] = {
    corners[1] - corners[0],
    corners[2] - corners[0],
    corners[3] - corners[0],
  };

  vec3 center = corners[0] + 0.5 * (axes[0] + axes[1] + axes[2]);
  vec3 extents = vec3(length(axes[0]), length(axes[1]), length(axes[2]));
  vec3 normalizedAxes[3] = {
    axes[0] / extents.x,
    axes[1] / extents.y,
    axes[2] / extents.z
  };
  vec3 halfExtents = extents * 0.5;
  OrientedBoundingBox obb = OrientedBoundingBox(center, halfExtents, normalizedAxes);

  // Frustum near and far planes
  {
    float moC = obb.center.z;
    float radius = 0;
    for (uint i = 0; i < 3; i++) {
      radius += abs(obb.axes[i].z) * obb.extents[i];
    }
    float obbMin = moC - radius;
    float obbMax = moC + radius;
    float tau0 = zFar;
    float tau1 = zNear;

    if (obbMin > tau1 || obbMax < tau0) {
      return false;
    }
  }

  // remaining frustum planes
  {
    vec3 frustumPlanes[4] = {
      vec3(0, -zNear, frustum.nearHalfHeight),
      vec3(0, zNear, frustum.nearHalfHeight),
      vec3(-zNear, 0, frustum.nearHalfWidth),
      vec3(zNear, 0, frustum.nearHalfWidth),
    };
    for (uint i = 0; i < 4; i++) {
      vec3 m = frustumPlanes[i];
      float moX = abs(m.x);
      float moY = abs(m.y);
      float moZ = m.z;
      float moC = dot(m, obb.center);
      float obbRadius = 0;
      for (uint j = 0; j < 3; j++) {
        obbRadius += abs(dot(m, obb.axes[j])) * obb.extents[j];
      }
      float obbMin = moC - obbRadius;
      float obbMax = moC + obbRadius;
      float p = frustum.nearHalfWidth * moX + frustum.nearHalfHeight * moY;
      float tau0 = zNear * moZ - p;
      float tau1 = zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= zFar / zNear;
      }

      if (tau1 > 0) {
        tau1 *= zFar / zNear;
      }

      if (obbMin > tau1 || obbMax < tau0) {
        return false;
      }
    }
  }

  // OBB axes
  {
    for (uint i = 0; i < 3; i++) {
      vec3 m = obb.axes[i];
      float moX = abs(m.x);
      float moY = abs(m.y);
      float moZ = m.z;
      float moC = dot(m, obb.center);
      float obbRadius = obb.extents[i];
      float obbMin = moC - obbRadius;
      float obbMax = moC + obbRadius;
      float p = frustum.nearHalfWidth * moX + frustum.nearHalfHeight * moY;

      float tau0 = zNear * moZ - p;
      float tau1 = zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= zFar / zNear;
      }
      if (tau1 > 0) {
        tau1 *= zFar / zNear;
      }

      if (obbMin > tau1 || obbMax < tau0) {
        return false;
      }
    }
  }

  // cross products between the edges
  // R x A_i
  {
    for (uint i = 0; i < 3; i++) {
      vec3 m = vec3(0, -obb.axes[i].z, obb.axes[i].y);
      float moX = 0;
      float moY = abs(m.y);
      float moZ = m.z;
      float moC = m.y * obb.center.y + m.z * obb.center.z;
      float obbRadius = 0;
      for (uint j = 0; j < 3; j++) {
        obbRadius += abs(dot(m, obb.axes[j])) * obb.extents[j];
      }
      float obbMin = moC - obbRadius;
      float obbMax = moC + obbRadius;
      float p = frustum.nearHalfWidth * moX + frustum.nearHalfHeight * moY;

      float tau0 = zNear * moZ - p;
      float tau1 = zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= zFar / zNear;
      }
      if (tau1 > 0) {
        tau1 *= zFar / zNear;
      }

      if (obbMin > tau1 || obbMax < tau0) {
        return false;
      }
    }
  }

  // U x A_i
  {
    for (uint i = 0; i < 3; i++) {
      vec3 m = vec3(obb.axes[i].z, 0, -obb.axes[i].y);
      float moX = abs(m.x);
      float moY = 0;
      float moZ = m.z;
      float moC = m.x * obb.center.x + m.z * obb.center.z;
      float obbRadius = 0;
      for (uint j = 0; j < 3; j++) {
        obbRadius += abs(dot(m, obb.axes[j])) * obb.extents[j];
      }
      float obbMin = moC - obbRadius;
      float obbMax = moC + obbRadius;
      float p = frustum.nearHalfWidth * moX + frustum.nearHalfHeight * moY;

      float tau0 = zNear * moZ - p;
      float tau1 = zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= zFar / zNear;
      }
      if (tau1 > 0) {
        tau1 *= zFar / zNear;
      }

      if (obbMin > tau1 || obbMax < tau0) {
        return false;
      }
    }
  }

  // Frustum edge x A_i
  {
    for (uint i = 0; i < 3; i++) {
      vec3 axis = obb.axes[i];
      vec3 ms[4] = {
        cross(vec3(-frustum.nearHalfWidth, 0, zNear), axis),
        cross(vec3(frustum.nearHalfWidth, 0, zNear), axis),
        cross(vec3(0, frustum.nearHalfHeight, zNear), axis),
        cross(vec3(0, -frustum.nearHalfHeight, zNear), axis),
      };
      for (uint j = 0; j < 4; j++) {
        vec3 m = ms[j];
        float moX = abs(m.x);
        float moY = abs(m.y);
        float moZ = m.z;
        const float EPSILON = 0.0001;
        if (moX < EPSILON && moY < EPSILON && abs(moZ) < EPSILON) {
          continue;
        }
        float moC = dot(m, obb.center);
        float obbRadius = 0;
        for (uint k = 0; k < 3; k++) {
          obbRadius += abs(dot(m, obb.axes[k])) * obb.extents[k];
        }
        float obbMin = moC - obbRadius;
        float obbMax = moC + obbRadius;
        float p = frustum.nearHalfWidth * moX + frustum.nearHalfHeight * moY;

        float tau0 = zNear * moZ - p;
        float tau1 = zNear * moZ + p;

        if (tau0 < 0) {
          tau0 *= zFar / zNear;
        }
        if (tau1 > 0) {
          tau1 *= zFar / zNear;
        }

        if (obbMin > tau1 || obbMax < tau0) {
          return false;
        }
      }
    }
  }

  return true;
}

// References:
// https://arm-software.github.io/opengl-es-sdk-for-android/occlusion_culling.html
// https://www.rastergrid.com/blog/2010/10/hierarchical-z-map-based-occlusion-culling/
// https://interplayoflight.wordpress.com/2017/11/15/experiments-in-gpu-based-occlusion-culling/
// https://github.com/zeux/niagara/blob/master/src/niagara.cpp