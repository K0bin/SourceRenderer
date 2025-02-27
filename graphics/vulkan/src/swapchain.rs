use core::panic;
use std::{
    cmp::{
        max,
        min,
    }, fmt::Debug, hash::Hash, sync::{
        Arc, Condvar
    }
};

use ash::{
    khr::swapchain::Device as SwapchainDevice,
    vk,
};
use smallvec::SmallVec;
use sourcerenderer_core::{
    gpu::*,
    Matrix4,
    Vec3,
    EulerRot
};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u32)]
pub enum VkSwapchainState {
    Okay = 0,
    Suboptimal = 1,
    OutOfDate = 2,
}

#[derive(Debug)]
pub struct VkBackbufferIndices {
    pub(crate) texture_index: u32,
    pub(crate) acquire_semaphore_index: u32,
    pub(crate) present_semaphore_index: u32
}

impl Backbuffer for VkBackbufferIndices {
    fn key(&self) -> u64 {
        (self.texture_index as u64) & 255u64
        | (((self.acquire_semaphore_index as u64) & 255u64) << 8)
        | (((self.present_semaphore_index as u64) & 255u64) << 16)
    }
}

pub struct VkSwapchainInner {
}

pub struct VkSwapchain {
    textures: SmallVec<[VkTexture; 5]>,
    acquire_semaphore_counter: u64,
    present_semaphore_counter: u64,
    state: VkSwapchainState,
    swapchain: vk::SwapchainKHR,
    transform_matrix: Matrix4,
    acquire_semaphores: SmallVec<[VkBinarySemaphore; 5]>,
    present_semaphores: SmallVec<[VkBinarySemaphore; 5]>,
    swapchain_device: SwapchainDevice,
    _instance: Arc<RawVkInstance>,
    surface: VkSurface,
    device: Arc<RawVkDevice>,
    vsync: bool,
    cond_var: Condvar,
}

impl VkSwapchain {
    fn create_swapchain_and_textures(
        device: &Arc<RawVkDevice>,
        swapchain_device: &SwapchainDevice,
        surface: &VkSurface,
        width: u32,
        height: u32,
        vsync: bool,
        old_swapchain: Option<&vk::SwapchainKHR>
    ) -> (vk::SwapchainKHR, SmallVec<[VkTexture; 5]>, Matrix4, u32) {
        unsafe {
            let physical_device = device.physical_device;
            let present_modes = match surface.get_present_modes(&physical_device) {
                Ok(present_modes) => present_modes,
                Err(e) => match e {
                    vk::Result::ERROR_SURFACE_LOST_KHR => {
                        panic!("Vulkan surface lost")
                    }
                    _ => {
                        panic!("Could not get surface modes: {:?}", e);
                    }
                },
            };
            let present_mode = VkSwapchain::pick_present_mode(vsync, &present_modes);

            let capabilities = match surface.get_capabilities(&physical_device) {
                Ok(capabilities) => capabilities,
                Err(e) => match e {
                    vk::Result::ERROR_SURFACE_LOST_KHR => {
                        panic!("Vulkan surface lost");
                    }
                    _ => {
                        panic!("Could not get surface capabilities: {:?}", e);
                    }
                },
            };
            let formats = match surface.get_formats(&physical_device) {
                Ok(format) => format,
                Err(e) => match e {
                    vk::Result::ERROR_SURFACE_LOST_KHR => {
                        panic!("Vulkan surface lost");
                    }
                    _ => {
                        panic!("Could not get surface formats: {:?}", e);
                    }
                },
            };
            let format = VkSwapchain::pick_format(&formats);

            let extent = VkSwapchain::pick_extent(&capabilities, width, height);

            if extent.width == 0 || extent.height == 0 {
                panic!("Zero extents");
            }

            if !capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                || !capabilities
                    .supported_usage_flags
                    .contains(vk::ImageUsageFlags::STORAGE)
            {
                panic!("Rendering to the surface is not supported.");
            }

            let (_matrix, _transform) = match capabilities.current_transform {
                vk::SurfaceTransformFlagsKHR::ROTATE_90 => (
                    Matrix4::from_euler(EulerRot::XYZ, 0f32, 0f32, -std::f32::consts::FRAC_PI_2),
                    vk::SurfaceTransformFlagsKHR::ROTATE_90,
                ),
                vk::SurfaceTransformFlagsKHR::ROTATE_180 => (
                    Matrix4::from_euler(EulerRot::XYZ, 0f32, 0f32, -std::f32::consts::PI),
                    vk::SurfaceTransformFlagsKHR::ROTATE_180,
                ),
                vk::SurfaceTransformFlagsKHR::ROTATE_270 => (
                    Matrix4::from_euler(EulerRot::XYZ, 0f32, 0f32, -std::f32::consts::FRAC_PI_2 * 3f32),
                    vk::SurfaceTransformFlagsKHR::ROTATE_270,
                ),
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR => (
                    Matrix4::from_scale(Vec3::new(-1f32, 1f32, 1f32)),
                    vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR,
                ),
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90 => (
                    Matrix4::from_scale(Vec3::new(-1f32, 1f32, 1f32))
                        * Matrix4::from_euler(EulerRot::XYZ, 0f32, 0f32, -std::f32::consts::FRAC_PI_2),
                    vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90,
                ),
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180 => (
                    Matrix4::from_scale(Vec3::new(-1f32, 1f32, 1f32))
                        * Matrix4::from_euler(EulerRot::XYZ, 0f32, 0f32, -std::f32::consts::PI),
                    vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180,
                ),
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270 => (
                    Matrix4::from_scale(Vec3::new(-1f32, 1f32, 1f32))
                        * Matrix4::from_euler(EulerRot::XYZ,
                            0f32,
                            0f32,
                            -std::f32::consts::FRAC_PI_2 * 3f32,
                        ),
                    vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270,
                ),
                _ => (Matrix4::IDENTITY, vk::SurfaceTransformFlagsKHR::IDENTITY),
            };

            // TODO: Rendering is broken with actual pretransform
            let matrix = Matrix4::IDENTITY;
            let transform = vk::SurfaceTransformFlagsKHR::IDENTITY;

            let image_count = VkSwapchain::pick_image_count(&capabilities, 3);

            let swapchain = {
                let surface_handle = surface.surface_handle();

                let swapchain_create_info = vk::SwapchainCreateInfoKHR {
                    surface: surface_handle,
                    min_image_count: image_count,
                    image_format: format.format,
                    image_color_space: format.color_space,
                    image_extent: extent,
                    image_array_layers: 1,
                    image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::TRANSFER_DST,
                    present_mode,
                    image_sharing_mode: vk::SharingMode::EXCLUSIVE,
                    pre_transform: transform,
                    composite_alpha: if capabilities
                        .supported_composite_alpha
                        .contains(vk::CompositeAlphaFlagsKHR::OPAQUE)
                    {
                        vk::CompositeAlphaFlagsKHR::OPAQUE
                    } else {
                        vk::CompositeAlphaFlagsKHR::INHERIT
                    },
                    clipped: vk::TRUE,
                    old_swapchain: old_swapchain.copied().unwrap_or(vk::SwapchainKHR::default()),
                    ..Default::default()
                };

                swapchain_device
                    .create_swapchain(&swapchain_create_info, None)
                    .map_err(|e| match e {
                        vk::Result::ERROR_SURFACE_LOST_KHR => {
                            panic!("Vulkan surface lost");
                        }
                        _ => {
                            panic!(
                                "Creating swapchain failed {:?}, old swapchain is: {:?}",
                                e, swapchain_create_info.old_swapchain
                            );
                        }
                    }).unwrap()
            };

            let swapchain_images = swapchain_device.get_swapchain_images(swapchain).unwrap();
            let textures: SmallVec<[VkTexture; 5]> = swapchain_images
                .iter()
                .map(|image| {
                    VkTexture::from_image(
                        device,
                        *image,
                        TextureInfo {
                            dimension: TextureDimension::Dim2D,
                            format: surface_vk_format_to_core(format.format),
                            width: extent.width,
                            height: extent.height,
                            array_length: 1u32,
                            mip_levels: 1u32,
                            depth: 1u32,
                            samples: SampleCount::Samples1,
                            usage: TextureUsage::RENDER_TARGET
                                | TextureUsage::COPY_DST
                                | TextureUsage::BLIT_DST,
                            supports_srgb: false,
                        },
                    )
                })
                .collect();

            (swapchain, textures, matrix, capabilities.max_image_count)
        }
    }

    pub fn new(
        vsync: bool,
        width: u32,
        height: u32,
        device: &Arc<RawVkDevice>,
        surface: VkSurface,
    ) -> Result<Self, SwapchainError> {
        let swapchain_device = SwapchainDevice::new(&device.instance.instance, &device.device);
        let (swapchain, textures, matrix, max_image_count) = Self::create_swapchain_and_textures(
            device, &swapchain_device,
            &surface,
            width,
            height,
            vsync,
            None
        );

        let acquire_semaphores: SmallVec<[VkBinarySemaphore; 5]> = (0..max_image_count)
            .map(|_i| VkBinarySemaphore::new(device))
            .collect();

        let present_semaphores: SmallVec<[VkBinarySemaphore; 5]> = (0..max_image_count)
            .map(|_i| VkBinarySemaphore::new(device))
            .collect();

        Ok(VkSwapchain {
            textures,
            acquire_semaphore_counter: 0u64,
            present_semaphore_counter: 0u64,
            state: VkSwapchainState::Okay,
            swapchain,
            transform_matrix: matrix,
            acquire_semaphores,
            present_semaphores,
            cond_var: Condvar::new(),
            swapchain_device,
            _instance: device.instance.clone(),
            surface,
            device: device.clone(),
            vsync,
        })
    }

    pub fn pick_extent(
        capabilities: &vk::SurfaceCapabilitiesKHR,
        preferred_width: u32,
        preferred_height: u32,
    ) -> vk::Extent2D {
        if capabilities.current_extent.width != u32::MAX
            && capabilities.current_extent.height != u32::MAX
        {
            vk::Extent2D {
                width: capabilities.current_extent.width,
                height: capabilities.current_extent.height,
            }
        } else {
            vk::Extent2D {
                width: min(
                    max(preferred_width, capabilities.min_image_extent.width),
                    capabilities.max_image_extent.width,
                ),
                height: min(
                    max(preferred_height, capabilities.min_image_extent.height),
                    capabilities.max_image_extent.height,
                )
            }
        }
    }

    pub fn pick_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
        if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
            vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_UNORM,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            }
        } else {
            *formats
                .iter()
                .find(|&format| {
                    (format.format == vk::Format::B8G8R8A8_UNORM
                        && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
                        || (format.format == vk::Format::R8G8B8A8_UNORM
                            && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
                })
                .expect("No compatible format found")
        }
    }

    pub fn pick_image_count(capabilities: &vk::SurfaceCapabilitiesKHR, preferred: u32) -> u32 {
        let mut image_count = max(capabilities.min_image_count + 1, preferred);
        if capabilities.max_image_count != 0 {
            image_count = min(capabilities.max_image_count, image_count);
        }
        image_count
    }

    unsafe fn pick_present_mode(
        vsync: bool,
        present_modes: &[vk::PresentModeKHR],
    ) -> vk::PresentModeKHR {
        if !vsync {
            if let Some(mode) = present_modes
                .iter()
                .find(|&&mode| mode == vk::PresentModeKHR::IMMEDIATE)
            {
                return *mode;
            }

            if let Some(mode) = present_modes
                .iter()
                .find(|&&mode| mode == vk::PresentModeKHR::MAILBOX)
            {
                return *mode;
            }
        }

        *present_modes
            .iter()
            .find(|&&mode| mode == vk::PresentModeKHR::FIFO)
            .expect("No compatible present mode found")
    }

    pub fn width(&self) -> u32 {
        self.textures.first().unwrap().info().width
    }

    pub fn height(&self) -> u32 {
        self.textures.first().unwrap().info().height
    }

    pub(super) fn present(&mut self, queue: vk::Queue, backbuffer_indices: &VkBackbufferIndices) {
        {
            let present_info = vk::PresentInfoKHR {
                wait_semaphore_count: 1,
                p_wait_semaphores: &self.present_semaphores[backbuffer_indices.present_semaphore_index as usize].handle() as *const vk::Semaphore,
                swapchain_count: 1,
                p_swapchains: &self.swapchain as *const vk::SwapchainKHR,
                p_image_indices: &backbuffer_indices.texture_index as *const u32,
                p_results: std::ptr::null_mut(),
                ..Default::default()
            };
            let result = unsafe { self.swapchain_device.queue_present(queue, &present_info) };
            self.present_semaphore_counter += 1;

            match result {
                Ok(optimal) => {
                    if optimal && self.state == VkSwapchainState::Okay {
                        self.state = VkSwapchainState::Suboptimal;
                    }
                }
                Err(err) => {
                    match err {
                        vk::Result::ERROR_SURFACE_LOST_KHR => {
                            panic!("Vulkan surface lost");
                        }
                        vk::Result::ERROR_OUT_OF_DATE_KHR => {
                            self.state = VkSwapchainState::OutOfDate;
                        }
                        vk::Result::NOT_READY => {
                            todo!("Figure out not ready");
                        }
                        _ => {
                            panic!(
                                "Unknown error in present: {:?}",
                                result.err().unwrap()
                            );
                        }
                    }
                }
            }
        }
        self.cond_var.notify_all();
    }

    pub(crate) fn acquire_semaphore(&self, index: u32) -> &VkBinarySemaphore {
        &self.acquire_semaphores[index as usize]
    }

    pub(crate) fn present_semaphore(&self, index: u32) -> &VkBinarySemaphore {
        &self.present_semaphores[index as usize]
    }
}

impl Drop for VkSwapchain {
    fn drop(&mut self) {
        self.device.wait_for_idle();
        unsafe {
            self.swapchain_device
                .destroy_swapchain(self.swapchain, None)
        }
    }
}

impl Swapchain<VkBackend> for VkSwapchain {
    type Backbuffer = VkBackbufferIndices;

    fn format(&self) -> Format {
        self.textures.first().unwrap().info().format
    }

    fn surface(&self) -> &VkSurface {
        &self.surface
    }

    fn transform(&self) -> sourcerenderer_core::Matrix4 {
        self.transform_matrix
    }

    unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a VkBackbufferIndices) -> &'a VkTexture {
        &self.textures[backbuffer.texture_index as usize]
    }

    fn will_reuse_backbuffers(&self) -> bool {
        true
    }

    unsafe fn recreate(&mut self) {
        self.device.wait_for_idle();

        let info = self.textures.first().unwrap().info();
        let width = info.width;
        let height = info.height;

        let (swapchain, textures, matrix, _) = Self::create_swapchain_and_textures(&self.device, &self.swapchain_device, &self.surface, width, height, self.vsync, Some(&self.swapchain));
        self.swapchain = swapchain;
        self.textures = textures;
        self.transform_matrix = matrix;
    }

    unsafe fn next_backbuffer(&mut self) -> Result<VkBackbufferIndices, SwapchainError> {
        let max_distance = self.textures.len();
        assert!(self.acquire_semaphore_counter - self.present_semaphore_counter < max_distance as u64);

        let needs_recreate = match self.state {
            VkSwapchainState::OutOfDate => true,
            VkSwapchainState::Okay | VkSwapchainState::Suboptimal => false
        };
        if needs_recreate {
            return Err(SwapchainError::NeedsRecreation);
        }

        let acquire_counter = self.acquire_semaphore_counter;
        self.acquire_semaphore_counter += 1;
        let acquire_semaphore_index: usize = (acquire_counter % self.acquire_semaphores.len() as u64) as usize;
        let acquire_semaphore = &self.acquire_semaphores[acquire_semaphore_index];

        let result = unsafe {
            self.swapchain_device.acquire_next_image(
                self.swapchain,
                std::u64::MAX,
                acquire_semaphore.handle(),
                vk::Fence::null(),
            )
        };

        let present_semaphore_index = (self.present_semaphore_counter % self.present_semaphores.len() as u64) as usize;

        if let Ok((image_index, is_optimal)) = result {
            if !is_optimal && false {
                self.state = VkSwapchainState::Suboptimal;
            }
            Ok(VkBackbufferIndices {
                texture_index: image_index,
                acquire_semaphore_index: acquire_semaphore_index as u32,
                present_semaphore_index: present_semaphore_index as u32,
            })
        } else {
            // The semaphores are unaffect in the error case.
            match result.err().unwrap() {
                vk::Result::ERROR_SURFACE_LOST_KHR => {
                    panic!("Vulkan surface lost");
                }
                vk::Result::ERROR_OUT_OF_DATE_KHR => {
                    self.state = VkSwapchainState::OutOfDate;
                    Err(SwapchainError::NeedsRecreation)
                }
                vk::Result::NOT_READY => {
                    todo!("Figure out not ready");
                }
                _ => {
                    panic!(
                        "Unknown error in prepare_back_buffer: {:?}",
                        result.err().unwrap()
                    );
                }
            }
        }
    }

    fn width(&self) -> u32 {
        self.width()
    }

    fn height(&self) -> u32 {
        self.height()
    }
}

fn surface_vk_format_to_core(format: vk::Format) -> Format {
    match format {
        vk::Format::B8G8R8A8_UNORM => Format::BGRA8UNorm,
        vk::Format::R8G8B8A8_UNORM => Format::RGBA8UNorm,
        _ => panic!("Unsupported format: {:?}", format),
    }
}

pub struct VkBinarySemaphore {
    device: Arc<RawVkDevice>,
    semaphore: vk::Semaphore,
}

impl VkBinarySemaphore {
    pub fn new(device: &Arc<RawVkDevice>) -> Self {
        let semaphore = unsafe {
            device
                .create_semaphore(
                    &vk::SemaphoreCreateInfo {
                        flags: vk::SemaphoreCreateFlags::empty(),
                        ..Default::default()
                    },
                    None,
                )
                .unwrap()
        };
        Self {
            device: device.clone(),
            semaphore,
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> vk::Semaphore {
        self.semaphore
    }
}

impl PartialEq for VkBinarySemaphore {
    fn eq(&self, other: &Self) -> bool {
        self.semaphore == other.semaphore
    }
}

impl Eq for VkBinarySemaphore {}

impl Hash for VkBinarySemaphore {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.semaphore.hash(state);
    }
}

impl Drop for VkBinarySemaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}
