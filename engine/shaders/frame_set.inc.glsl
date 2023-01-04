#ifndef FRAME_SET_INC_GLSL
#define FRAME_SET_INC_GLSL

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"
#include "vertex.inc.glsl"
#include "gpu_scene.inc.glsl"

layout(set = DESCRIPTOR_SET_FRAME, binding = 0, std430) readonly restrict buffer sceneBuffer {
  GPUScene scene;
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 1, std430) readonly restrict buffer sceneDrawsBuffer {
  GPUDraw scene_draws[];
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 2, std430) readonly restrict buffer sceneMeshesBuffer {
  GPUMesh scene_meshes[];
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 3, std430) readonly restrict buffer sceneDrawablesBuffer {
  GPUDrawable scene_drawables[];
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 4, std430) readonly restrict buffer scenePartsBuffer {
  GPUMeshPart scene_parts[];
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 5, std430) readonly restrict buffer sceneMaterialsBuffer {
  GPUMaterial scene_materials[];
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 6, std430) readonly restrict buffer sceneLightsBuffer {
  GPULight scene_lights[];
};

layout(set = DESCRIPTOR_SET_FRAME, binding = 7, std140) uniform CameraUBO {
  Camera camera;
};
layout(set = DESCRIPTOR_SET_FRAME, binding = 8, std140) uniform OldCameraUBO {
  Camera oldCamera;
};
layout(set = DESCRIPTOR_SET_FRAME, binding = 9, std430) readonly restrict buffer verticesSSBO {
  Vertex vertices[];
};
layout(set = DESCRIPTOR_SET_FRAME, binding = 10, std430) readonly restrict buffer indicesSSBO {
  uint indices[];
};
layout(set = DESCRIPTOR_SET_FRAME, binding = 11, std140) uniform SetupUBO {
  uint pointLightCount;
  uint directionalLightCount;
  float clusterZBias;
  float clusterZScale;
  uvec3 clusterCount;
  mat4 swapchainTransform;
  vec2 jitterPoint;
  uvec2 rtSize;
};
struct PointLight {
  vec4 positionAndIntensity;
};
layout(set = DESCRIPTOR_SET_FRAME, binding = 12, std140) uniform PointLightUBO {
  PointLight pointLights[1024];
};
struct DirectionalLight {
  vec4 directionAndIntensity;
};
layout(set = DESCRIPTOR_SET_FRAME, binding = 13, std140) uniform DirectionalLightUBO {
  DirectionalLight directionalLights[1024];
};

#endif
