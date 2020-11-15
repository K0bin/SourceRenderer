#version 450

layout(set = 0, binding = 0, std140) uniform Cameras {
    mat4 mats[16];
    uint counter;
} cameras;

layout(set = 0, binding = 1, std140) buffer Camera {
    mat4 mat;
} camera;

void main() {
    camera.mat = cameras.mats[cameras.counter % 16];
}
