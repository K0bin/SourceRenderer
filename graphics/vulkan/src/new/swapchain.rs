use std::{
    cmp::{
        max,
        min,
    },
    sync::{
        atomic::{
            AtomicU32,
            AtomicU64,
            Ordering,
        },
        Arc,
    },
};

use ash::{
    extensions::khr::Swapchain as SwapchainLoader,
    prelude::VkResult,
    vk,
    vk::SurfaceTransformFlagsKHR,
};
use crossbeam_utils::atomic::AtomicCell;
use smallvec::SmallVec;
use sourcerenderer_core::{
    gpu::*,
    Matrix4,
    Vec3,
};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VkSwapchainState {
    Okay,
    Suboptimal,
    OutOfDate,
    Retired,
}

pub struct VkSwapchain {
    textures: SmallVec<[VkTexture; 5]>,
    views: SmallVec<[VkTextureView; 5]>,
    acquire_semaphores: SmallVec<[VkBinarySemaphore; 5]>,
    present_semaphores: SmallVec<[VkBinarySemaphore; 5]>,
    semaphore_index: u32,
    swapchain: vk::SwapchainKHR,
    swapchain_loader: SwapchainLoader,
    instance: Arc<RawVkInstance>,
    surface: Option<VkSurface>,
    device: Arc<RawVkDevice>,
    vsync: bool,
    state: VkSwapchainState,
    acquired_image: u32,
    presented_image: u32,
    transform_matrix: Matrix4,
}

impl VkSwapchain {
    fn new_internal(
        vsync: bool,
        width: u32,
        height: u32,
        device: &Arc<RawVkDevice>,
        mut surface: VkSurface,
        old_swapchain: Option<vk::SwapchainKHR>,
    ) -> Result<Self, SwapchainError> {
        if surface.is_lost() {
            return Err(SwapchainError::SurfaceLost);
        }

        let vk_device = &device.device;
        let instance = &device.instance;

        unsafe {
            let physical_device = device.physical_device;
            let present_modes = match surface.get_present_modes(&physical_device) {
                Ok(present_modes) => present_modes,
                Err(e) => match e {
                    vk::Result::ERROR_SURFACE_LOST_KHR => {
                        surface.mark_lost();
                        return Err(SwapchainError::SurfaceLost);
                    }
                    _ => {
                        panic!("Could not get surface modes: {:?}", e);
                    }
                },
            };
            let present_mode = VkSwapchain::pick_present_mode(vsync, &present_modes);
            let swapchain_loader = SwapchainLoader::new(&instance.instance, vk_device);

            let capabilities = match surface.get_capabilities(&physical_device) {
                Ok(capabilities) => capabilities,
                Err(e) => match e {
                    vk::Result::ERROR_SURFACE_LOST_KHR => {
                        surface.mark_lost();
                        return Err(SwapchainError::SurfaceLost);
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
                        surface.mark_lost();
                        return Err(SwapchainError::SurfaceLost);
                    }
                    _ => {
                        panic!("Could not get surface formats: {:?}", e);
                    }
                },
            };
            let format = VkSwapchain::pick_format(&formats);
            println!("format: {:?}", format);

            let (width, height) = VkSwapchain::pick_extent(&capabilities, width, height);
            let extent = vk::Extent2D { width, height };

            if width == 0 || height == 0 {
                return Err(SwapchainError::ZeroExtents);
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
                SurfaceTransformFlagsKHR::ROTATE_90 => (
                    Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2),
                    SurfaceTransformFlagsKHR::ROTATE_90,
                ),
                SurfaceTransformFlagsKHR::ROTATE_180 => (
                    Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::PI),
                    SurfaceTransformFlagsKHR::ROTATE_180,
                ),
                SurfaceTransformFlagsKHR::ROTATE_270 => (
                    Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2 * 3f32),
                    SurfaceTransformFlagsKHR::ROTATE_270,
                ),
                SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR => (
                    Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32)),
                    SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR,
                ),
                SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90 => (
                    Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32))
                        * Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2),
                    SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90,
                ),
                SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180 => (
                    Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32))
                        * Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::PI),
                    SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180,
                ),
                SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270 => (
                    Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32))
                        * Matrix4::from_euler_angles(
                            0f32,
                            0f32,
                            -std::f32::consts::FRAC_PI_2 * 3f32,
                        ),
                    SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270,
                ),
                _ => (Matrix4::identity(), SurfaceTransformFlagsKHR::IDENTITY),
            };

            // TODO: Rendering is broken with actual pretransform
            let matrix = Matrix4::identity();
            let transform = SurfaceTransformFlagsKHR::IDENTITY;

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
                    old_swapchain: old_swapchain.unwrap_or(vk::SwapchainKHR::default()),
                    ..Default::default()
                };

                swapchain_loader
                    .create_swapchain(&swapchain_create_info, None)
                    .map_err(|e| match e {
                        vk::Result::ERROR_SURFACE_LOST_KHR => {
                            surface.mark_lost();
                            SwapchainError::SurfaceLost
                        }
                        _ => {
                            panic!(
                                "Creating swapchain failed {:?}, old swapchain is: {:?}",
                                e, swapchain_create_info.old_swapchain
                            );
                        }
                    })?
            };

            let swapchain_images = swapchain_loader.get_swapchain_images(swapchain).unwrap();
            let textures: SmallVec<[VkTexture; 5]> = swapchain_images
                .iter()
                .map(|image| {
                    VkTexture::from_image(
                        device,
                        *image,
                        TextureInfo {
                            dimension: TextureDimension::Dim2D,
                            format: surface_vk_format_to_core(format.format),
                            width,
                            height,
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

            let swapchain_image_views: SmallVec<[VkTextureView; 5]> = textures
                .iter()
                .enumerate()
                .map(|(index, texture)| {
                    VkTextureView::new(
                        device,
                        texture,
                        &TextureViewInfo::default(),
                        Some(&format!("Backbuffer view {}", index)),
                    )
                })
                .collect();

            let acquire_semaphores: SmallVec<[VkBinarySemaphore; 5]> = (0..textures.len())
                .map(|_i| VkBinarySemaphore::new(device))
                .collect();

            let present_semaphores: SmallVec<[VkBinarySemaphore; 5]> = (0..textures.len())
                .map(|_i| VkBinarySemaphore::new(device))
                .collect();

            Ok(VkSwapchain {
                textures,
                views: swapchain_image_views,
                acquire_semaphores,
                present_semaphores,
                semaphore_index: 0,
                swapchain: swapchain,
                swapchain_loader,
                instance: device.instance.clone(),
                surface: Some(surface),
                device: device.clone(),
                vsync,
                state: VkSwapchainState::Okay,
                presented_image: 0,
                acquired_image: 0,
                transform_matrix: matrix,
            })
        }
    }

    pub fn new(
        vsync: bool,
        width: u32,
        height: u32,
        device: &Arc<RawVkDevice>,
        surface: VkSurface,
    ) -> Result<Self, SwapchainError> {
        VkSwapchain::new_internal(vsync, width, height, device, surface, None)
    }

    pub fn pick_extent(
        capabilities: &vk::SurfaceCapabilitiesKHR,
        preferred_width: u32,
        preferred_height: u32,
    ) -> (u32, u32) {
        if capabilities.current_extent.width != u32::MAX
            && capabilities.current_extent.height != u32::MAX
        {
            (
                capabilities.current_extent.width,
                capabilities.current_extent.height,
            )
        } else {
            (
                min(
                    max(preferred_width, capabilities.min_image_extent.width),
                    capabilities.max_image_extent.width,
                ),
                min(
                    max(preferred_height, capabilities.min_image_extent.height),
                    capabilities.max_image_extent.height,
                ),
            )
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

    pub fn loader(&self) -> &SwapchainLoader {
        &self.swapchain_loader
    }

    pub fn handle(&self) -> vk::SwapchainKHR {
        self.swapchain
    }

    pub fn width(&self) -> u32 {
        self.textures.first().unwrap().info().width
    }

    pub fn height(&self) -> u32 {
        self.textures.first().unwrap().info().height
    }

    #[allow(clippy::logic_bug)]
    pub unsafe fn acquire_back_buffer(&mut self) -> VkResult<(u32, bool)> {
        let index = self.semaphore_index as usize % self.acquire_semaphores.len();
        self.semaphore_index += 1;
        let semaphore = &self.acquire_semaphores[index];

        while self.presented_image != self.acquired_image {}
        let result = {
            let swapchain_handle = self.handle();
            self.swapchain_loader.acquire_next_image(
                swapchain_handle,
                std::u64::MAX,
                semaphore.handle(),
                vk::Fence::null(),
            )
        };
        if let Ok((image, is_optimal)) = result {
            if !is_optimal && false {
                self.set_state(VkSwapchainState::Suboptimal);
            }
            self.acquired_image = image;
        } else {
            match result.err().unwrap() {
                vk::Result::ERROR_SURFACE_LOST_KHR => {
                    if let Some(surface) = self.surface.as_mut() {
                        surface.mark_lost();
                    }
                    self.set_state(VkSwapchainState::Retired);
                }
                vk::Result::ERROR_OUT_OF_DATE_KHR => {
                    #[cfg(target_os = "android")]
                    {
                        // I guess we can not recreate the SC on OUT_OF_DATE
                        self.surface.mark_lost();
                    }

                    self.set_state(VkSwapchainState::OutOfDate);
                }
                _ => {
                    panic!(
                        "Unknown error in prepare_back_buffer: {:?}",
                        result.err().unwrap()
                    );
                }
            }
        }
        result
    }

    pub(crate) fn set_presented_image(&mut self, presented_image_index: u32) {
        self.presented_image = presented_image_index
    }

    pub fn set_state(&mut self, state: VkSwapchainState) {
        self.state = state;
    }

    pub fn state(&self) -> VkSwapchainState {
        self.state
    }

    pub fn acquired_image(&self) -> u32 {
        self.acquired_image
    }
}

impl Drop for VkSwapchain {
    fn drop(&mut self) {
        self.device.wait_for_idle();
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None)
        }
    }
}

impl Swapchain<VkBackend> for VkSwapchain {
    unsafe fn recreate(mut old: Self, width: u32, height: u32) -> Result<Self, SwapchainError> {
        let state = old.state();
        let old_sc_handle = old.handle();
        let surface = std::mem::replace(&mut old.surface, None).unwrap();
        old.set_state(VkSwapchainState::Retired);

        println!("Recreating swapchain");
        VkSwapchain::new_internal(
            old.vsync,
            width,
            height,
            &old.device,
            surface,
            if old.state() == VkSwapchainState::Retired {
                None
            } else {
                Some(old_sc_handle)
            },
        )
    }

    unsafe fn recreate_on_surface(
        mut old: Self,
        surface: VkSurface,
        width: u32,
        height: u32,
    ) -> Result<Self, SwapchainError> {
        let state = old.state();
        let old_sc_handle = old.handle();
        old.set_state(VkSwapchainState::Retired);
        let surface = std::mem::replace(&mut old.surface, None).unwrap();

        println!("Recreating swapchain on new surface");
        VkSwapchain::new_internal(
            old.vsync,
            width,
            height,
            &old.device,
            surface,
            if old.state() == VkSwapchainState::Retired {
                None
            } else {
                Some(old_sc_handle)
            },
        )
    }

    fn sample_count(&self) -> SampleCount {
        self.textures.first().unwrap().info().samples
    }

    fn format(&self) -> Format {
        self.textures.first().unwrap().info().format
    }

    fn surface(&self) -> &VkSurface {
        self.surface.as_ref().unwrap()
    }

    fn transform(&self) -> sourcerenderer_core::Matrix4 {
        self.transform_matrix
    }

    unsafe fn prepare_back_buffer(&mut self) -> Option<PreparedBackBuffer<'_, VkBackend>> {
        let res: Result<(u32, bool), vk::Result> = self.acquire_back_buffer();
        res.ok()
            .and_then(move |(img_index, _optimal)| // TODO: handle optimal
        //optimal.then(||
        Some(
            PreparedBackBuffer {
                texture_view: self.views.get(img_index as usize).unwrap(),
                prepare_fence: &self.acquire_semaphores[img_index as usize],
                present_fence: &self.present_semaphores[img_index as usize],
            }
        ))
    }

    fn width(&self) -> u32 {
        self.width()
    }

    fn height(&self) -> u32 {
        self.height()
    }
}

pub(crate) enum VkSwapchainAcquireResult<'a> {
    Success {
        back_buffer: &'a Arc<VkTexture>,
        back_buffer_index: u32,
    },
    SubOptimal {
        back_buffer: &'a Arc<VkTexture>,
        back_buffer_index: u32,
    },
    Broken,
    DeviceLost,
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

    pub fn handle(&self) -> vk::Semaphore {
        self.semaphore
    }
}

impl Drop for VkBinarySemaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}

impl WSIFence for VkBinarySemaphore {}