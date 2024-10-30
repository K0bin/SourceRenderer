#version 460
#extension GL_EXT_nonuniform_qualifier : enable

layout(local_size_x = 8,
       local_size_y = 8,
       local_size_z = 1) in;

layout(set = 0, binding = 0, rgba8) uniform image2D image;
layout(set = 0, binding = 1) uniform sampler linearSampler;
layout(set = 3, binding = 0) uniform texture2D textures[];


struct AnotherStruct {
    uint _unused;
};
layout(set = 2, binding = 0, std430) readonly restrict buffer anotherBuffer {
  AnotherStruct arr[];
};

void main() {
    ivec2 texSize = imageSize(image);
    if (gl_GlobalInvocationID.x >= texSize.x || gl_GlobalInvocationID.y >= texSize.y) {
        return;
    }
    vec2 texCoord = vec2((float(gl_GlobalInvocationID.x) + 0.5) / float(texSize.x), (float(gl_GlobalInvocationID.y) + 0.5) / float(texSize.y));
    ivec2 iTexCoord = ivec2(gl_GlobalInvocationID.xy);

    uint index = iTexCoord.x; // anything that spirv cross cant statically evaluate, to make sure we get a runtime array

    vec3 color = texture(sampler2D(textures[index], linearSampler), texCoord).xyz;
    imageStore(image, iTexCoord, vec4(color, 1.0));

    return;
}
