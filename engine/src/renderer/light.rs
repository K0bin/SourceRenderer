use std::sync::Arc;

use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::Vec3;

use crate::graphics::*;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PointLight {
    pub position: Vec3,
    pub intensity: f32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct DirectionalLight {
    pub direction: Vec3,
    pub intensity: f32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CullingPointLight {
    pub position: Vec3,
    pub radius: f32,
}

#[derive(Clone)]
pub struct RendererDirectionalLight<B: GPUBackend> {
    pub direction: Vec3,
    pub intensity: f32,
    pub shadow_map: AtomicRefCell<Option<Arc<Texture<B>>>>,
}

impl<B: GPUBackend> RendererDirectionalLight<B> {
    pub fn new(direction: Vec3, intensity: f32) -> Self {
        Self {
            direction,
            intensity,
            shadow_map: AtomicRefCell::new(None),
        }
    }
}

#[derive(Clone)]
pub struct RendererPointLight<B: GPUBackend> {
    pub position: Vec3,
    pub intensity: f32,
    pub shadow_map: AtomicRefCell<Option<Arc<Texture<B>>>>,
}

impl<B: GPUBackend> RendererPointLight<B> {
    pub fn new(position: Vec3, intensity: f32) -> Self {
        Self {
            position,
            intensity,
            shadow_map: AtomicRefCell::new(None),
        }
    }
}
