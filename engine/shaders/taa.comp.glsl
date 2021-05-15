#version 450

layout(set = 0, binding = 0) uniform sampler2D frame;
layout(set = 0, binding = 1) uniform sampler2D history;
layout(set = 0, binding = 2, rgba8) uniform writeonly image2D outputTexture;
layout(set = 0, binding = 3) uniform sampler2D motion;

const int HISTORY_FRAMES = 8;

void main() {
    ivec2 textureSize = textureSize(frame, 0);
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(textureSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(textureSize.y));
    ivec2 storageTexCoord = ivec2(int(gl_GlobalInvocationID.x), int(gl_GlobalInvocationID.y));
    vec4 color = texture(frame, texCoord);

    vec2 historyTexCoord = texCoord - texture(motion, texCoord).xy;
    vec4 historyColor = texture(history, historyTexCoord);
    bool useHistory = historyTexCoord.x >= 0.0 && historyTexCoord.x <= 1.0 && historyTexCoord.y >= 0.0 && historyTexCoord.y <= 1.0;
    float taaFactor = useHistory ? (1.0 - 1.0 / float(HISTORY_FRAMES)) : 0.0;
    imageStore(outputTexture, storageTexCoord, color * (1.0 - taaFactor) + historyColor * taaFactor);
}
