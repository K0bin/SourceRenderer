use std::sync::{
    Arc,
    Mutex,
};

use sourcerenderer_core::atomic_refcell::AtomicRefCell;

use sourcerenderer_core::{
    Matrix4,
    Vec4,
};

use crate::graphics::*;

use super::drawable::{
    make_camera_proj,
    make_camera_view,
    View,
};
use super::LateLatching;
use crate::fps_camera::{
    fps_camera_rotation,
    FPSCamera,
};
use crate::transform::interpolation::deconstruct_transform;

#[derive(Clone)]
#[repr(C)]
struct LateLatchCamerabuffer {
    view_proj: Matrix4,
    inv_proj: Matrix4,
    view: Matrix4,
    proj: Matrix4,
    inv_view: Matrix4,
    position: Vec4,
    inv_proj_view: Matrix4,
    z_near: f32,
    z_far: f32,
    aspect_ratio: f32,
    fov: f32,
}

pub struct LateLatchCamera<B: GPUBackend> {
    fps_camera: Mutex<FPSCamera>,
    buffer: AtomicRefCell<Arc<BufferSlice<B>>>,
    history_buffer: AtomicRefCell<Arc<BufferSlice<B>>>,
    aspect_ratio: f32,
    fov: f32,
    z_near: f32,
    z_far: f32,
}

impl<B: GPUBackend> LateLatching<B> for LateLatchCamera<B> {
    fn buffer(&self) -> Arc<BufferSlice<B>> {
        let buffer_ref = self.buffer.borrow();
        buffer_ref.clone()
    }

    fn history_buffer(&self) -> Option<Arc<BufferSlice<B>>> {
        let history_buffer_ref = self.history_buffer.borrow();
        Some(history_buffer_ref.clone())
    }

    fn before_recording(&self, _input: &crate::input::InputState, _view: &View) {}

    fn before_submit(&self, input: &crate::input::InputState, view: &View) {
        let mut fps_camera = self.fps_camera.lock().unwrap();
        let (position, _rotation, _) = deconstruct_transform(&view.camera_transform);
        let rotation = fps_camera_rotation(input, &mut fps_camera);

        let view = make_camera_view(position, rotation);
        let proj = make_camera_proj(self.fov, self.aspect_ratio, self.z_near, self.z_far);

        let buffer_mut = self.buffer.borrow();

        let buffer_data = LateLatchCamerabuffer {
            view: view,
            proj: proj,
            inv_view: view.try_inverse().unwrap(),
            inv_proj: proj.try_inverse().unwrap(),
            view_proj: proj * view,
            position: Vec4::new(position.x, position.y, position.z, 1f32),
            inv_proj_view: view.try_inverse().unwrap() * proj.try_inverse().unwrap(),
            z_near: self.z_near,
            z_far: self.z_far,
            aspect_ratio: self.aspect_ratio,
            fov: self.fov
        };
        buffer_mut.write(&buffer_data);

    }

    fn after_submit(&self, device: &crate::graphics::Device<B>) {
        let mut buffer_mut = self.buffer.borrow_mut();
        let mut history_buffer_mut = self.history_buffer.borrow_mut();
        *history_buffer_mut = std::mem::replace(&mut buffer_mut, Self::create_buffer(device).expect("Failed to allocate camera buffer"));
    }
}

impl<B: GPUBackend> LateLatchCamera<B> {
    pub fn new(device: &Device<B>, aspect_ratio: f32, fov: f32) -> Self {
        Self {
            fps_camera: Mutex::new(FPSCamera::new()),
            buffer: AtomicRefCell::new(Self::create_buffer(device).expect("Failed to allocate camera buffer")),
            history_buffer: AtomicRefCell::new(Self::create_buffer(device).expect("Failed to allocate camera buffer")),
            aspect_ratio,
            fov,
            z_near: 0.1f32,
            z_far: 100f32,
        }
    }

    fn create_buffer(device: &Device<B>) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        device.create_buffer(
            &BufferInfo {
                size: std::mem::size_of::<LateLatchCamerabuffer>() as u64,
                usage: BufferUsage::STORAGE | BufferUsage::CONSTANT,
                sharing_mode: QueueSharingMode::Concurrent
            },
            MemoryUsage::MainMemoryWriteCombined,
            None,
        )
    }

    pub fn z_near(&self) -> f32 {
        self.z_near
    }

    pub fn z_far(&self) -> f32 {
        self.z_far
    }

    pub fn fov(&self) -> f32 {
        self.fov
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }
}
