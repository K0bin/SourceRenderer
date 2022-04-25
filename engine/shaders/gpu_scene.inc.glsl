struct GPUDraw {
  uint drawableIndex;
  uint partIndex;
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
  uint _padding;
  uint _padding1;
  uint _padding2; // 16 byte alignment because of the mat4 members
};

struct GPUBoundingBox {
  vec4 bbmin;
  vec4 bbmax; // vec3 has vec4 alignment, so just use vec4
};

struct GPUMesh {
  GPUBoundingBox aabb;
};

#define DRAWABLE_CAPACITY 4096
#define PART_CAPACITY 4096
#define DRAW_CAPACITY 4096
#define MATERIAL_CAPACITY 4096
#define MESH_CAPACITY 4096

struct GPUScene {
  uint partCount;
  uint materialCount;
  uint drawableCount;
  uint meshCount;
  uint drawCount;
  uint _padding;
  uint _padding1;
  uint _padding2;
  GPUDraw draws[DRAW_CAPACITY]; // needs 4 byte alignment
  GPUMeshPart parts[PART_CAPACITY]; // needs 4 byte alignment
  GPUMaterial materials[MATERIAL_CAPACITY]; // needs 16 byte alignment
  GPUDrawable drawables[DRAWABLE_CAPACITY]; // needs 16 byte alignment
  GPUMesh meshes[MESH_CAPACITY]; // needs 16 byte alignment
};
