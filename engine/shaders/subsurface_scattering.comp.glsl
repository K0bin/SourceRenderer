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

#define USE_17_SAMPLES

#ifdef USE_25_SAMPLES
const int kernelSize = 25;

const vec4 kernel[kernelSize] = vec4[](
    vec4(0.530605, 0.613514, 0.739601, 0),
    vec4(0.000973794, 1.11862e-005, 9.43437e-007, -3),
    vec4(0.00333804, 7.85443e-005, 1.2945e-005, -2.52083),
    vec4(0.00500364, 0.00020094, 5.28848e-005, -2.08333),
    vec4(0.00700976, 0.00049366, 0.000151938, -1.6875),
    vec4(0.0094389, 0.00139119, 0.000416598, -1.33333),
    vec4(0.0128496, 0.00356329, 0.00132016, -1.02083),
    vec4(0.017924, 0.00711691, 0.00347194, -0.75),
    vec4(0.0263642, 0.0119715, 0.00684598, -0.520833),
    vec4(0.0410172, 0.0199899, 0.0118481, -0.333333),
    vec4(0.0493588, 0.0367726, 0.0219485, -0.1875),
    vec4(0.0402784, 0.0657244, 0.04631, -0.0833333),
    vec4(0.0211412, 0.0459286, 0.0378196, -0.0208333),
    vec4(0.0211412, 0.0459286, 0.0378196, 0.0208333),
    vec4(0.0402784, 0.0657244, 0.04631, 0.0833333),
    vec4(0.0493588, 0.0367726, 0.0219485, 0.1875),
    vec4(0.0410172, 0.0199899, 0.0118481, 0.333333),
    vec4(0.0263642, 0.0119715, 0.00684598, 0.520833),
    vec4(0.017924, 0.00711691, 0.00347194, 0.75),
    vec4(0.0128496, 0.00356329, 0.00132016, 1.02083),
    vec4(0.0094389, 0.00139119, 0.000416598, 1.33333),
    vec4(0.00700976, 0.00049366, 0.000151938, 1.6875),
    vec4(0.00500364, 0.00020094, 5.28848e-005, 2.08333),
    vec4(0.00333804, 7.85443e-005, 1.2945e-005, 2.52083),
    vec4(0.000973794, 1.11862e-005, 9.43437e-007, 3)
);
#endif //USE_25_SAMPLES

#ifdef USE_17_SAMPLES
const int kernelSize = 17;
const vec4 kernel[kernelSize] = vec4[](
    vec4(0.536343, 0.624624, 0.748867, 0),
    vec4(0.00317394, 0.000134823, 3.77269e-005, -2),
    vec4(0.0100386, 0.000914679, 0.000275702, -1.53125),
    vec4(0.0144609, 0.00317269, 0.00106399, -1.125),
    vec4(0.0216301, 0.00794618, 0.00376991, -0.78125),
    vec4(0.0347317, 0.0151085, 0.00871983, -0.5),
    vec4(0.0571056, 0.0287432, 0.0172844, -0.28125),
    vec4(0.0582416, 0.0659959, 0.0411329, -0.125),
    vec4(0.0324462, 0.0656718, 0.0532821, -0.03125),
    vec4(0.0324462, 0.0656718, 0.0532821, 0.03125),
    vec4(0.0582416, 0.0659959, 0.0411329, 0.125),
    vec4(0.0571056, 0.0287432, 0.0172844, 0.28125),
    vec4(0.0347317, 0.0151085, 0.00871983, 0.5),
    vec4(0.0216301, 0.00794618, 0.00376991, 0.78125),
    vec4(0.0144609, 0.00317269, 0.00106399, 1.125),
    vec4(0.0100386, 0.000914679, 0.000275702, 1.53125),
    vec4(0.00317394, 0.000134823, 3.77269e-005, 2)
);
#endif //USE_17_SAMPLES

#ifdef USE_11_SAMPLES
const int kernelSize = 11;
const vec4 kernel[kernelSize] = vec4[](
    vec4(0.560479, 0.669086, 0.784728, 0),
    vec4(0.00471691, 0.000184771, 5.07566e-005, -2),
    vec4(0.0192831, 0.00282018, 0.00084214, -1.28),
    vec4(0.03639, 0.0130999, 0.00643685, -0.72),
    vec4(0.0821904, 0.0358608, 0.0209261, -0.32),
    vec4(0.0771802, 0.113491, 0.0793803, -0.08),
    vec4(0.0771802, 0.113491, 0.0793803, 0.08),
    vec4(0.0821904, 0.0358608, 0.0209261, 0.32),
    vec4(0.03639, 0.0130999, 0.00643685, 0.72),
    vec4(0.0192831, 0.00282018, 0.00084214, 1.28),
    vec4(0.00471691, 0.000184771, 5.07565e-005, 2)
);
#endif //USE_11_SAMPLES

layout(push_constant, std430) uniform Params {
	vec2 dir;
	float sssWidth;
} params;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0) uniform sampler2D sourceImage;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1) uniform restrict writeonly image2D destImage;
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2) uniform sampler2D sourceDepth;

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 3) uniform CameraUBO {
  Camera camera;
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
