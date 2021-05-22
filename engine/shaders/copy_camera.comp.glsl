#version 450

layout(set = 0, binding = 0, std140) readonly buffer Cameras {
  mat4 proj[16];
  mat4 view[16];
  uint proj_index;
  uint view_index;
} cameras;

layout(set = 0, binding = 1, std140) buffer Camera {
  mat4 viewProj;
  mat4 invProj;
} camera;

void main() {
  mat4 proj = cameras.proj[cameras.proj_index];
  mat4 view = cameras.view[cameras.view_index];
  camera.viewProj = proj * view;
  camera.invProj = inverse(proj);
}
