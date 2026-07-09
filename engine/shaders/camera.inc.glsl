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
  float aspectRatio;
  float fov;
};

float linearizeDepth(Camera camera, float depth) {
    return camera.zNear * camera.zFar / (camera.zFar - depth * (camera.zFar - camera.zNear));
}

// Aspect ratio is width/height
// TODO: Do this on the CPU and add it to the camera buffer
float calculateVerticalFov(float fov, float aspectRatio) {
    return 2.0 * atan(tan((fov / 2.0)) / aspectRatio);
}

#endif
