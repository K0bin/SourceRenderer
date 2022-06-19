#ifndef VIS_BUF_H
#define VIS_BUF_H

#include "gpu_scene.inc.glsl"
#include "camera.inc.glsl"
#include "vertex.inc.glsl"

#ifndef VERTICES_ARRAY_NAME
#define VERTICES_ARRAY_NAME vertices
#endif

#ifndef INDICES_ARRAY_NAME
#define INDICES_ARRAY_NAME indices
#endif

#ifndef GPU_SCENE_NAME
#define GPU_SCENE_NAME scene
#endif

uint getDrawIndex(uint id) {
  return id >> 16;
}

uint getPrimitiveIndex(uint id) {
  return id & 0xffff;
}

Vertex interpolateVertex(vec2 barycentrics, Vertex vertices[3]) {
  vec3 bary = vec3(barycentrics.x, barycentrics.y, 1 - barycentrics.x - barycentrics.y);
  Vertex interpolated;
  interpolated.position = vertices[0].position * bary.x + vertices[1].position * bary.y + vertices[2].position * bary.z;
  interpolated.normal = vertices[0].normal * bary.x + vertices[1].normal * bary.y + vertices[2].normal * bary.z;
  interpolated.uv = vertices[0].uv * bary.x + vertices[1].uv * bary.y + vertices[2].uv * bary.z;
  interpolated.lightmapUv = vertices[0].lightmapUv * bary.x + vertices[1].lightmapUv * bary.y + vertices[2].lightmapUv * bary.z;
  interpolated.alpha = vertices[0].alpha * bary.x + vertices[1].alpha * bary.y + vertices[2].alpha * bary.z;
  return interpolated;
}

Vertex[3] getVertices(uint id) {
  uint drawId = getDrawIndex(id);
  uint primitiveId = getPrimitiveIndex(id);

  GPUDraw draw = GPU_SCENE_NAME.draws[drawId];
  GPUMeshPart part = GPU_SCENE_NAME.parts[draw.partIndex];

  uint firstIndex = part.meshFirstIndex + primitiveId * 3;
  uint index0 = INDICES_ARRAY_NAME[firstIndex];
  uint index1 = INDICES_ARRAY_NAME[firstIndex + 1];
  uint index2 = INDICES_ARRAY_NAME[firstIndex + 2];

  Vertex readVertices[3];
  readVertices[0] = VERTICES_ARRAY_NAME[part.meshVertexOffset + index0];
  readVertices[1] = VERTICES_ARRAY_NAME[part.meshVertexOffset + index1];
  readVertices[2] = VERTICES_ARRAY_NAME[part.meshVertexOffset + index2];
  return readVertices;
}

Vertex getVertex(uint id, vec2 barycentrics) {
  Vertex vertices[3] = getVertices(id);

  uint drawId = getDrawIndex(id);
  uint primitiveId = getPrimitiveIndex(id);

  GPUDraw draw = GPU_SCENE_NAME.draws[drawId];
  GPUDrawable drawable = GPU_SCENE_NAME.drawables[draw.drawableIndex];

  mat4 transposedTransform = transpose(inverse(drawable.transform));
  Vertex vertex = interpolateVertex(barycentrics, vertices);
  vertex.position = (drawable.transform * vec4(vertex.position, 1)).xyz;
  vertex.normal = normalize((transposedTransform * vec4(vertex.normal, 0)).xyz);
  return vertex;
}

GPUMaterial getMaterial(uint id) {
  uint drawIndex = getDrawIndex(id);
  GPUDraw draw = scene.draws[drawIndex];
  GPUMeshPart part = scene.parts[draw.partIndex];
  GPUMaterial material = scene.materials[part.materialIndex];
  return material;
}

vec2 getMotionVector(uint id, vec2 barycentrics, Camera camera, Camera oldCamera) {
  Vertex vertices[3] = getVertices(id);
  Vertex oldVertices[3] = vertices;

  uint drawId = getDrawIndex(id);
  uint primitiveId = getPrimitiveIndex(id);

  GPUDraw draw = GPU_SCENE_NAME.draws[drawId];
  GPUDrawable drawable = GPU_SCENE_NAME.drawables[draw.drawableIndex];

  Vertex interpolatedVertex = interpolateVertex(barycentrics, vertices);
  Vertex interpolatedOldVertex = interpolateVertex(barycentrics, oldVertices);

  vec4 projectedPosition = camera.viewProj * drawable.transform * vec4(interpolatedVertex.position, 1);
  vec2 ndcPosition = (projectedPosition.xy / projectedPosition.w) * 0.5 + 0.5;
  vec4 projectedOldPosition = oldCamera.viewProj * drawable.oldTransform * vec4(interpolatedOldVertex.position, 1);
  vec2 oldNdcPosition = (projectedOldPosition.xy / projectedOldPosition.w) * 0.5 + 0.5;
  vec2 motion = ndcPosition - oldNdcPosition;
  motion.y = -motion.y;
  return motion;
}

#endif
