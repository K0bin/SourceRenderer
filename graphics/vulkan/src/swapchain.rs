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
        }, Arc, Mutex, MutexGuard
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
#[repr(u32)]
pub enum VkSwapchainState {
    Okay,
    Suboptimal,
    OutOfDate,
    Retired,
}

pub struct VkSwapchain {
    textures: SmallVec<[VkTexture; 5]>,
    acquire_semaphores: SmallVec<[VkBinarySemaphore; 5]>,
    present_semaphores: SmallVec<[VkBinarySemaphore; 5]>,
    swapchain: Mutex<vk::SwapchainKHR>,
    swapchain_loader: SwapchainLoader,
    instance: Arc<RawVkInstance>,
    surface: Option<VkSurface>,
    device: Arc<RawVkDevice>,
    vsync: bool,
    state: AtomicCell<VkSwapchainState>,
    semaphore_index: AtomicU64,
    image_index: AtomicU32,
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

            let acquire_semaphores: SmallVec<[VkBinarySemaphore; 5]> = (0..textures.len())
                .map(|_i| VkBinarySemaphore::new(device))
                .collect();

            let present_semaphores: SmallVec<[VkBinarySemaphore; 5]> = (0..textures.len())
                .map(|_i| VkBinarySemaphore::new(device))
                .collect();

            Ok(VkSwapchain {
                textures,
                acquire_semaphores,
                present_semaphores,
                semaphore_index: AtomicU64::new(0),
                image_index: AtomicU32::new(0),
                swapchain: Mutex::new(swapchain),
                swapchain_loader,
                instance: device.instance.clone(),
                surface: Some(surface),
                device: device.clone(),
                vsync,
                state: AtomicCell::new(VkSwapchainState::Okay),
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

    pub fn handle(&self) -> MutexGuard<vk::SwapchainKHR> {
        self.swapchain.lock().unwrap()
    }

    pub fn width(&self) -> u32 {
        self.textures.first().unwrap().info().width
    }

    pub fn height(&self) -> u32 {
        self.textures.first().unwrap().info().height
    }

    #[allow(clippy::logic_bug)]
    pub unsafe fn acquire_back_buffer(&self) -> VkResult<(u32, bool)> {
        let index: usize = ((self.semaphore_index.fetch_add(1, Ordering::AcqRel) + 1) % self.acquire_semaphores.len() as u64) as usize;
        let semaphore = &self.acquire_semaphores[index];

        let result = {
            let swapchain_handle = self.handle();
            self.swapchain_loader.acquire_next_image(
                *swapchain_handle,
                std::u64::MAX,
                semaphore.handle(),
                vk::Fence::null(),
            )
        };
        if let Ok((image, is_optimal)) = result {
            if !is_optimal && false {
                self.set_state(VkSwapchainState::Suboptimal);
            }
            self.image_index.store(image, Ordering::Release);
        } else {
            match result.err().unwrap() {
                vk::Result::ERROR_SURFACE_LOST_KHR => {
                    if let Some(surface) = self.surface.as_ref() {
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

    pub(super) fn set_state(&self, state: VkSwapchainState) {
        self.state.store(state);
    }

    pub(super) fn state(&self) -> VkSwapchainState {
        self.state.load()
    }

    pub(super) fn acquire_semaphore(&self) -> &VkBinarySemaphore {
        let index = (self.semaphore_index.load(Ordering::Acquire) % self.acquire_semaphores.len() as u64) as usize;
        &self.acquire_semaphores[index]
    }

    pub(super) fn present_semaphore(&self) -> &VkBinarySemaphore {
        let index = (self.semaphore_index.load(Ordering::Acquire) % self.present_semaphores.len() as u64) as usize;
        &self.present_semaphores[index]
    }
}

impl Drop for VkSwapchain {
    fn drop(&mut self) {
        self.device.wait_for_idle();
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(*self.handle(), None)
        }
    }
}

impl Swapchain<VkBackend> for VkSwapchain {
    unsafe fn recreate(mut old: Self, width: u32, height: u32) -> Result<Self, SwapchainError> {
        let state = old.state();
        let old_sc_handle = *old.handle();
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
        let old_sc_handle = *old.handle();
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

    fn backbuffer(&self, index: u32) -> &VkTexture {
        &self.textures[index as usize]
    }

    fn backbuffer_index(&self) -> u32 {
        self.image_index.load(Ordering::Acquire)
    }

    fn backbuffer_count(&self) -> u32 {
        self.textures.len() as u32
    }

    unsafe fn next_backbuffer(&self) -> Result<(), SwapchainError> {
        let _ = self.acquire_back_buffer()
            .map_err(|e| match e {
                vk::Result::ERROR_OUT_OF_DATE_KHR => SwapchainError::Other,
                vk::Result::ERROR_SURFACE_LOST_KHR => SwapchainError::SurfaceLost,
                _ => SwapchainError::Other
            })?;
        Ok(())
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
