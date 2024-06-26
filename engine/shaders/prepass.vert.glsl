#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_normal;

layout(location = 0) out vec4 out_position;
layout(location = 1) out vec3 out_normal;
layout(location = 2) out vec4 out_oldPosition;

layout(set = DESCRIPTOR_SET_FREQUENT, binding = 0) uniform CurrentLowFrequencyUbo {
    mat4 viewProj;
    mat4 invProj;
    mat4 view;
    mat4 proj;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 1) uniform PreviousLowFrequencyUbo {
    mat4 oldViewProjection;
};
layout(set = DESCRIPTOR_SET_FREQUENT, binding = 2) uniform PerFrameUbo {
    mat4 swapchainTransform;
    vec2 jitterPoint;
};

layout(push_constant) uniform VeryHighFrequencyUbo {
    mat4 model;
    mat4 oldModel;
};

invariant gl_Position;

void main(void) {
    vec4 pos = vec4(in_pos, 1);

    mat4 mvp = swapchainTransform * viewProj * model;
    vec4 transformedPos = mvp * pos;

    vec4 transformedOldPos = (swapchainTransform * (oldViewProjection * oldModel)) * pos;

    mat4 normalMat = transpose(inverse(model));
    out_normal = normalize((normalMat * vec4(in_normal, 0.0)).xyz); // shouldnt be necessary
    out_position = transformedPos;
    out_oldPosition = transformedOldPos;

    mat4 jitterMat;
    jitterMat[0] = vec4(1.0, 0.0, 0.0, 0.0);
    jitterMat[1] = vec4(0.0, 1.0, 0.0, 0.0);
    jitterMat[2] = vec4(0.0, 0.0, 1.0, 0.0);
    jitterMat[3] = vec4(jitterPoint.x, jitterPoint.y, 0.0, 1.0);
    vec4 jitteredPoint = (jitterMat * mvp) * pos;
    gl_Position = jitteredPoint;
}
