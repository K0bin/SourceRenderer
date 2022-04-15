struct GPUDrawableRange {
  uint materialIndex;
  uint drawableIndex;
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
  uint aabbIndex;
  uint partStart;
  uint partCount;
  uint _padding; // 16 byte alignment because of the mat4 members
};

struct GPUBoundingBox {
  vec4 bbmin;
  vec4 bbmax; // vec3 has vec4 alignment, so just use vec4
};

struct GPUScene {
  uint partCount;
  uint materialCount;
  uint drawableCount;
  uint aabbCount;
  GPUDrawableRange parts[16384 * 32]; // needs 4 byte alignment
  GPUMaterial materials[4096]; // needs 16 byte alignment
  GPUDrawable drawables[16384]; // needs 16 byte alignment
  GPUBoundingBox aabbs[4096]; // needs 16 byte alignment
};
