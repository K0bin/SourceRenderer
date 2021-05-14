#version 450

layout(set = 0, binding = 0) uniform sampler2D frame;
layout(set = 0, binding = 1) uniform sampler2D history;
layout(set = 0, binding = 2, rgba8) uniform writeonly image2D outputTexture;
layout(set = 0, binding = 3) uniform sampler2D motion;

void main() {
    ivec2 textureSize = textureSize(frame, 0);
    vec2 texCoord = vec2(float(gl_GlobalInvocationID.x) / float(textureSize.x), float(gl_GlobalInvocationID.y) / float(textureSize.y));
    ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
    vec4 color = texture(frame, texCoord);
    vec4 historyColor = texture(history, texCoord - texture(motion, texCoord).xy);
    imageStore(outputTexture, storageTexCoord, (color + historyColor) * 0.5);
}
