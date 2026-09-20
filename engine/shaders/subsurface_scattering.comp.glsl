/* Adapted from https://github.com/iryoku/separable-sss/blob/master/SeparableSSS.h */

/**
 * Copyright (C) 2012 Jorge Jimenez (jorge@iryoku.com)
 * Copyright (C) 2012 Diego Gutierrez (diegog@unizar.es)
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 *    1. Redistributions of source code must retain the above copyright notice,
 *       this list of conditions and the following disclaimer.
 *
 *    2. Redistributions in binary form must reproduce the following disclaimer
 *       in the documentation and/or other materials provided with the
 *       distribution:
 *
 *       "Uses Separable SSS. Copyright (C) 2012 by Jorge Jimenez and Diego
 *        Gutierrez."
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS ``AS
 * IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
 * THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL COPYRIGHT HOLDERS OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 *
 * The views and conclusions contained in the software and documentation are
 * those of the authors and should not be interpreted as representing official
 * policies, either expressed or implied, of the copyright holders.
 */

#version 450

#extension GL_GOOGLE_include_directive : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

#include "descriptor_sets.inc.glsl"
#include "camera.inc.glsl"
#include "util.inc.glsl"

layout (constant_id = 0) const uint kernelSize = 17;

layout(push_constant, std430) uniform Params {
    vec2 dir;
    float sssWidth;
} params;

layout(set = DESCRIPTOR_SET_FRAME, binding = 0) uniform CameraUBO {
    Camera camera;
};

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform sampler2D sourceImage;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform restrict writeonly image2D destImage;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D sourceDepth;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform Kernel {
    vec4[kernelSize] kernel;
};

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 4) uniform sampler2D sssIntensityImage;

void main() {
    ivec2 outputPx = ivec2(gl_GlobalInvocationID.xy);

    ivec2 outputSize = imageSize(destImage);
    if (any(greaterThanEqual(outputPx, outputSize))) {
        return;
    }

    vec2 texcoord = (vec2(outputPx) + 0.5) / vec2(outputSize);

    vec4 colorM = textureLod(sourceImage, texcoord, 0.0);
    float sssIntensity = textureLod(sssIntensityImage, texcoord, 0.0).r;

    if (sssIntensity == 0.0) {
        imageStore(destImage, outputPx, colorM);
        return;
    }

    float depth = textureLod(sourceDepth, texcoord, 0.0).r;
    float depthM = linearizeDepth(depth, camera.zNear, camera.zFar);

    // Calculate the sssWidth scale (1.0 for a unit plane sitting on the
    // projection window)
    float fovY = calculateVerticalFov(camera.fov, float(outputSize.x) / float(outputSize.y));
    float distanceToProjectionWindow = 1.0 / tan(0.5 * radians(fovY));
    float scale = distanceToProjectionWindow / depthM;

    // Calculate the final step to fetch the surrounding pixels
    vec2 finalStep = params.sssWidth * scale * params.dir;
    finalStep *= sssIntensity; // Modulate it using the alpha channel.
    finalStep *= 0.333; // Divide by 3 as the kernels range from -3 to 3.

    // Accumulate the center sample
    vec4 colorBlurred = colorM;
    colorBlurred.rgb *= kernel[0].rgb;

    for (int i = 1; i < kernelSize; i++) {
        // Fetch color and depth for current sample
        vec2 offset = texcoord + kernel[i].a * finalStep;
        vec4 color = textureLod(sourceImage, offset, 0.0);

        #ifdef SSSS_FOLLOW_SURFACE
        // If the difference in depth is huge, we lerp color back to "colorM":
        float depth = linearizeDepth(camera, textureLod(sourceDepth, offset, 0.0).r);
        float s = clamp(300.0f * distanceToProjectionWindow *
        params.sssWidth * abs(depthM - depth), 0.0, 1.0);
        color.rgb = mix(color.rgb, colorM.rgb, s);
        #endif

        // Accumulate
        colorBlurred.rgb += kernel[i].rgb * color.rgb;
    }

    imageStore(destImage, outputPx, colorBlurred);
}
