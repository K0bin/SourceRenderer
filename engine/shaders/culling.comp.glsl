#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

#include "util.inc.glsl"
#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"
#include "camera.inc.glsl"

struct Frustum {
  float nearHalfWidth;
  float nearHalfHeight;
  float zNear;
  float zFar;
};

bool checkVisibilityAgainstFrustum(Frustum frustum, GPUBoundingBox aabb, Camera camera, mat4 modelTransform);
bool checkOcclusion(GPUBoundingBox aabb, mat4 modelTransform);

layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 0) readonly restrict buffer sceneBuffer {
  GPUScene scene;
};
layout(std430, set = DESCRIPTOR_SET_PER_DRAW, binding = 1) writeonly restrict buffer visibleBuffer {
  uint visibleBitmasks[];
};
layout(std140, set = DESCRIPTOR_SET_PER_DRAW, binding = 2) uniform cameraUBO {
  Camera camera;
};
layout(std140, set = DESCRIPTOR_SET_PER_DRAW, binding = 3) uniform frustumUBO {
  Frustum frustum;
};
layout(set = DESCRIPTOR_SET_PER_DRAW, binding = 4) uniform sampler2D hiZ;

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

    isVisible = checkVisibilityAgainstFrustum(frustum, aabb, camera, drawable.transform);
    bool passedFrustumCheck = isVisible;
    isVisible = isVisible && checkOcclusion(aabb, drawable.transform);
    /* if (!isVisible && passedFrustumCheck) {
      debugPrintfEXT("Drawable passed frustum check but failed occlusion culling: %u", drawableIndex);
    } */
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

bool checkOcclusion(GPUBoundingBox aabb, mat4 modelTransform) {
  mat4 projViewModel = camera.viewProj * modelTransform;
  vec4 corners[8] = {
    (projViewModel * vec4(aabb.bbmin.x, aabb.bbmin.y, aabb.bbmin.z, 1)),
    (projViewModel * vec4(aabb.bbmax.x, aabb.bbmin.y, aabb.bbmin.z, 1)),
    (projViewModel * vec4(aabb.bbmax.x, aabb.bbmax.y, aabb.bbmin.z, 1)),
    (projViewModel * vec4(aabb.bbmax.x, aabb.bbmax.y, aabb.bbmax.z, 1)),
    (projViewModel * vec4(aabb.bbmax.x, aabb.bbmin.y, aabb.bbmax.z, 1)),
    (projViewModel * vec4(aabb.bbmin.x, aabb.bbmax.y, aabb.bbmin.z, 1)),
    (projViewModel * vec4(aabb.bbmin.x, aabb.bbmax.y, aabb.bbmax.z, 1)),
    (projViewModel * vec4(aabb.bbmin.x, aabb.bbmin.y, aabb.bbmax.z, 1)),
  };

  corners[0].xyz /= corners[0].w;
  vec2 minCorner = corners[0].xy;
  vec3 maxCorner = corners[0].xyz;
  for (uint i = 1; i < 8; i++) {
    corners[i].xyz /= corners[i].w;
    minCorner = vec2(
      min(minCorner.x, corners[i].x),
      min(minCorner.y, corners[i].y)
    );
    maxCorner = vec3(
      max(maxCorner.x, corners[i].x),
      max(maxCorner.y, corners[i].y),
      max(maxCorner.z, corners[i].z)
    );
  }

  if (maxCorner.z >= 1)
    return true;

  vec2 mip0texSize = vec2(textureSize(hiZ, 0));
  vec2 dist = (maxCorner.xy - minCorner.xy) * mip0texSize;
  float maxDist = max(dist.x, dist.y);
  float mip = ceil(log2(maxDist / 2));

  vec2 texSize = vec2(textureSize(hiZ, int(mip)));
  vec2 mipTexelSize = vec2(1 / texSize.x, 1 / texSize.y);
  vec2 center = round(minCorner.xy + (maxCorner.xy - minCorner.xy) * 0.5) / texSize;
  vec4 depths[4] = {
    textureLod(hiZ, center + vec2(-0.5, -0.5) * mipTexelSize, mip),
    textureLod(hiZ, center + vec2(-0.5, 0.5) * mipTexelSize, mip),
    textureLod(hiZ, center + vec2(0.5, 0.5) * mipTexelSize, mip),
    textureLod(hiZ, center + vec2(0.5, -0.5) * mipTexelSize, mip)
  };

  float maxDepth = max(max(depths[0].x, depths[1].x), max(depths[2].x, depths[3].x));
  return maxCorner.z <= maxDepth;
}

struct OrientedBoundingBox {
  vec3 center;
  vec3 extents;
  vec3 axes[3];
};

bool checkVisibilityAgainstFrustum(Frustum frustum, GPUBoundingBox aabb, Camera camera, mat4 modelTransform) {
  // TODO check bounding sphere instead? that would be much cheaper.

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
    float tau0 = frustum.zFar;
    float tau1 = frustum.zNear;

    if (obbMin > tau1 || obbMax < tau0) {
      return false;
    }
  }

  // remaining frustum planes
  {
    vec3 frustumPlanes[4] = {
      vec3(0, -frustum.zNear, frustum.nearHalfHeight),
      vec3(0, frustum.zNear, frustum.nearHalfHeight),
      vec3(-frustum.zNear, 0, frustum.nearHalfWidth),
      vec3(frustum.zNear, 0, frustum.nearHalfWidth),
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
      float tau0 = frustum.zNear * moZ - p;
      float tau1 = frustum.zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= frustum.zFar / frustum.zNear;
      }

      if (tau1 > 0) {
        tau1 *= frustum.zFar / frustum.zNear;
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

      float tau0 = frustum.zNear * moZ - p;
      float tau1 = frustum.zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= frustum.zFar / frustum.zNear;
      }
      if (tau1 > 0) {
        tau1 *= frustum.zFar / frustum.zNear;
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

      float tau0 = frustum.zNear * moZ - p;
      float tau1 = frustum.zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= frustum.zFar / frustum.zNear;
      }
      if (tau1 > 0) {
        tau1 *= frustum.zFar / frustum.zNear;
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

      float tau0 = frustum.zNear * moZ - p;
      float tau1 = frustum.zNear * moZ + p;

      if (tau0 < 0) {
        tau0 *= frustum.zFar / frustum.zNear;
      }
      if (tau1 > 0) {
        tau1 *= frustum.zFar / frustum.zNear;
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
        cross(vec3(-frustum.nearHalfWidth, 0, frustum.zNear), axis),
        cross(vec3(frustum.nearHalfWidth, 0, frustum.zNear), axis),
        cross(vec3(0, frustum.nearHalfHeight, frustum.zNear), axis),
        cross(vec3(0, -frustum.nearHalfHeight, frustum.zNear), axis),
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

        float tau0 = frustum.zNear * moZ - p;
        float tau1 = frustum.zNear * moZ + p;

        if (tau0 < 0) {
          tau0 *= frustum.zFar / frustum.zNear;
        }
        if (tau1 > 0) {
          tau1 *= frustum.zFar / frustum.zNear;
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