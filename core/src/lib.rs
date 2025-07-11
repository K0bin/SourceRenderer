pub mod console;
pub mod gpu;
pub mod input;
pub mod platform;
pub mod pool;

pub mod atomic_refcell;

pub type Vec2 = bevy_math::Vec2;
pub type Vec3 = bevy_math::Vec3;
pub type Vec4 = bevy_math::Vec4;
pub type Vec2I = bevy_math::IVec2;
pub type Vec2UI = bevy_math::UVec2;
pub type Vec3UI = bevy_math::UVec3;
pub type Quaternion = bevy_math::Quat;
pub type Matrix4 = bevy_math::Mat4;
pub type Matrix3 = bevy_math::Mat3;
pub type EulerRot = bevy_math::EulerRot;

mod align;
pub use align::*;
mod fixed_size_vec;
pub use fixed_size_vec::*;

pub unsafe fn extend_lifetime<'b, T>(r: &'b T) -> &'static T {
    std::mem::transmute::<&'b T, &'static T>(r)
}
