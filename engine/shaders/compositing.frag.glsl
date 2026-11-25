#version 450
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "frame_set_volume.inc.glsl"
#include "util.inc.glsl"

layout(location = 0) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform texture2D color;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform texture2D ssaoTex;

float blurSsao(vec2 uv) {
    uvec2 ssaoTextureSize = textureSize(sampler2D(ssaoTex, samplerNearest), 0);

    // TODO: Use linear sampling more explicitly to reduce samples
    vec2 texel = vec2(1.0 / float(ssaoTextureSize.x), 1.0 / float(ssaoTextureSize.y));
    float sum = 0.0;
    const int kernelSize = 4;
    for (int x = 0; x < kernelSize; x++) {
        for (int y = 0; y < kernelSize; y++) {
            vec2 offset = vec2(float(x - kernelSize / 2), float(y - kernelSize / 2));
            vec4 ssaoTexel = texture(sampler2D(ssaoTex, samplerLinear), uv + offset * texel);
            sum += ssaoTexel.a;
        }
    }
    sum /= kernelSize * kernelSize;

    return sum;
}

void main(void) {
    float ssao = blurSsao(in_uv);
    vec4 withSSAO = textureLod(sampler2D(color, samplerLinear), in_uv, 0) * ssao;

    // We need to do color conversion manually here because DearImgui expects rendering to a linear image
    // and it renders to the swapchain image.
    // Easier to do the color conversion here than to do another copy of the DearImgui contents or convert those
    out_color = vec4(linearToSRGB(withSSAO.rgb), 1.0);
}
