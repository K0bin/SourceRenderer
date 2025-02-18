use js_sys::{wasm_bindgen::{self, prelude::Closure, JsCast, JsValue}, Array};
use smallvec::{SmallVec, smallvec};
use sourcerenderer_core::{align_up_32, gpu::{self, Texture as _, TextureLayout}};
use web_sys::{GpuAdapter, GpuDevice, GpuQueue, GpuTexelCopyTextureInfo, GpuTexelCopyBufferLayout, GpuExtent3dDict};

use crate::{WebGPUBackend, WebGPUBuffer, WebGPUComputePipeline, WebGPUFence, WebGPUGraphicsPipeline, WebGPUHeap, WebGPUQueue, WebGPUSampler, WebGPUShader, WebGPUShared, WebGPUTexture, WebGPUTextureView};

pub struct WebGPUDevice {
    device: GpuDevice,
    shared: WebGPUShared,
    memory_infos: [gpu::MemoryTypeInfo; 1],
    queue: WebGPUQueue
}

unsafe impl Send for WebGPUDevice {}
unsafe impl Sync for WebGPUDevice {}

impl WebGPUDevice {
    pub fn new(device: GpuDevice, debug: bool) -> Self {
        let memory_infos: [gpu::MemoryTypeInfo; 1] = [
            gpu::MemoryTypeInfo {
                is_cached: true,
                is_coherent: false,
                is_cpu_accessible: true,
                memory_index: 0,
                memory_kind: gpu::MemoryKind::VRAM
            }
        ];

        let shared = WebGPUShared::new(&device);
        let queue = WebGPUQueue::new(&device);

        if debug {
            log::info!("Initializing device with error callback.");
            let callback_closure = Closure::wrap(Box::new(move |event: web_sys::Event| { Self::on_uncaptured_error(event); }) as Box<dyn FnMut(_)>);
            device.add_event_listener_with_callback("uncapturederror", callback_closure.as_ref().unchecked_ref()).unwrap();
            std::mem::forget(callback_closure);
        }

        Self {
            device,
            shared,
            memory_infos,
            queue
        }
    }

    pub fn handle(&self) -> &GpuDevice {
        &self.device
    }

    fn on_uncaptured_error(event: web_sys::Event) {
        let webgpu_error = event.dyn_into::<web_sys::GpuUncapturedErrorEvent>().unwrap();
        log::error!("Uncaptured WebGPU error: {}", webgpu_error.error().message())
    }
}

impl Drop for WebGPUDevice {
    fn drop(&mut self) {
        self.device.destroy();
    }
}

impl gpu::Device<WebGPUBackend> for WebGPUDevice {
    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, memory_type_index: u32, name: Option<&str>) -> Result<WebGPUBuffer, gpu::OutOfMemoryError> {
        let mem = &self.memory_infos[memory_type_index as usize];
        WebGPUBuffer::new(&self.device, info, mem.is_cpu_accessible, name).map_err(|_e| gpu::OutOfMemoryError {})
    }


    unsafe fn create_texture(&self, info: &gpu::TextureInfo, _memory_type_index: u32, name: Option<&str>) -> Result<WebGPUTexture, gpu::OutOfMemoryError> {
        WebGPUTexture::new(&self.device, info, name).map_err(|_e| gpu::OutOfMemoryError {})
    }

    unsafe fn create_shader(&self, shader: &gpu::PackedShader, name: Option<&str>) -> WebGPUShader {
        WebGPUShader::new(&self.device, shader, name)
    }

    unsafe fn create_texture_view(&self, texture: &WebGPUTexture, info: &gpu::TextureViewInfo, name: Option<&str>) -> WebGPUTextureView {
        WebGPUTextureView::new(&self.device, texture, info, name).unwrap()
    }

    unsafe fn create_compute_pipeline(&self, shader: &WebGPUShader, name: Option<&str>) -> WebGPUComputePipeline {
        WebGPUComputePipeline::new(&self.device, shader, &self.shared, name).unwrap()
    }

    unsafe fn create_sampler(&self, info: &gpu::SamplerInfo) -> WebGPUSampler {
        WebGPUSampler::new(&self.device, info, None).unwrap()
    }

    unsafe fn create_graphics_pipeline(&self, info: &gpu::GraphicsPipelineInfo<WebGPUBackend>, name: Option<&str>) -> WebGPUGraphicsPipeline {
        WebGPUGraphicsPipeline::new(&self.device, info, &self.shared, name).unwrap()
    }

    unsafe fn wait_for_idle(&self) {}

    unsafe fn create_fence(&self, _is_cpu_accessible: bool) -> WebGPUFence {
        WebGPUFence::new(&self.device)
    }

    unsafe fn memory_infos(&self) -> Vec<gpu::MemoryInfo> {
        /*
            TODO: Implement rudimentary memory tracking by having a fixed number and change it in WebGPUTexture, WebGPUBuffer and WebGPUHeap.
            Increase it in the constructor and decrease it in the destructor.
         */
        vec![gpu::MemoryInfo {
            available: (u32::MAX as u64) / 3u64, total: (u32::MAX as u64) / 3u64, memory_kind: gpu::MemoryKind::VRAM
        }]
    }

    unsafe fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_infos
    }

    unsafe fn create_heap(&self, memory_type_index: u32, size: u64) -> Result<WebGPUHeap, gpu::OutOfMemoryError> {
        let mem = &self.memory_infos[memory_type_index as usize];
        Ok(WebGPUHeap::new(&self.device, memory_type_index, size, mem.is_cpu_accessible))
    }

    unsafe fn get_buffer_heap_info(&self, info: &gpu::BufferInfo) -> gpu::ResourceHeapInfo {
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: gpu::DedicatedAllocationPreference::PreferDedicated,
            memory_type_mask: 1,
            alignment: 4,
            size: info.size,
        }
    }

    unsafe fn get_texture_heap_info(&self, info: &gpu::TextureInfo) -> gpu::ResourceHeapInfo {
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: gpu::DedicatedAllocationPreference::PreferDedicated,
            memory_type_mask: 1,
            alignment: 4,
            size: (info.width * info.height * info.array_length * (4 * 4)) as u64, // TODO: We just assume RGBA Float32, make this take the format into account properly
        }
    }

    unsafe fn insert_texture_into_bindless_heap(&self, _slot: u32, _texture: &WebGPUTextureView) {
        panic!("WebGPU does not support bindless textures");
    }

    fn graphics_queue(&self) -> &WebGPUQueue {
        &self.queue
    }

    fn compute_queue(&self) -> Option<&WebGPUQueue> {
        None
    }

    fn transfer_queue(&self) -> Option<&WebGPUQueue> {
        None
    }

    fn supports_bindless(&self) -> bool {
        false
    }

    fn supports_ray_tracing(&self) -> bool {
        false
    }

    fn supports_indirect(&self) -> bool {
        true
    }

    fn supports_min_max_filter(&self) -> bool {
        false
    }

    fn supports_barycentrics(&self) -> bool {
        false
    }

    unsafe fn get_bottom_level_acceleration_structure_size(&self, _info: &gpu::BottomLevelAccelerationStructureInfo<WebGPUBackend>) -> gpu::AccelerationStructureSizes {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn get_top_level_acceleration_structure_size(&self, _info: &gpu::TopLevelAccelerationStructureInfo<WebGPUBackend>) -> gpu::AccelerationStructureSizes {
        panic!("WebGPU does not support bindless")
    }

    fn get_top_level_instances_buffer_size(&self, _instances: &[gpu::AccelerationStructureInstance<WebGPUBackend>]) -> u64 {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn get_raytracing_pipeline_sbt_buffer_size(&self, _info: &gpu::RayTracingPipelineInfo<WebGPUBackend>) -> u64 {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn create_raytracing_pipeline(&self, _info: &gpu::RayTracingPipelineInfo<WebGPUBackend>, _sbt_buffer: &WebGPUBuffer, _sbt_buffer_offset: u64, _name: Option<&str>) -> () {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn transition_texture(&self, _dst: &WebGPUTexture, _transition: &gpu::CPUTextureTransition<'_, WebGPUBackend>) {}

    unsafe fn copy_to_texture(&self, src: *const std::ffi::c_void, dst: &WebGPUTexture, _texture_layout: TextureLayout, region: &gpu::MemoryTextureCopyRegion) {
        let src_info = GpuTexelCopyBufferLayout::new();

        let format = dst.info().format;
        let row_pitch = if region.row_pitch != 0 {
            region.row_pitch
        } else {
            (align_up_32(region.texture_extent.x, format.block_size().x) / format.block_size().x * format.element_size()) as u64
        };
        let slice_pitch = if region.slice_pitch != 0 {
            region.slice_pitch
        } else {
            (align_up_32(region.texture_extent.y, format.block_size().y) / format.block_size().y) as u64 * row_pitch
        };
        assert_eq!(slice_pitch % row_pitch, 0);

        src_info.set_bytes_per_row(row_pitch as u32);
        src_info.set_rows_per_image((slice_pitch / row_pitch) as u32);
        let dst_info = GpuTexelCopyTextureInfo::new(dst.handle());
        dst_info.set_mip_level(region.texture_subresource.mip_level);
        let origin = Array::new_with_length(3);
        origin.set(0, JsValue::from(region.texture_offset.x as f64));
        origin.set(1, JsValue::from(region.texture_offset.y as f64));
        let copy_size = GpuExtent3dDict::new(region.texture_extent.x);
        copy_size.set_height(region.texture_extent.y);
        assert!(dst.info().array_length == 0 || dst.info().dimension != gpu::TextureDimension::Dim3D);
        if dst.info().dimension == gpu::TextureDimension::Dim3D {
            assert_eq!(region.texture_subresource.array_layer, 0);
            copy_size.set_depth_or_array_layers(region.texture_extent.z);
            origin.set(2, JsValue::from(region.texture_offset.z as f64));
        } else {
            assert_eq!(region.texture_extent.z, 1);
            assert_eq!(region.texture_offset.z, 0);
            copy_size.set_depth_or_array_layers(1);
            origin.set(2, JsValue::from(region.texture_subresource.array_layer as f64));
        }
        dst_info.set_origin(&origin);

        let queue = self.queue.handle();
        let data_len = slice_pitch as usize * dst.info().depth as usize;
        let slice = unsafe { std::slice::from_raw_parts(src as *const u8, data_len) };
        queue.write_texture_with_u8_slice_and_gpu_extent_3d_dict(&dst_info, slice, &src_info, &copy_size).unwrap();
    }
}
