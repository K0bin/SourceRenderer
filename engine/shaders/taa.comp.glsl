#version 450

layout(set = 0, binding = 0, rgba8) uniform readonly image2D frame;
layout(set = 0, binding = 1, rgba8) uniform readonly image2D history;
layout(set = 0, binding = 2, rgba8) uniform writeonly image2D outputTexture;

void main() {
    ivec2 texCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
    imageStore(outputTexture, texCoord, imageLoad(frame, texCoord));
}
