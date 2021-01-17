#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_normal;

layout(location = 0) out vec3 out_position;
layout(location = 1) out vec3 out_normal;
layout(location = 2) out vec3 out_oldPosition;

layout(set = 2, binding = 0) uniform LowFrequencyUbo {
    mat4 viewProjection;
    mat4 oldViewProjection;
};

layout(set = 0, binding = 0) uniform HighFrequencyUbo {
    mat4 model;
    mat4 oldModel;
};

void main(void) {
    vec4 pos = vec4(in_pos, 1);

    vec4 transformedPos = (viewProjection * model) * pos;
    transformedPos.y = -transformedPos.y;

    vec4 transformedOldPos = (oldViewProjection * oldModel) * pos;
    transformedOldPos.y = -transformedOldPos.y;

    out_normal = in_normal;
    out_position = transformedPos.xyz;
    out_oldPosition = transformedOldPos.xyz;
    gl_Position = transformedPos;
}
