#ifndef GPU_SCENE_H
#define GPU_SCENE_H

#extension GL_EXT_shader_16bit_storage : require
#extension GL_EXT_shader_explicit_arithmetic_types : require

struct GPUDraw {
  uint16_t drawableIndex;
  uint16_t partIndex;
};

struct GPUMeshPart {
  uint materialIndex;
  uint meshFirstIndex;
  uint meshIndexCount;
  uint meshVertexOffset;
};

struct GPUMaterial {
  vec4 albedoColor;
  float roughnessFactor;
  float metalnessFactor;
  uint albedoTextureIndex;
  uint _padding; // 16 byte alignment because of the vec4 member
};

struct GPUDrawable {
  mat4 transform;
  mat4 oldTransform;
  uint meshIndex;
  uint flags;
  uint partStart;
  uint partCount; // 16 byte alignment because of the mat4 members
};

struct GPUBoundingBox {
  vec4 bbmin;
  vec4 bbmax; // vec3 has vec4 alignment, so just use vec4
};

struct GPUBoundingSphere {
  vec3 center;
  float radius;
};

struct GPUMesh {
  GPUBoundingBox aabb;
  GPUBoundingSphere sphere;
};

struct GPULight {
  vec3 pos;
  uint32_t lightType;
  vec3 dir;
  float intensity;
  vec3 color;
  uint _padding;
};

#define DRAWABLE_CAPACITY 4096
#define PART_CAPACITY 4096
#define DRAW_CAPACITY 4096
#define MATERIAL_CAPACITY 4096
#define MESH_CAPACITY 4096
#define LIGHT_CAPACITY 64

// TODO: Move arrays to separate buffers

struct GPUScene {
  uint drawableCount;
  uint drawCount;
  uint lightCount;
};

#endif
