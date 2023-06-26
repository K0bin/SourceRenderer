#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_GOOGLE_include_directive : enable

#include "descriptor_sets.inc.glsl"
#include "gpu_scene.inc.glsl"
#include "camera.inc.glsl"

layout(location = 0) in vec3 in_pos;

layout(push_constant) uniform VeryHighFrequencyUbo {
    mat4 viewProj;
};

#include "frame_set.inc.glsl"

invariant gl_Position;

void main(void) {
    uint drawIndex = gl_InstanceIndex;
    GPUDraw draw = scene_draws[drawIndex];
    GPUMeshPart part = scene_parts[draw.partIndex];
    uint materialIndex = part.materialIndex;
    uint drawableIndex = draw.drawableIndex;
    GPUDrawable drawable = scene_drawables[drawableIndex];
    mat4 model = drawable.transform;

    vec4 pos = vec4(in_pos, 1);
    mat4 mvp = viewProj * model;
    gl_Position = mvp * pos;

    // Pancaking
    gl_Position.z = max(gl_Position.z, 0.01);
}
