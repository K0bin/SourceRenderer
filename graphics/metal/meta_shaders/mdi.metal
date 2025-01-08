//
//  mdi.metal
//  
//
//  Created by Robin Kertels on 06.05.24.
//

#include <metal_stdlib>
using namespace metal;

struct VkDrawIndirectCommand {
    uint32_t    vertexCount;
    uint32_t    instanceCount;
    uint32_t    firstVertex;
    uint32_t    firstInstance;
};

struct Parameters
{
    command_buffer commandBuffer [[ id(0) ]];
    device VkDrawIndirectCommand *commands [[ id(1) ]];
    device uint32_t *count [[ id(2) ]];
    size_t stride;
    primitive_type primitive_type;
};

kernel void writeMDICommands(constant Parameters *params [[buffer(0)]], uint3 global_invocation_id [[thread_position_in_grid]]) {
                                 uint32_t actual_count = *params->count;
    uint32_t thread_id = global_invocation_id.x;
    if (thread_id >= actual_count) {
        return;
    }
     device uint8_t *ptr = reinterpret_cast<device uint8_t*>(params->commands);
     ptr += params->stride * thread_id;
     device VkDrawIndirectCommand *command = reinterpret_cast<device VkDrawIndirectCommand*>(ptr);

     render_command cmd(params->commandBuffer, thread_id);
     cmd.draw_primitives(params->primitive_type,
                         command->firstVertex,
                         command->vertexCount,
                         command->instanceCount,
                         command->firstInstance);
}
