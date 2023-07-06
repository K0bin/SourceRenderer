use std::collections::HashMap;
use std::ffi::{
    CStr,
    CString,
};
use std::hash::{
    Hash,
    Hasher,
};
use std::os::raw::c_char;
use std::sync::Arc;

use ash::vk;
use ash::vk::{
    Handle,
    PipelineRasterizationStateCreateFlags,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::*;
use spirv_cross_sys;

use super::*;

const BINDLESS_TEXTURE_SET_INDEX: u32 = 3;

#[inline]
pub(crate) fn input_rate_to_vk(input_rate: InputRate) -> vk::VertexInputRate {
    match input_rate {
        InputRate::PerVertex => vk::VertexInputRate::VERTEX,
        InputRate::PerInstance => vk::VertexInputRate::INSTANCE,
    }
}

pub struct VkShader {
    shader_type: ShaderType,
    shader_module: vk::ShaderModule,
    device: Arc<RawVkDevice>,
    descriptor_set_bindings: HashMap<u32, Vec<VkDescriptorSetEntryInfo>>,
    push_constants_range: Option<vk::PushConstantRange>,
    uses_bindless_texture_set: bool,
}

impl PartialEq for VkShader {
    fn eq(&self, other: &Self) -> bool {
        self.shader_module == other.shader_module
    }
}

impl Eq for VkShader {}

impl Hash for VkShader {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shader_module.hash(state);
    }
}

impl VkShader {
    #[allow(clippy::size_of_in_element_count)]
    pub fn new(
        device: &Arc<RawVkDevice>,
        shader_type: ShaderType,
        bytecode: &[u8],
        name: Option<&str>,
    ) -> Self {
        let create_info = vk::ShaderModuleCreateInfo {
            code_size: bytecode.len(),
            p_code: bytecode.as_ptr() as *const u32,
            ..Default::default()
        };
        let vk_device = &device.device;
        let shader_module = unsafe { vk_device.create_shader_module(&create_info, None).unwrap() };
        let mut uses_bindless_texture_set = false;
        let mut sets: HashMap<u32, Vec<VkDescriptorSetEntryInfo>> = HashMap::new();

        let mut context: spirv_cross_sys::spvc_context = std::ptr::null_mut();
        let mut ir: spirv_cross_sys::spvc_parsed_ir = std::ptr::null_mut();
        let mut compiler: spirv_cross_sys::spvc_compiler = std::ptr::null_mut();
        let mut resources: spirv_cross_sys::spvc_resources = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                spirv_cross_sys::spvc_context_create(&mut context),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            assert_eq!(
                spirv_cross_sys::spvc_context_parse_spirv(
                    context,
                    bytecode.as_ptr() as *const u32,
                    (bytecode.len() / std::mem::size_of::<u32>()) as u64,
                    &mut ir
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            assert_eq!(
                spirv_cross_sys::spvc_context_create_compiler(
                    context,
                    spirv_cross_sys::spvc_backend_SPVC_BACKEND_NONE,
                    ir,
                    spirv_cross_sys::spvc_capture_mode_SPVC_CAPTURE_MODE_COPY,
                    &mut compiler
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );

            assert_eq!(
                spirv_cross_sys::spvc_compiler_create_shader_resources(compiler, &mut resources),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
        }

        let push_constant_buffers = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_PUSH_CONSTANT,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        let push_constant_resource = push_constant_buffers.first();
        let push_constants_range = push_constant_resource
            .map(|resource| unsafe {
                let type_handle = spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                assert_ne!(type_handle, std::ptr::null());
                let mut size = 0u64;
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_get_declared_struct_size(compiler, type_handle, &mut size as *mut u64),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
                let push_constant_range = vk::PushConstantRange {
                    stage_flags: match shader_type {
                        ShaderType::VertexShader => vk::ShaderStageFlags::VERTEX,
                        ShaderType::FragmentShader => vk::ShaderStageFlags::FRAGMENT,
                        ShaderType::ComputeShader => vk::ShaderStageFlags::COMPUTE,
                        ShaderType::RayGen => vk::ShaderStageFlags::RAYGEN_KHR,
                        ShaderType::RayMiss => vk::ShaderStageFlags::MISS_KHR,
                        ShaderType::RayClosestHit => vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                        _ => unimplemented!(),
                    },
                    offset: 0u32,
                    size: size as u32,
                };

                if push_constant_range.size > 128 {
                    panic!(
                        "Shader push constants exceed the size limit of 128 bytes, name: {:?}",
                        name
                    );
                }

                push_constant_range
            });

        let separate_images = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_IMAGE,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in separate_images {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);
            if set_index == BINDLESS_TEXTURE_SET_INDEX {
                uses_bindless_texture_set = true;
                continue;
            }

            let array_size = unsafe {
                let type_handle =
                    spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(type_handle, 0) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };

            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                shader_stage: shader_type_to_vk(shader_type),
                count: array_size,
                writable: false,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let separate_samplers = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_SAMPLERS,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in separate_samplers {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);

            let array_size = unsafe {
                let type_handle =
                    spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(type_handle, 0) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };

            assert_eq!(array_size, 1);
            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::SAMPLER,
                shader_stage: shader_type_to_vk(shader_type),
                count: array_size,
                writable: false,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let sampled_images = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SAMPLED_IMAGE,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in sampled_images {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);

            let array_size = unsafe {
                let type_handle =
                    spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(type_handle, 0) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };

            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                shader_stage: shader_type_to_vk(shader_type),
                count: array_size,
                writable: false,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let subpass_inputs = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SUBPASS_INPUT,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in subpass_inputs {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);
            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::INPUT_ATTACHMENT,
                shader_stage: shader_type_to_vk(shader_type),
                count: 1,
                writable: false,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let uniform_buffers = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_UNIFORM_BUFFER,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in uniform_buffers {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);

            let array_size = unsafe {
                let type_handle =
                    spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(type_handle, 0) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };

            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                shader_stage: shader_type_to_vk(shader_type),
                count: array_size,
                writable: false,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let storage_buffers = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_BUFFER,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in storage_buffers {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let writable = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationNonWritable,
                )
            } == 0;
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);

            let array_size = unsafe {
                let type_handle =
                    spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(type_handle, 0) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };

            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                shader_stage: shader_type_to_vk(shader_type),
                count: array_size,
                writable,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let storage_images = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_IMAGE,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in storage_images {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let writable = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationNonWritable,
                )
            } == 0;
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);

            let array_size = unsafe {
                let type_handle =
                    spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(type_handle, 0) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };
            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::STORAGE_IMAGE,
                shader_stage: shader_type_to_vk(shader_type),
                count: array_size,
                writable,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        let acceleration_structures = unsafe {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: u64 = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    resources,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_ACCELERATION_STRUCTURE,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in acceleration_structures {
            let set_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                )
            };
            let binding_index = unsafe {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
                )
            };
            let name = unsafe {
                CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                    compiler,
                    resource.id,
                ))
                .to_str()
                .unwrap()
                .to_string()
            };
            let set = sets.entry(set_index).or_insert_with(Vec::new);
            set.push(VkDescriptorSetEntryInfo {
                name,
                index: binding_index,
                descriptor_type: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                shader_stage: shader_type_to_vk(shader_type),
                count: 1,
                writable: false,
                flags: vk::DescriptorBindingFlags::empty(),
            });
        }

        if let Some(name) = name {
            if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .debug_utils_loader
                        .set_debug_utils_object_name(
                            device.handle(),
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::SHADER_MODULE,
                                object_handle: shader_module.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        unsafe {
            spirv_cross_sys::spvc_context_destroy(context);
        }

        VkShader {
            shader_type,
            shader_module,
            device: device.clone(),
            descriptor_set_bindings: sets,
            push_constants_range,
            uses_bindless_texture_set,
        }
    }

    fn shader_module(&self) -> vk::ShaderModule {
        self.shader_module
    }
}

impl Shader for VkShader {
    fn shader_type(&self) -> ShaderType {
        self.shader_type
    }
}

impl Drop for VkShader {
    fn drop(&mut self) {
        unsafe {
            let vk_device = &self.device.device;
            vk_device.destroy_shader_module(self.shader_module, None);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VkPipelineType {
    Graphics,
    Compute,
    RayTracing,
}

pub struct VkPipeline {
    pipeline: vk::Pipeline,
    layout: Arc<VkPipelineLayout>,
    device: Arc<RawVkDevice>,
    pipeline_type: VkPipelineType,
    uses_bindless_texture_set: bool,
    sbt: Option<VkShaderBindingTables>,
}

struct VkShaderBindingTables {
    buffer: VkBuffer,
    raygen_region: vk::StridedDeviceAddressRegionKHR,
    closest_hit_region: vk::StridedDeviceAddressRegionKHR,
    miss_region: vk::StridedDeviceAddressRegionKHR,
}

impl PartialEq for VkPipeline {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline == other.pipeline
    }
}

const SHADER_ENTRY_POINT_NAME: &str = "main";

pub fn shader_type_to_vk(shader_type: ShaderType) -> vk::ShaderStageFlags {
    match shader_type {
        ShaderType::VertexShader => vk::ShaderStageFlags::VERTEX,
        ShaderType::FragmentShader => vk::ShaderStageFlags::FRAGMENT,
        ShaderType::GeometryShader => vk::ShaderStageFlags::GEOMETRY,
        ShaderType::TessellationControlShader => vk::ShaderStageFlags::TESSELLATION_CONTROL,
        ShaderType::TessellationEvaluationShader => vk::ShaderStageFlags::TESSELLATION_EVALUATION,
        ShaderType::ComputeShader => vk::ShaderStageFlags::COMPUTE,
        ShaderType::RayClosestHit => vk::ShaderStageFlags::CLOSEST_HIT_KHR,
        ShaderType::RayGen => vk::ShaderStageFlags::RAYGEN_KHR,
        ShaderType::RayMiss => vk::ShaderStageFlags::MISS_KHR,
    }
}

pub fn samples_to_vk(samples: SampleCount) -> vk::SampleCountFlags {
    match samples {
        SampleCount::Samples1 => vk::SampleCountFlags::TYPE_1,
        SampleCount::Samples2 => vk::SampleCountFlags::TYPE_2,
        SampleCount::Samples4 => vk::SampleCountFlags::TYPE_4,
        SampleCount::Samples8 => vk::SampleCountFlags::TYPE_8,
    }
}

pub fn compare_func_to_vk(compare_func: CompareFunc) -> vk::CompareOp {
    match compare_func {
        CompareFunc::Always => vk::CompareOp::ALWAYS,
        CompareFunc::NotEqual => vk::CompareOp::NOT_EQUAL,
        CompareFunc::Never => vk::CompareOp::NEVER,
        CompareFunc::Less => vk::CompareOp::LESS,
        CompareFunc::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
        CompareFunc::Equal => vk::CompareOp::EQUAL,
        CompareFunc::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        CompareFunc::Greater => vk::CompareOp::GREATER,
    }
}

pub fn stencil_op_to_vk(stencil_op: StencilOp) -> vk::StencilOp {
    match stencil_op {
        StencilOp::Decrease => vk::StencilOp::DECREMENT_AND_WRAP,
        StencilOp::Increase => vk::StencilOp::INCREMENT_AND_WRAP,
        StencilOp::DecreaseClamp => vk::StencilOp::DECREMENT_AND_CLAMP,
        StencilOp::IncreaseClamp => vk::StencilOp::INCREMENT_AND_CLAMP,
        StencilOp::Invert => vk::StencilOp::INVERT,
        StencilOp::Keep => vk::StencilOp::KEEP,
        StencilOp::Replace => vk::StencilOp::REPLACE,
        StencilOp::Zero => vk::StencilOp::ZERO,
    }
}

pub fn logic_op_to_vk(logic_op: LogicOp) -> vk::LogicOp {
    match logic_op {
        LogicOp::And => vk::LogicOp::AND,
        LogicOp::AndInverted => vk::LogicOp::AND_INVERTED,
        LogicOp::AndReversed => vk::LogicOp::AND_REVERSE,
        LogicOp::Clear => vk::LogicOp::CLEAR,
        LogicOp::Copy => vk::LogicOp::COPY,
        LogicOp::CopyInverted => vk::LogicOp::COPY_INVERTED,
        LogicOp::Equivalent => vk::LogicOp::EQUIVALENT,
        LogicOp::Invert => vk::LogicOp::INVERT,
        LogicOp::Nand => vk::LogicOp::NAND,
        LogicOp::Noop => vk::LogicOp::NO_OP,
        LogicOp::Nor => vk::LogicOp::NOR,
        LogicOp::Or => vk::LogicOp::OR,
        LogicOp::OrInverted => vk::LogicOp::OR_INVERTED,
        LogicOp::OrReverse => vk::LogicOp::OR_REVERSE,
        LogicOp::Set => vk::LogicOp::SET,
        LogicOp::Xor => vk::LogicOp::XOR,
    }
}

pub fn blend_factor_to_vk(blend_factor: BlendFactor) -> vk::BlendFactor {
    match blend_factor {
        BlendFactor::ConstantColor => vk::BlendFactor::CONSTANT_COLOR,
        BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
        BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
        BlendFactor::One => vk::BlendFactor::ONE,
        BlendFactor::OneMinusConstantColor => vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR,
        BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
        BlendFactor::OneMinusSrc1Alpha => vk::BlendFactor::ONE_MINUS_SRC1_ALPHA,
        BlendFactor::OneMinusSrc1Color => vk::BlendFactor::ONE_MINUS_SRC1_COLOR,
        BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        BlendFactor::Src1Alpha => vk::BlendFactor::SRC1_ALPHA,
        BlendFactor::Src1Color => vk::BlendFactor::SRC1_COLOR,
        BlendFactor::SrcAlphaSaturate => vk::BlendFactor::SRC_ALPHA_SATURATE,
        BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
        BlendFactor::Zero => vk::BlendFactor::ZERO,
        BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
        BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
    }
}

pub fn blend_op_to_vk(blend_op: BlendOp) -> vk::BlendOp {
    match blend_op {
        BlendOp::Add => vk::BlendOp::ADD,
        BlendOp::Max => vk::BlendOp::MAX,
        BlendOp::Min => vk::BlendOp::MIN,
        BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
        BlendOp::Subtract => vk::BlendOp::SUBTRACT,
    }
}

pub fn color_components_to_vk(color_components: ColorComponents) -> vk::ColorComponentFlags {
    let components_bits = color_components.bits() as u32;
    let mut colors = 0u32;
    colors |= components_bits.rotate_left(
        ColorComponents::RED.bits().trailing_zeros()
            - vk::ColorComponentFlags::R.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::R.as_raw();
    colors |= components_bits.rotate_left(
        ColorComponents::GREEN.bits().trailing_zeros()
            - vk::ColorComponentFlags::G.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::G.as_raw();
    colors |= components_bits.rotate_left(
        ColorComponents::BLUE.bits().trailing_zeros()
            - vk::ColorComponentFlags::B.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::B.as_raw();
    colors |= components_bits.rotate_left(
        ColorComponents::ALPHA.bits().trailing_zeros()
            - vk::ColorComponentFlags::A.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::A.as_raw();
    vk::ColorComponentFlags::from_raw(colors)
}

#[derive(Hash, Eq, PartialEq)]
pub struct VkGraphicsPipelineInfo<'a> {
    pub info: &'a GraphicsPipelineInfo<'a, VkBackend>,
    pub render_pass: &'a VkRenderPass,
    pub sub_pass: u32,
}

impl VkPipeline {
    pub fn new_graphics(
        device: &Arc<RawVkDevice>,
        info: &VkGraphicsPipelineInfo,
        shared: &VkShared,
        name: Option<&str>,
    ) -> Self {
        let vk_device = &device.device;
        let mut shader_stages: Vec<vk::PipelineShaderStageCreateInfo> = Vec::new();
        let mut descriptor_set_layouts =
            <[VkDescriptorSetLayoutKey; (BINDLESS_TEXTURE_SET_INDEX + 1) as usize]>::default();
        let mut push_constants_ranges = <[Option<VkConstantRange>; 3]>::default();
        let mut uses_bindless_texture_set = false;

        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();
        let mut dynamic_storage_buffers = [0; 4];
        let mut dynamic_uniform_buffers = [0; 4];

        {
            let shader = info.info.vs.clone();
            let shader_stage = vk::PipelineShaderStageCreateInfo {
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                stage: shader_type_to_vk(shader.shader_type()),
                ..Default::default()
            };
            shader_stages.push(shader_stage);
            for (index, shader_set) in &shader.descriptor_set_bindings {
                let set = &mut descriptor_set_layouts[*index as usize];
                for binding in shader_set {
                    let existing_binding_option = set
                        .bindings
                        .iter_mut()
                        .find(|existing_binding| existing_binding.index == binding.index);
                    if let Some(existing_binding) = existing_binding_option {
                        if existing_binding.descriptor_type
                            == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                        } else if existing_binding.descriptor_type
                            == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                        } else {
                            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                        }
                        existing_binding.shader_stage |= binding.shader_stage;
                    } else {
                        let mut binding_clone = binding.clone();
                        if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                            && dynamic_storage_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_storage_buffers_dynamic
                        {
                            dynamic_storage_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                        }
                        if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                            && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_uniform_buffers_dynamic
                        {
                            dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                        }
                        set.bindings.push(binding_clone);
                    }
                }
            }
            if let Some(push_constants_range) = &shader.push_constants_range {
                push_constants_ranges[0] = Some(VkConstantRange {
                    offset: push_constants_range.offset,
                    size: push_constants_range.size,
                    shader_stage: vk::ShaderStageFlags::VERTEX,
                });
            }
            uses_bindless_texture_set |= shader.uses_bindless_texture_set;
        }

        if let Some(shader) = info.info.fs.clone() {
            let shader_stage = vk::PipelineShaderStageCreateInfo {
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                stage: shader_type_to_vk(shader.shader_type()),
                ..Default::default()
            };
            shader_stages.push(shader_stage);
            for (index, shader_set) in &shader.descriptor_set_bindings {
                let set = &mut descriptor_set_layouts[*index as usize];
                for binding in shader_set {
                    let existing_binding_option = set
                        .bindings
                        .iter_mut()
                        .find(|existing_binding| existing_binding.index == binding.index);
                    if let Some(existing_binding) = existing_binding_option {
                        if existing_binding.descriptor_type
                            == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                        } else if existing_binding.descriptor_type
                            == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                        } else {
                            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                        }
                        existing_binding.shader_stage |= binding.shader_stage;
                    } else {
                        let mut binding_clone = binding.clone();
                        if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                            && dynamic_storage_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_storage_buffers_dynamic
                        {
                            dynamic_storage_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                        }
                        if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                            && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_uniform_buffers_dynamic
                        {
                            dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                        }
                        set.bindings.push(binding_clone);
                    }
                }
            }
            if let Some(push_constants_range) = &shader.push_constants_range {
                push_constants_ranges[1] = Some(VkConstantRange {
                    offset: push_constants_range.offset,
                    size: push_constants_range.size,
                    shader_stage: vk::ShaderStageFlags::FRAGMENT,
                });
            }
            uses_bindless_texture_set |= shader.uses_bindless_texture_set;
        }

        let mut attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = Vec::new();
        let mut binding_descriptions: Vec<vk::VertexInputBindingDescription> = Vec::new();
        for element in info.info.vertex_layout.shader_inputs {
            attribute_descriptions.push(vk::VertexInputAttributeDescription {
                location: element.location_vk_mtl,
                binding: element.input_assembler_binding,
                format: format_to_vk(element.format, false),
                offset: element.offset as u32,
            });
        }

        for element in info.info.vertex_layout.input_assembler {
            binding_descriptions.push(vk::VertexInputBindingDescription {
                binding: element.binding,
                stride: element.stride as u32,
                input_rate: input_rate_to_vk(element.input_rate),
            });
        }

        let vertex_input_create_info = vk::PipelineVertexInputStateCreateInfo {
            vertex_binding_description_count: binding_descriptions.len() as u32,
            p_vertex_binding_descriptions: binding_descriptions.as_ptr(),
            vertex_attribute_description_count: attribute_descriptions.len() as u32,
            p_vertex_attribute_descriptions: attribute_descriptions.as_ptr(),
            ..Default::default()
        };

        let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo {
            topology: match info.info.primitive_type {
                PrimitiveType::Triangles => vk::PrimitiveTopology::TRIANGLE_LIST,
                PrimitiveType::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
                PrimitiveType::Lines => vk::PrimitiveTopology::LINE_LIST,
                PrimitiveType::LineStrip => vk::PrimitiveTopology::LINE_STRIP,
                PrimitiveType::Points => vk::PrimitiveTopology::POINT_LIST,
            },
            primitive_restart_enable: false as u32,
            ..Default::default()
        };

        let rasterizer_create_info = vk::PipelineRasterizationStateCreateInfo {
            flags: PipelineRasterizationStateCreateFlags::empty(),
            depth_clamp_enable: vk::FALSE,
            rasterizer_discard_enable: vk::FALSE,
            polygon_mode: match &info.info.rasterizer.fill_mode {
                FillMode::Fill => vk::PolygonMode::FILL,
                FillMode::Line => vk::PolygonMode::LINE,
            },
            cull_mode: match &info.info.rasterizer.cull_mode {
                CullMode::Back => vk::CullModeFlags::BACK,
                CullMode::Front => vk::CullModeFlags::FRONT,
                CullMode::None => vk::CullModeFlags::NONE,
            },
            front_face: match &info.info.rasterizer.front_face {
                FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
                FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
            },
            depth_bias_enable: vk::FALSE,
            depth_bias_constant_factor: 0.0f32,
            depth_bias_clamp: 0.0f32,
            depth_bias_slope_factor: 0.0f32,
            line_width: 1.0f32,
            ..Default::default()
        };

        let multisample_create_info = vk::PipelineMultisampleStateCreateInfo {
            rasterization_samples: samples_to_vk(info.info.rasterizer.sample_count),
            alpha_to_coverage_enable: info.info.blend.alpha_to_coverage_enabled as u32,
            ..Default::default()
        };

        let depth_stencil_create_info = vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: info.info.depth_stencil.depth_test_enabled as u32,
            depth_write_enable: info.info.depth_stencil.depth_write_enabled as u32,
            depth_compare_op: compare_func_to_vk(info.info.depth_stencil.depth_func),
            depth_bounds_test_enable: vk::FALSE,
            stencil_test_enable: info.info.depth_stencil.stencil_enable as u32,
            front: vk::StencilOpState {
                pass_op: stencil_op_to_vk(info.info.depth_stencil.stencil_front.pass_op),
                fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_front.fail_op),
                depth_fail_op: stencil_op_to_vk(
                    info.info.depth_stencil.stencil_front.depth_fail_op,
                ),
                compare_op: compare_func_to_vk(info.info.depth_stencil.stencil_front.func),
                write_mask: info.info.depth_stencil.stencil_write_mask as u32,
                compare_mask: info.info.depth_stencil.stencil_read_mask as u32,
                reference: 0u32,
            },
            back: vk::StencilOpState {
                pass_op: stencil_op_to_vk(info.info.depth_stencil.stencil_back.pass_op),
                fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_back.fail_op),
                depth_fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_back.depth_fail_op),
                compare_op: compare_func_to_vk(info.info.depth_stencil.stencil_back.func),
                write_mask: info.info.depth_stencil.stencil_write_mask as u32,
                compare_mask: info.info.depth_stencil.stencil_read_mask as u32,
                reference: 0u32,
            },
            min_depth_bounds: 0.0,
            max_depth_bounds: 0.0,
            ..Default::default()
        };

        let mut blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> = Vec::new();
        for blend in info.info.blend.attachments {
            blend_attachments.push(vk::PipelineColorBlendAttachmentState {
                blend_enable: blend.blend_enabled as u32,
                src_color_blend_factor: blend_factor_to_vk(blend.src_color_blend_factor),
                dst_color_blend_factor: blend_factor_to_vk(blend.dst_color_blend_factor),
                color_blend_op: blend_op_to_vk(blend.color_blend_op),
                src_alpha_blend_factor: blend_factor_to_vk(blend.src_alpha_blend_factor),
                dst_alpha_blend_factor: blend_factor_to_vk(blend.dst_alpha_blend_factor),
                alpha_blend_op: blend_op_to_vk(blend.alpha_blend_op),
                color_write_mask: color_components_to_vk(blend.write_mask),
            });
        }
        let blend_create_info = vk::PipelineColorBlendStateCreateInfo {
            logic_op_enable: info.info.blend.logic_op_enabled as u32,
            logic_op: logic_op_to_vk(info.info.blend.logic_op),
            p_attachments: blend_attachments.as_ptr(),
            attachment_count: blend_attachments.len() as u32,
            blend_constants: info.info.blend.constants,
            ..Default::default()
        };

        let dynamic_state = [
            vk::DynamicState::VIEWPORT,
            vk::DynamicState::SCISSOR,
            vk::DynamicState::STENCIL_REFERENCE,
        ];
        let dynamic_state_create_info = vk::PipelineDynamicStateCreateInfo {
            p_dynamic_states: dynamic_state.as_ptr(),
            dynamic_state_count: dynamic_state.len() as u32,
            ..Default::default()
        };

        if uses_bindless_texture_set {
            /*if !device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
                panic!("Pipeline {:?} is trying to use the bindless texture descriptor set but the Vulkan device does not support descriptor indexing.", name);
            }

            descriptor_set_layouts[BINDLESS_TEXTURE_SET_INDEX as usize] =
                VkDescriptorSetLayoutKey {
                    bindings: vec![VkDescriptorSetEntryInfo {
                        name: "bindless_textures".to_string(),
                        shader_stage: vk::ShaderStageFlags::VERTEX
                            | vk::ShaderStageFlags::FRAGMENT
                            | vk::ShaderStageFlags::COMPUTE,
                        index: 0,
                        descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                        count: BINDLESS_TEXTURE_COUNT,
                        writable: false,
                        flags: vk::DescriptorBindingFlags::UPDATE_AFTER_BIND_EXT
                            | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING_EXT
                            | vk::DescriptorBindingFlags::PARTIALLY_BOUND_EXT,
                    }],
                    flags: vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT,
                };*/
        }

        let mut offset = 0u32;
        let mut remapped_push_constant_ranges = <[Option<VkConstantRange>; 3]>::default();
        if let Some(range) = &push_constants_ranges[0] {
            remapped_push_constant_ranges[0] = Some(VkConstantRange {
                offset,
                size: range.size,
                shader_stage: vk::ShaderStageFlags::VERTEX,
            });
            offset += range.size;
        }
        if let Some(range) = &push_constants_ranges[1] {
            remapped_push_constant_ranges[1] = Some(VkConstantRange {
                offset,
                size: range.size,
                shader_stage: vk::ShaderStageFlags::FRAGMENT,
            });
        }

        let layout = shared.get_pipeline_layout(&VkPipelineLayoutKey {
            descriptor_set_layouts,
            push_constant_ranges: remapped_push_constant_ranges,
        });

        let viewport_info = vk::PipelineViewportStateCreateInfo {
            viewport_count: 1,
            p_viewports: &vk::Viewport {
                x: 0f32,
                y: 0f32,
                width: 0f32,
                height: 0f32,
                min_depth: 0f32,
                max_depth: 1f32,
            },
            scissor_count: 1,
            p_scissors: &vk::Rect2D {
                offset: vk::Offset2D { x: 0i32, y: 0i32 },
                extent: vk::Extent2D {
                    width: 0u32,
                    height: 0u32,
                },
            },
            ..Default::default()
        };

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo {
            stage_count: shader_stages.len() as u32,
            p_stages: shader_stages.as_ptr(),
            p_vertex_input_state: &vertex_input_create_info,
            p_input_assembly_state: &input_assembly_info,
            p_rasterization_state: &rasterizer_create_info,
            p_multisample_state: &multisample_create_info,
            p_depth_stencil_state: &depth_stencil_create_info,
            p_color_blend_state: &blend_create_info,
            p_viewport_state: &viewport_info,
            p_tessellation_state: &vk::PipelineTessellationStateCreateInfo::default(),
            p_dynamic_state: &dynamic_state_create_info,
            layout: layout.handle(),
            render_pass: info.render_pass.handle(),
            subpass: info.sub_pass,
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0i32,
            ..Default::default()
        };

        let pipeline = unsafe {
            vk_device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_create_info], None)
                .unwrap()[0]
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .debug_utils_loader
                        .set_debug_utils_object_name(
                            device.handle(),
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        Self {
            pipeline,
            device: device.clone(),
            layout,
            pipeline_type: VkPipelineType::Graphics,
            uses_bindless_texture_set,
            sbt: None,
        }
    }

    pub fn new_compute(
        device: &Arc<RawVkDevice>,
        shader: &VkShader,
        shared: &VkShared,
        name: Option<&str>,
    ) -> Self {
        let mut descriptor_set_layouts: [VkDescriptorSetLayoutKey;
            (BINDLESS_TEXTURE_SET_INDEX + 1) as usize] = Default::default();
        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();

        let shader_stage = vk::PipelineShaderStageCreateInfo {
            module: shader.shader_module(),
            p_name: entry_point.as_ptr() as *const c_char,
            stage: shader_type_to_vk(shader.shader_type()),
            ..Default::default()
        };

        let mut dynamic_storage_buffers = [0; 4];
        let mut dynamic_uniform_buffers = [0; 4];
        for (index, shader_set) in &shader.descriptor_set_bindings {
            let set = &mut descriptor_set_layouts[*index as usize];
            for binding in shader_set {
                let existing_binding_option = set
                    .bindings
                    .iter_mut()
                    .find(|existing_binding| existing_binding.index == binding.index);
                if let Some(existing_binding) = existing_binding_option {
                    if existing_binding.descriptor_type
                        == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                    {
                        assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                    } else if existing_binding.descriptor_type
                        == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                    {
                        assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                    } else {
                        assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                    }
                    existing_binding.shader_stage |= binding.shader_stage;
                } else {
                    let mut binding_clone = binding.clone();
                    if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                        && dynamic_storage_buffers[*index as usize] + binding_clone.count
                            < device
                                .properties
                                .limits
                                .max_descriptor_set_storage_buffers_dynamic
                    {
                        dynamic_storage_buffers[*index as usize] += binding_clone.count;
                        binding_clone.descriptor_type = vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                    }
                    if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                        && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                            < device
                                .properties
                                .limits
                                .max_descriptor_set_uniform_buffers_dynamic
                    {
                        dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                        binding_clone.descriptor_type = vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                    }
                    set.bindings.push(binding_clone);
                }
            }
        }

        if shader.uses_bindless_texture_set {
            /*descriptor_set_layouts[BINDLESS_TEXTURE_SET_INDEX as usize] =
                VkDescriptorSetLayoutKey {
                    bindings: vec![VkDescriptorSetEntryInfo {
                        name: "bindless_textures".to_string(),
                        shader_stage: vk::ShaderStageFlags::VERTEX
                            | vk::ShaderStageFlags::FRAGMENT
                            | vk::ShaderStageFlags::COMPUTE,
                        index: 0,
                        descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                        count: BINDLESS_TEXTURE_COUNT,
                        writable: false,
                        flags: vk::DescriptorBindingFlags::UPDATE_AFTER_BIND_EXT
                            | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING_EXT
                            | vk::DescriptorBindingFlags::PARTIALLY_BOUND_EXT,
                    }],
                    flags: vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT,
                };*/
        }

        let mut push_constants_ranges = <[Option<VkConstantRange>; 3]>::default();
        if let Some(push_constants_range) = &shader.push_constants_range {
            push_constants_ranges[0] = Some(VkConstantRange {
                offset: push_constants_range.offset,
                size: push_constants_range.size,
                shader_stage: vk::ShaderStageFlags::COMPUTE,
            });
        }

        let layout = shared.get_pipeline_layout(&VkPipelineLayoutKey {
            descriptor_set_layouts,
            push_constant_ranges: push_constants_ranges,
        });

        let pipeline_create_info = vk::ComputePipelineCreateInfo {
            flags: vk::PipelineCreateFlags::empty(),
            stage: shader_stage,
            layout: layout.handle(),
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0,
            ..Default::default()
        };
        let pipeline = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_create_info], None)
                .unwrap()[0]
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .debug_utils_loader
                        .set_debug_utils_object_name(
                            device.handle(),
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        VkPipeline {
            pipeline,
            device: device.clone(),
            layout,
            pipeline_type: VkPipelineType::Compute,
            uses_bindless_texture_set: shader.uses_bindless_texture_set,
            sbt: None,
        }
    }

    pub fn new_compute_meta(
        device: &Arc<RawVkDevice>,
        shader: &VkShader,
        name: Option<&str>,
    ) -> Self {
        let mut descriptor_set_layout_keys: [VkDescriptorSetLayoutKey;
            (BINDLESS_TEXTURE_SET_INDEX + 1) as usize] = Default::default();
        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();

        let shader_stage = vk::PipelineShaderStageCreateInfo {
            module: shader.shader_module(),
            p_name: entry_point.as_ptr() as *const c_char,
            stage: shader_type_to_vk(shader.shader_type()),
            ..Default::default()
        };

        let mut dynamic_storage_buffers = [0; 4];
        let mut dynamic_uniform_buffers = [0; 4];
        for (index, shader_set) in &shader.descriptor_set_bindings {
            let set = &mut descriptor_set_layout_keys[*index as usize];
            for binding in shader_set {
                let existing_binding_option = set
                    .bindings
                    .iter_mut()
                    .find(|existing_binding| existing_binding.index == binding.index);
                if let Some(existing_binding) = existing_binding_option {
                    if existing_binding.descriptor_type
                        == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                    {
                        assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                    } else if existing_binding.descriptor_type
                        == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                    {
                        assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                    } else {
                        assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                    }
                    existing_binding.shader_stage |= binding.shader_stage;
                } else {
                    let mut binding_clone = binding.clone();
                    if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                        && dynamic_storage_buffers[*index as usize] + binding_clone.count
                            < device
                                .properties
                                .limits
                                .max_descriptor_set_storage_buffers_dynamic
                    {
                        dynamic_storage_buffers[*index as usize] += binding_clone.count;
                        binding_clone.descriptor_type = vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                    }
                    if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                        && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                            < device
                                .properties
                                .limits
                                .max_descriptor_set_uniform_buffers_dynamic
                    {
                        dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                        binding_clone.descriptor_type = vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                    }
                    set.bindings.push(binding_clone);
                }
            }
        }

        let mut push_constants_ranges = <[Option<VkConstantRange>; 3]>::default();
        if let Some(push_constants_range) = &shader.push_constants_range {
            push_constants_ranges[0] = Some(VkConstantRange {
                offset: push_constants_range.offset,
                size: push_constants_range.size,
                shader_stage: vk::ShaderStageFlags::COMPUTE,
            });
        }

        let mut descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 5] =
            Default::default();
        for (i, set_key) in descriptor_set_layout_keys.iter().enumerate() {
            descriptor_set_layouts[i] = Some(Arc::new(VkDescriptorSetLayout::new(
                &set_key.bindings,
                set_key.flags,
                device,
            )));
        }

        let layout = Arc::new(VkPipelineLayout::new(
            &descriptor_set_layouts,
            &push_constants_ranges,
            device,
        ));

        let pipeline_create_info = vk::ComputePipelineCreateInfo {
            flags: vk::PipelineCreateFlags::empty(),
            stage: shader_stage,
            layout: layout.handle(),
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0,
            ..Default::default()
        };
        let pipeline = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_create_info], None)
                .unwrap()[0]
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .debug_utils_loader
                        .set_debug_utils_object_name(
                            device.handle(),
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        VkPipeline {
            pipeline,
            device: device.clone(),
            layout,
            pipeline_type: VkPipelineType::Compute,
            uses_bindless_texture_set: shader.uses_bindless_texture_set,
            sbt: None,
        }
    }

    /*pub fn new_ray_tracing(
        device: &Arc<RawVkDevice>,
        info: &RayTracingPipelineInfo<VkBackend>,
        shared: &VkShared
    ) -> Self {
        let rt = device.rt.as_ref().unwrap();
        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();

        let mut stages = SmallVec::<[vk::PipelineShaderStageCreateInfo; 4]>::new();
        let mut groups = SmallVec::<[vk::RayTracingShaderGroupCreateInfoKHR; 4]>::new();
        let mut descriptor_set_layouts: [VkDescriptorSetLayoutKey;
            (BINDLESS_TEXTURE_SET_INDEX + 1) as usize] = Default::default();
        let mut push_constants_ranges = <[Option<VkConstantRange>; 3]>::default();

        let mut uses_bindless_texture_set = false;
        let mut dynamic_storage_buffers = [0; 4];
        let mut dynamic_uniform_buffers = [0; 4];

        {
            let shader = info.ray_gen_shader;
            let stage_info = vk::PipelineShaderStageCreateInfo {
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::RAYGEN_KHR,
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                ..Default::default()
            };
            let group_info = vk::RayTracingShaderGroupCreateInfoKHR {
                ty: vk::RayTracingShaderGroupTypeKHR::GENERAL,
                general_shader: stages.len() as u32,
                closest_hit_shader: vk::SHADER_UNUSED_KHR,
                any_hit_shader: vk::SHADER_UNUSED_KHR,
                intersection_shader: vk::SHADER_UNUSED_KHR,
                p_shader_group_capture_replay_handle: std::ptr::null(),
                ..Default::default()
            };
            stages.push(stage_info);
            groups.push(group_info);
            for (index, shader_set) in &shader.descriptor_set_bindings {
                let set = &mut descriptor_set_layouts[*index as usize];
                for binding in shader_set {
                    let existing_binding_option = set
                        .bindings
                        .iter_mut()
                        .find(|existing_binding| existing_binding.index == binding.index);
                    if let Some(existing_binding) = existing_binding_option {
                        if existing_binding.descriptor_type
                            == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                        } else if existing_binding.descriptor_type
                            == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                        } else {
                            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                        }
                        existing_binding.shader_stage |= binding.shader_stage;
                    } else {
                        let mut binding_clone = binding.clone();
                        if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                            && dynamic_storage_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_storage_buffers_dynamic
                        {
                            dynamic_storage_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                        }
                        if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                            && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_uniform_buffers_dynamic
                        {
                            dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                        }
                        set.bindings.push(binding_clone);
                    }
                }
            }
            if let Some(push_constants_range) = &shader.push_constants_range {
                push_constants_ranges[0] = Some(VkConstantRange {
                    offset: push_constants_range.offset,
                    size: push_constants_range.size,
                    shader_stage: vk::ShaderStageFlags::RAYGEN_KHR,
                });
            }
            uses_bindless_texture_set |= shader.uses_bindless_texture_set;
        }

        for shader in info.closest_hit_shaders.iter() {
            let stage_info = vk::PipelineShaderStageCreateInfo {
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                ..Default::default()
            };
            let group_info = vk::RayTracingShaderGroupCreateInfoKHR {
                ty: vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP,
                general_shader: vk::SHADER_UNUSED_KHR,
                closest_hit_shader: stages.len() as u32,
                any_hit_shader: vk::SHADER_UNUSED_KHR,
                intersection_shader: vk::SHADER_UNUSED_KHR,
                p_shader_group_capture_replay_handle: std::ptr::null(),
                ..Default::default()
            };
            stages.push(stage_info);
            groups.push(group_info);
            for (index, shader_set) in &shader.descriptor_set_bindings {
                let set = &mut descriptor_set_layouts[*index as usize];
                for binding in shader_set {
                    let existing_binding_option = set
                        .bindings
                        .iter_mut()
                        .find(|existing_binding| existing_binding.index == binding.index);
                    if let Some(existing_binding) = existing_binding_option {
                        if existing_binding.descriptor_type
                            == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                        } else if existing_binding.descriptor_type
                            == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                        } else {
                            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                        }
                        existing_binding.shader_stage |= binding.shader_stage;
                    } else {
                        let mut binding_clone = binding.clone();
                        if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                            && dynamic_storage_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_storage_buffers_dynamic
                        {
                            dynamic_storage_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                        }
                        if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                            && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_uniform_buffers_dynamic
                        {
                            dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                        }
                        set.bindings.push(binding_clone);
                    }
                }
            }
            if let Some(push_constants_range) = &shader.push_constants_range {
                push_constants_ranges[0] = Some(VkConstantRange {
                    offset: push_constants_range.offset,
                    size: push_constants_range.size,
                    shader_stage: vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                });
            }
            uses_bindless_texture_set |= shader.uses_bindless_texture_set;
        }

        for shader in info.miss_shaders.iter() {
            let stage_info = vk::PipelineShaderStageCreateInfo {
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::MISS_KHR,
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                ..Default::default()
            };
            let group_info = vk::RayTracingShaderGroupCreateInfoKHR {
                ty: vk::RayTracingShaderGroupTypeKHR::GENERAL,
                general_shader: stages.len() as u32,
                closest_hit_shader: vk::SHADER_UNUSED_KHR,
                any_hit_shader: vk::SHADER_UNUSED_KHR,
                intersection_shader: vk::SHADER_UNUSED_KHR,
                p_shader_group_capture_replay_handle: std::ptr::null(),
                ..Default::default()
            };
            stages.push(stage_info);
            groups.push(group_info);
            for (index, shader_set) in &shader.descriptor_set_bindings {
                let set = &mut descriptor_set_layouts[*index as usize];
                for binding in shader_set {
                    let existing_binding_option = set
                        .bindings
                        .iter_mut()
                        .find(|existing_binding| existing_binding.index == binding.index);
                    if let Some(existing_binding) = existing_binding_option {
                        if existing_binding.descriptor_type
                            == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                        } else if existing_binding.descriptor_type
                            == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                        {
                            assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                        } else {
                            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                        }
                        existing_binding.shader_stage |= binding.shader_stage;
                    } else {
                        let mut binding_clone = binding.clone();
                        if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                            && dynamic_storage_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_storage_buffers_dynamic
                        {
                            dynamic_storage_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                        }
                        if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                            && dynamic_uniform_buffers[*index as usize] + binding_clone.count
                                < device
                                    .properties
                                    .limits
                                    .max_descriptor_set_uniform_buffers_dynamic
                        {
                            dynamic_uniform_buffers[*index as usize] += binding_clone.count;
                            binding_clone.descriptor_type =
                                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                        }
                        set.bindings.push(binding_clone);
                    }
                }
            }
            if let Some(push_constants_range) = &shader.push_constants_range {
                push_constants_ranges[0] = Some(VkConstantRange {
                    offset: push_constants_range.offset,
                    size: push_constants_range.size,
                    shader_stage: vk::ShaderStageFlags::MISS_KHR,
                });
            }
            uses_bindless_texture_set |= shader.uses_bindless_texture_set;
        }

        let mut offset = 0u32;
        let mut remapped_push_constant_ranges = <[Option<VkConstantRange>; 3]>::default();
        if let Some(range) = &push_constants_ranges[0] {
            remapped_push_constant_ranges[0] = Some(VkConstantRange {
                offset,
                size: range.size,
                shader_stage: vk::ShaderStageFlags::VERTEX,
            });
            offset += range.size;
        }
        if let Some(range) = &push_constants_ranges[1] {
            remapped_push_constant_ranges[1] = Some(VkConstantRange {
                offset,
                size: range.size,
                shader_stage: vk::ShaderStageFlags::FRAGMENT,
            });
        }

        if uses_bindless_texture_set {
            /*if !device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
                panic!("RT Pipeline is trying to use the bindless texture descriptor set but the Vulkan device does not support descriptor indexing.");
            }

            descriptor_set_layouts[BINDLESS_TEXTURE_SET_INDEX as usize] =
                VkDescriptorSetLayoutKey {
                    bindings: vec![VkDescriptorSetEntryInfo {
                        name: "bindless_textures".to_string(),
                        shader_stage: vk::ShaderStageFlags::VERTEX
                            | vk::ShaderStageFlags::FRAGMENT
                            | vk::ShaderStageFlags::COMPUTE,
                        index: 0,
                        descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                        count: BINDLESS_TEXTURE_COUNT,
                        writable: false,
                        flags: vk::DescriptorBindingFlags::UPDATE_AFTER_BIND_EXT
                            | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING_EXT
                            | vk::DescriptorBindingFlags::PARTIALLY_BOUND_EXT,
                    }],
                    flags: vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT,
                };*/
        }

        let layout = shared.get_pipeline_layout(&VkPipelineLayoutKey {
            descriptor_set_layouts,
            push_constant_ranges: remapped_push_constant_ranges,
        });

        let vk_info = vk::RayTracingPipelineCreateInfoKHR {
            flags: vk::PipelineCreateFlags::empty(),
            stage_count: stages.len() as u32,
            p_stages: stages.as_ptr(),
            group_count: groups.len() as u32,
            p_groups: groups.as_ptr(),
            max_pipeline_ray_recursion_depth: 2,
            p_library_info: std::ptr::null(),
            p_library_interface: std::ptr::null(),
            p_dynamic_state: std::ptr::null(),
            layout: layout.handle(),
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0,
            ..Default::default()
        };
        let pipeline = unsafe {
            rt.rt_pipelines.create_ray_tracing_pipelines(
                vk::DeferredOperationKHR::null(),
                vk::PipelineCache::null(),
                &[vk_info],
                None,
            )
        }
        .unwrap()
        .pop()
        .unwrap();

        // SBT
        let handle_size = rt.rt_pipeline_properties.shader_group_handle_size;
        let handle_alignment = rt.rt_pipeline_properties.shader_group_handle_alignment;
        let handle_stride = align_up_32(handle_size, handle_alignment);
        let group_alignment = rt.rt_pipeline_properties.shader_group_base_alignment as u64;

        let handles = unsafe {
            rt.rt_pipelines.get_ray_tracing_shader_group_handles(
                pipeline,
                0,
                groups.len() as u32,
                handle_size as usize * groups.len(),
            )
        }
        .unwrap();

        let sbt = VkBuffer::new(
            device,
            MemoryUsage::UncachedRAM,
            &BufferInfo {
                size: align_up_32(handle_stride, group_alignment as u32) as u64 * groups.len() as u64,
                usage: BufferUsage::SHADER_BINDING_TABLE,
            },
            None,
            None
        );
        let map = unsafe { sbt.map_unsafe(0, WHOLE_BUFFER, false).unwrap() };

        let mut src_offset = 0u64;
        let mut dst_offset = 0u64;
        let raygen_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt.va().unwrap(),
            stride: align_up_64(handle_stride as u64, group_alignment),
            size: align_up_64(handle_stride as u64, group_alignment),
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                (handles.as_ptr() as *const u8).add(src_offset as usize),
                map.add(dst_offset as usize),
                handle_size as usize,
            );
        }
        src_offset += handle_size as u64;
        dst_offset += handle_stride as u64;

        dst_offset = align_up_64(dst_offset as u64, group_alignment);
        let closest_hit_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt.va().unwrap() + dst_offset,
            stride: handle_stride as u64,
            size: align_up_64(
                info.closest_hit_shaders.len() as u64 * handle_stride as u64,
                group_alignment,
            ),
        };
        for _ in 0..info.closest_hit_shaders.len() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (handles.as_ptr() as *const u8).add(src_offset as usize),
                    map.add(dst_offset as usize),
                    handle_size as usize,
                );
            }
            src_offset += handle_size as u64;
            dst_offset += handle_stride as u64;
        }

        dst_offset = align_up_64(dst_offset as u64, group_alignment);
        let miss_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt.va().unwrap() + dst_offset,
            stride: handle_stride as u64,
            size: align_up_64(
                info.miss_shaders.len() as u64 * handle_stride as u64,
                group_alignment,
            ),
        };
        for _ in 0..info.miss_shaders.len() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (handles.as_ptr() as *const u8).add(src_offset as usize),
                    map.add(dst_offset as usize),
                    handle_size as usize,
                );
            }
            src_offset += handle_size as u64;
            dst_offset += handle_stride as u64;
        }

        unsafe {
            sbt.unmap_unsafe(0, WHOLE_BUFFER, true);
        }

        Self {
            pipeline,
            layout,
            device: device.clone(),
            pipeline_type: VkPipelineType::RayTracing,
            uses_bindless_texture_set,
            sbt: Some(VkShaderBindingTables {
                buffer: sbt,
                raygen_region,
                closest_hit_region,
                miss_region,
            }),
        }
    }*/

    #[inline]
    pub(crate) fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    #[inline]
    pub(crate) fn layout(&self) -> &Arc<VkPipelineLayout> {
        &self.layout
    }

    pub(crate) fn pipeline_type(&self) -> VkPipelineType {
        self.pipeline_type
    }

    #[inline]
    pub(crate) fn uses_bindless_texture_set(&self) -> bool {
        self.uses_bindless_texture_set
    }

    #[inline]
    pub(crate) fn sbt_buffer(&self) -> &VkBuffer {
        &self.sbt.as_ref().unwrap().buffer
    }

    #[inline]
    pub(crate) fn raygen_sbt_region(&self) -> &vk::StridedDeviceAddressRegionKHR {
        &self.sbt.as_ref().unwrap().raygen_region
    }

    #[inline]
    pub(crate) fn closest_hit_sbt_region(&self) -> &vk::StridedDeviceAddressRegionKHR {
        &self.sbt.as_ref().unwrap().closest_hit_region
    }

    #[inline]
    pub(crate) fn miss_sbt_region(&self) -> &vk::StridedDeviceAddressRegionKHR {
        &self.sbt.as_ref().unwrap().miss_region
    }
}

impl Drop for VkPipeline {
    fn drop(&mut self) {
        unsafe {
            let vk_device = &self.device.device;
            vk_device.destroy_pipeline(self.pipeline, None);
        }
    }
}

impl ComputePipeline for VkPipeline {
    fn binding_info(&self, set: BindingFrequency, slot: u32) -> Option<BindingInfo> {
        self.layout
            .descriptor_set_layouts
            .get(set as usize)
            .unwrap()
            .as_ref()
            .and_then(|layout| layout.binding(slot))
            .map(|i| BindingInfo {
                name: i.name.as_str(),
                binding_type: match i.descriptor_type {
                    vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                    | vk::DescriptorType::STORAGE_BUFFER => BindingType::StorageTexture,
                    vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                    | vk::DescriptorType::UNIFORM_BUFFER => BindingType::ConstantBuffer,
                    vk::DescriptorType::STORAGE_IMAGE => BindingType::StorageTexture,
                    vk::DescriptorType::SAMPLED_IMAGE => BindingType::SampledTexture,
                    vk::DescriptorType::SAMPLER => BindingType::Sampler,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER => BindingType::TextureAndSampler,
                    _ => unreachable!(),
                },
            })
    }
}

pub(crate) struct VkPipelineLayout {
    device: Arc<RawVkDevice>,
    layout: vk::PipelineLayout,
    descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 5],
    push_constant_ranges: [Option<VkConstantRange>; 3],
}

impl VkPipelineLayout {
    pub fn new(
        descriptor_set_layouts: &[Option<Arc<VkDescriptorSetLayout>>; 5],
        push_constant_ranges: &[Option<VkConstantRange>; 3],
        device: &Arc<RawVkDevice>,
    ) -> Self {
        let layouts: Vec<vk::DescriptorSetLayout> = descriptor_set_layouts
            .iter()
            .filter(|descriptor_set_layout| descriptor_set_layout.is_some())
            .map(|descriptor_set_layout| descriptor_set_layout.as_ref().unwrap().handle())
            .collect();

        let ranges: Vec<vk::PushConstantRange> = push_constant_ranges
            .iter()
            .filter(|r| r.is_some())
            .map(|r| {
                let r = r.as_ref().unwrap();
                vk::PushConstantRange {
                    stage_flags: r.shader_stage,
                    offset: r.offset,
                    size: r.size,
                }
            })
            .collect();

        let info = vk::PipelineLayoutCreateInfo {
            p_set_layouts: layouts.as_ptr(),
            set_layout_count: layouts.len() as u32,
            p_push_constant_ranges: ranges.as_ptr(),
            push_constant_range_count: ranges.len() as u32,
            ..Default::default()
        };

        unsafe {
            if info.push_constant_range_count != 0 && (*(info.p_push_constant_ranges)).size == 0 {
                panic!("aaaa");
            }
        }

        let layout = unsafe { device.create_pipeline_layout(&info, None) }.unwrap();
        Self {
            device: device.clone(),
            layout,
            descriptor_set_layouts: descriptor_set_layouts.clone(),
            push_constant_ranges: push_constant_ranges.clone(),
        }
    }

    #[inline]
    pub(crate) fn handle(&self) -> vk::PipelineLayout {
        self.layout
    }

    #[inline]
    pub(crate) fn descriptor_set_layout(&self, index: u32) -> Option<&Arc<VkDescriptorSetLayout>> {
        self.descriptor_set_layouts[index as usize].as_ref()
    }

    pub(crate) fn push_constant_range(&self, shader_type: ShaderType) -> Option<&VkConstantRange> {
        match shader_type {
            ShaderType::VertexShader => self.push_constant_ranges[0].as_ref(),
            ShaderType::FragmentShader => self.push_constant_ranges[1].as_ref(),
            ShaderType::ComputeShader => self.push_constant_ranges[0].as_ref(),
            ShaderType::RayGen => self.push_constant_ranges[0].as_ref(),
            ShaderType::RayClosestHit => self.push_constant_ranges[1].as_ref(),
            ShaderType::RayMiss => self.push_constant_ranges[2].as_ref(),
            _ => None,
        }
    }
}

impl Drop for VkPipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
