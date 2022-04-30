#ifndef CAMERA_H
#define CAMERA_H

struct Camera {
  mat4 viewProj;
  mat4 invProj;
  mat4 view;
  mat4 proj;
  mat4 invView;
  vec4 position;
  mat4 invViewProj;
  float zNear;
  float zFar;
};

#endif
