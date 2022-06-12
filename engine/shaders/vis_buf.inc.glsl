#ifndef VIS_BUF_H
#define VIS_BUF_H

#include "gpu_scene.inc.glsl"

uint getDrawIndex(uint id) {
  return id >> 16;
}

Vertex getVertex(in GPUScene scene, uint id, vec2 barycentrics) {
  uint drawId = getDrawIndex(id);
  uint primitiveId = id & 0xff;

  GPUDraw draw = scene.draws[drawId];
  GPUMeshPart part = scene.parts[draw.partIndex];
  GPUDrawable drawable = scene.drawables[draw.drawableIndex];

  uint index0 = indices[part.meshFirstIndex + primitiveId * 3];
  uint index1 = indices[part.meshFirstIndex + primitiveId * 3 + 1];
  uint index2 = indices[part.meshFirstIndex + primitiveId * 3 + 2];

  Vertex vert0 = vertices[index0];
  Vertex vert1 = vertices[index1];
  Vertex vert2 = vertices[index2];

  vert0.position = (drawable.transform * vec4(vert0.position, 1)).xyz;
  vert1.position = (drawable.transform * vec4(vert1.position, 1)).xyz;
  vert2.position = (drawable.transform * vec4(vert2.position, 1)).xyz;

  mat4 transposedTransform = transpose(drawable.transform);
  vert0.normal = (normalize(transposedTransform * vec4(vert0.normal, 1))).xyz;
  vert1.normal = (normalize(transposedTransform * vec4(vert1.normal, 1))).xyz;
  vert2.normal = (normalize(transposedTransform * vec4(vert2.normal, 1))).xyz;

  vec3 bary = vec3(barycentrics.x, barycentrics.y, 1 - barycentrics.x - barycentrics.y);
  Vertex interpolated;
  interpolated.position = vert0.position * bary.x + vert1.position * bary.y + vert2.position * bary.z;
  interpolated.normal = vert0.normal * bary.x + vert1.normal * bary.y + vert2.normal * bary.z;
  interpolated.uv = vert0.uv * bary.x + vert1.uv * bary.y + vert2.uv * bary.z;
  interpolated.lightmapUv = vert0.lightmapUv * bary.x + vert1.lightmapUv * bary.y + vert2.lightmapUv * bary.z;
  interpolated.alpha = vert0.alpha * bary.x + vert1.alpha * bary.y + vert2.alpha * bary.z;

  return interpolated;
}

GPUMaterial getMaterial(in GPUScene scene, uint id) {
  uint drawIndex = getDrawIndex(id);
  GPUDraw draw = scene.draws[drawIndex];
  GPUMeshPart part = scene.parts[draw.partIndex];
  GPUMaterial material = scene.materials[part.materialIndex];
  return material;
}

#endif
