#version 450

layout(set = 0, binding = 0, std140) readonly buffer Cameras {
    mat4 mats[256];
    uint counter;
} cameras;

layout(set = 0, binding = 1, std140) buffer Camera {
    mat4 mat;
} camera;

void main() {
    camera.mat = cameras.mats[cameras.counter % 16];
}
