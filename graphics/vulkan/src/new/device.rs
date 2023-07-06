use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use ash::vk;
use sourcerenderer_core::gpu::*;

use super::*;

use crate::queue::VkQueueInfo; // the RawVkDevice uses this, so we cannot use the new one

pub struct VkDevice {
    device: Arc<RawVkDevice>,
    graphics_queue: VkQueue,
    compute_queue: Option<VkQueue>,
    transfer_queue: Option<VkQueue>,
    shared: Arc<VkShared>,
    query_count: AtomicU64,
}

impl VkDevice {
    pub fn new(
        device: ash::Device,
        instance: &Arc<RawVkInstance>,
        physical_device: vk::PhysicalDevice,
        graphics_queue_info: VkQueueInfo,
        compute_queue_info: Option<VkQueueInfo>,
        transfer_queue_info: Option<VkQueueInfo>,
        features: VkFeatures,
        max_surface_image_count: u32,
    ) -> Self {
        let mut vma_flags = vma_sys::VmaAllocatorCreateFlags::default();
        if features.intersects(VkFeatures::DEDICATED_ALLOCATION) {
            vma_flags |= vma_sys::VmaAllocatorCreateFlagBits_VMA_ALLOCATOR_CREATE_KHR_DEDICATED_ALLOCATION_BIT as u32;
        }
        if features.intersects(VkFeatures::RAY_TRACING) {
            vma_flags |=
                vma_sys::VmaAllocatorCreateFlagBits_VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT
                    as u32;
        }

        let allocator = unsafe {
            unsafe extern "system" fn get_instance_proc_addr_stub(
                _instance: ash::vk::Instance,
                _p_name: *const ::std::os::raw::c_char,
            ) -> ash::vk::PFN_vkVoidFunction {
                panic!("VMA_DYNAMIC_VULKAN_FUNCTIONS is unsupported")
            }

            unsafe extern "system" fn get_get_device_proc_stub(
                _device: ash::vk::Device,
                _p_name: *const ::std::os::raw::c_char,
            ) -> ash::vk::PFN_vkVoidFunction {
                panic!("VMA_DYNAMIC_VULKAN_FUNCTIONS is unsupported")
            }

            let routed_functions = vma_sys::VmaVulkanFunctions {
                vkGetInstanceProcAddr: None,
                vkGetDeviceProcAddr: None,
                vkGetPhysicalDeviceProperties: Some(
                    instance.fp_v1_0().get_physical_device_properties,
                ),
                vkGetPhysicalDeviceMemoryProperties: Some(
                    instance.fp_v1_0().get_physical_device_memory_properties,
                ),
                vkAllocateMemory: Some(device.fp_v1_0().allocate_memory),
                vkFreeMemory: Some(device.fp_v1_0().free_memory),
                vkMapMemory: Some(device.fp_v1_0().map_memory),
                vkUnmapMemory: Some(device.fp_v1_0().unmap_memory),
                vkFlushMappedMemoryRanges: Some(device.fp_v1_0().flush_mapped_memory_ranges),
                vkInvalidateMappedMemoryRanges: Some(
                    device.fp_v1_0().invalidate_mapped_memory_ranges,
                ),
                vkBindBufferMemory: Some(device.fp_v1_0().bind_buffer_memory),
                vkBindImageMemory: Some(device.fp_v1_0().bind_image_memory),
                vkGetBufferMemoryRequirements: Some(
                    device.fp_v1_0().get_buffer_memory_requirements,
                ),
                vkGetImageMemoryRequirements: Some(device.fp_v1_0().get_image_memory_requirements),
                vkCreateBuffer: Some(device.fp_v1_0().create_buffer),
                vkDestroyBuffer: Some(device.fp_v1_0().destroy_buffer),
                vkCreateImage: Some(device.fp_v1_0().create_image),
                vkDestroyImage: Some(device.fp_v1_0().destroy_image),
                vkCmdCopyBuffer: Some(device.fp_v1_0().cmd_copy_buffer),
                vkGetBufferMemoryRequirements2KHR: Some(
                    device.fp_v1_1().get_buffer_memory_requirements2,
                ),
                vkGetImageMemoryRequirements2KHR: Some(
                    device.fp_v1_1().get_image_memory_requirements2,
                ),
                vkBindBufferMemory2KHR: Some(device.fp_v1_1().bind_buffer_memory2),
                vkBindImageMemory2KHR: Some(device.fp_v1_1().bind_image_memory2),
                vkGetPhysicalDeviceMemoryProperties2KHR: Some(
                    instance.fp_v1_1().get_physical_device_memory_properties2,
                ),
                vkGetDeviceBufferMemoryRequirements: None, // device.fp_v1_3().get_device_buffer_memory_requirements,
                vkGetDeviceImageMemoryRequirements: None, // device.fp_v1_3().get_device_image_memory_requirements,
            };

            let vma_create_info = vma_sys::VmaAllocatorCreateInfo {
                flags: vma_flags,
                physicalDevice: physical_device,
                device: device.handle(),
                preferredLargeHeapBlockSize: 0,
                pAllocationCallbacks: std::ptr::null(),
                pDeviceMemoryCallbacks: std::ptr::null(),
                pHeapSizeLimit: std::ptr::null(),
                pVulkanFunctions: &routed_functions,
                instance: instance.handle(),
                vulkanApiVersion: vk::API_VERSION_1_1,
                pTypeExternalMemoryHandleTypes: std::ptr::null(),
            };

            let mut allocator: vma_sys::VmaAllocator = std::ptr::null_mut();
            assert_eq!(
                vma_sys::vmaCreateAllocator(&vma_create_info, &mut allocator),
                vk::Result::SUCCESS
            );
            allocator
        };

        let raw_graphics_queue = unsafe {
            device.get_device_queue(
                graphics_queue_info.queue_family_index as u32,
                graphics_queue_info.queue_index as u32,
            )
        };
        let raw_compute_queue = compute_queue_info.map(|info| unsafe {
            device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32)
        });
        let raw_transfer_queue = transfer_queue_info.map(|info| unsafe {
            device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32)
        });

        let raw = Arc::new(RawVkDevice::new(
            device,
            allocator,
            physical_device,
            instance.clone(),
            features,
            graphics_queue_info,
            compute_queue_info,
            transfer_queue_info,
            raw_graphics_queue,
            raw_compute_queue,
            raw_transfer_queue,
        ));

        let shared = Arc::new(VkShared::new(&raw));

        let graphics_queue = {
            VkQueue::new(
                graphics_queue_info,
                VkQueueType::Graphics,
                &raw,
                &shared
            )
        };

        let compute_queue = compute_queue_info.map(|info| {
            VkQueue::new(
                info,
                VkQueueType::Compute,
                &raw,
                &shared
            )
        });

        let transfer_queue = transfer_queue_info.map(|info| {
            VkQueue::new(
                info,
                VkQueueType::Transfer,
                &raw,
                &shared
            )
        });

        VkDevice {
            device: raw,
            graphics_queue,
            compute_queue,
            transfer_queue,
            shared,
            query_count: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn inner(&self) -> &Arc<RawVkDevice> {
        &self.device
    }

    #[inline]
    pub fn graphics_queue(&self) -> &VkQueue {
        &self.graphics_queue
    }

    #[inline]
    pub fn compute_queue(&self) -> Option<&VkQueue> {
        self.compute_queue.as_ref()
    }

    #[inline]
    pub fn transfer_queue(&self) -> Option<&VkQueue> {
        self.transfer_queue.as_ref()
    }
}

impl Device<VkBackend> for VkDevice {
    unsafe fn create_buffer(
        &self,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        name: Option<&str>,
    ) -> VkBuffer {
        VkBuffer::new(
            &self.device,
            memory_usage,
            info,
            None,
            name
        )
    }

    unsafe fn create_shader(
        &self,
        shader_type: ShaderType,
        bytecode: &[u8],
        name: Option<&str>,
    ) -> VkShader {
    VkShader::new(&self.device, shader_type, bytecode, name)
    }

    unsafe fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> VkTexture {
        VkTexture::new(&self.device, info, name)
    }

    unsafe fn create_texture_view(
        &self,
        texture: &VkTexture,
        info: &TextureViewInfo,
        name: Option<&str>,
    ) -> VkTextureView {
        VkTextureView::new(&self.device, texture, info, name)
    }

    unsafe fn create_sampler(&self, info: &SamplerInfo) -> VkSampler {
        VkSampler::new(&self.device, info)
    }

    unsafe fn create_compute_pipeline(
        &self,
        shader: &VkShader,
        name: Option<&str>,
    ) -> VkPipeline {
        VkPipeline::new_compute(
            &self.device,
            shader,
            self.shared.as_ref(),
            name,
        )
    }

    unsafe fn wait_for_idle(&self) {
        self.device.wait_for_idle();
    }

    unsafe fn create_graphics_pipeline(
        &self,
        info: &GraphicsPipelineInfo<VkBackend>,
        renderpass_info: &RenderPassInfo,
        subpass: u32,
        name: Option<&str>,
    ) -> VkPipeline {
        let shared = &self.shared;
        let rp_info = VkRenderPassInfo {
            attachments: renderpass_info
                .attachments
                .iter()
                .map(|a| VkAttachmentInfo {
                    format: a.format,
                    samples: a.samples,
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::DontCare,
                    stencil_load_op: LoadOp::DontCare,
                    stencil_store_op: StoreOp::DontCare,
                })
                .collect(),
            subpasses: renderpass_info
                .subpasses
                .iter()
                .map(|sp| VkSubpassInfo {
                    input_attachments: sp.input_attachments.iter().cloned().collect(),
                    output_color_attachments: sp.output_color_attachments.iter().cloned().collect(),
                    depth_stencil_attachment: sp.depth_stencil_attachment.clone(),
                })
                .collect(),
        };
        let rp = shared.get_render_pass(rp_info);
        let vk_info = VkGraphicsPipelineInfo {
            info,
            render_pass: &rp,
            sub_pass: subpass,
        };
        VkPipeline::new_graphics(
            &self.device,
            &vk_info,
            shared,
            name,
        )
    }

    unsafe fn create_fence(&self) -> VkTimelineSemaphore {
        VkTimelineSemaphore::new(&self.device)
    }

    fn graphics_queue(&self) -> &VkQueue {
        &self.graphics_queue
    }

    fn supports_bindless(&self) -> bool {
        false
        /*self.device
            .features
            .contains(VkFeatures::DESCRIPTOR_INDEXING)*/
    }

    fn supports_ray_tracing(&self) -> bool {
        self.device.features.contains(VkFeatures::RAY_TRACING)
    }

    fn supports_indirect(&self) -> bool {
        self.device.features.contains(VkFeatures::ADVANCED_INDIRECT)
    }

    fn supports_min_max_filter(&self) -> bool {
        self.device.features.contains(VkFeatures::MIN_MAX_FILTER)
    }

    unsafe fn get_bottom_level_acceleration_structure_size(
        &self,
        info: &BottomLevelAccelerationStructureInfo<VkBackend>,
    ) -> AccelerationStructureSizes {
        unimplemented!()
        //VkAccelerationStructure::bottom_level_size(&self.device, info)
    }

    unsafe fn get_top_level_acceleration_structure_size(
        &self,
        info: &TopLevelAccelerationStructureInfo<VkBackend>,
    ) -> AccelerationStructureSizes {
        unimplemented!()
        //VkAccelerationStructure::top_level_size(&self.device, info)
    }

    unsafe fn create_raytracing_pipeline(
        &self,
        info: &RayTracingPipelineInfo<VkBackend>,
    ) -> VkPipeline {
        VkPipeline::new_ray_tracing(
            &self.device,
            info,
            &self.shared,
        )
    }

    fn supports_barycentrics(&self) -> bool {
        self.device.features.contains(VkFeatures::BARYCENTRICS)
    }

}

impl Drop for VkDevice {
    fn drop(&mut self) {
        unsafe {
            self.wait_for_idle();
        }
    }
}

#[derive(Debug)]
pub(crate) struct VulkanMemoryFlags {
    pub(crate) preferred: vk::MemoryPropertyFlags,
    pub(crate) required: vk::MemoryPropertyFlags,
}

pub(crate) fn memory_usage_to_vma(memory_usage: MemoryUsage) -> VulkanMemoryFlags {
    use vk::MemoryPropertyFlags as VkMem;
    match memory_usage {
        MemoryUsage::CachedRAM => VulkanMemoryFlags {
            preferred: VkMem::HOST_COHERENT,
            required: VkMem::HOST_VISIBLE | VkMem::HOST_CACHED,
        },
        MemoryUsage::VRAM => VulkanMemoryFlags {
            preferred: VkMem::DEVICE_LOCAL,
            required: VkMem::empty(),
        },
        MemoryUsage::UncachedRAM => VulkanMemoryFlags {
            preferred: VkMem::HOST_COHERENT,
            required: VkMem::HOST_VISIBLE,
        },
        MemoryUsage::MappableVRAM => VulkanMemoryFlags {
            preferred: VkMem::DEVICE_LOCAL | VkMem::HOST_COHERENT, // Fall back to uncached RAM
            required: VkMem::HOST_VISIBLE,
        },
    }
}
