pub mod console;
pub mod gpu;
pub mod input;
pub mod platform;
pub mod pool;

pub type Vec2 = bevy_math::Vec2;
pub type Vec3 = bevy_math::Vec3;
pub type Vec4 = bevy_math::Vec4;
pub type Vec2I = bevy_math::IVec2;
pub type Vec2UI = bevy_math::UVec2;
pub type Vec3UI = bevy_math::UVec3;
pub type Vec3I = bevy_math::UVec3;
pub type Vec4I = bevy_math::IVec4;
pub type Vec4UI = bevy_math::UVec4;
pub type Quaternion = bevy_math::Quat;
pub type Matrix4 = bevy_math::Mat4;
pub type Matrix3 = bevy_math::Mat3;
pub type EulerRot = bevy_math::EulerRot;
pub use half::f16;
pub use half_vec::HalfVec3;
pub use half_vec::HalfVec4;

mod align;
pub use align::*;
mod fixed_size_vec;
mod half_vec;

pub use fixed_size_vec::*;

pub unsafe fn extend_lifetime<'b, T>(r: &'b T) -> &'static T {
    unsafe { std::mem::transmute::<&'b T, &'static T>(r) }
}

#[proc_macro]
pub fn file_exists(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Extract the string literal from the macro input
    let input_str = input.to_string();
    let filename = input_str.trim_matches('"');

    // Check existence relative to the workspace/compilation directory
    let exists = std::path::Path::new(filename).exists();

    // Return a boolean literal as a token stream
    if exists {
        "true".parse().unwrap()
    } else {
        "false".parse().unwrap()
    }
}