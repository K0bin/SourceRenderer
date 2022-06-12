#ifndef VERTEX_H
#define VERTEX_H

struct Vertex {
  vec3 position;
  vec3 normal;
  vec2 uv;
  vec2 lightmapUv;
  float alpha;
};

#endif
