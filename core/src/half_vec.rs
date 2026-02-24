use crate::{Vec3, Vec4};
use bevy_math::vec4;
use half::f16;
use std::fmt::{Debug, Formatter};

pub struct HalfVec3 {
    pub x: f16,
    pub y: f16,
    pub z: f16,
}

impl HalfVec3 {
    pub fn new(x: f16, y: f16, z: f16) -> Self {
        Self { x, y, z }
    }
    pub fn new_from_f32(x: f32, y: f32, z: f32) -> Self {
        Self {
            x: f16::from_f32(x),
            y: f16::from_f32(y),
            z: f16::from_f32(z),
        }
    }
}

impl std::ops::Add<HalfVec3> for HalfVec3 {
    type Output = HalfVec3;

    fn add(self, rhs: HalfVec3) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Sub<HalfVec3> for HalfVec3 {
    type Output = HalfVec3;

    fn sub(self, rhs: HalfVec3) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Mul<f32> for HalfVec3 {
    type Output = HalfVec3;

    fn mul(self, rhs: f32) -> Self::Output {
        let rhs_f16 = f16::from_f32(rhs);
        Self {
            x: self.x * rhs_f16,
            y: self.y * rhs_f16,
            z: self.z * rhs_f16,
        }
    }
}

impl std::ops::Mul<HalfVec3> for f32 {
    type Output = HalfVec3;

    fn mul(self, rhs: HalfVec3) -> Self::Output {
        let self_f16 = f16::from_f32(self);
        HalfVec3 {
            x: self_f16 * rhs.x,
            y: self_f16 * rhs.y,
            z: self_f16 * rhs.z,
        }
    }
}

impl std::ops::Mul<Vec3> for HalfVec3 {
    type Output = HalfVec3;

    fn mul(self, rhs: Vec3) -> Self::Output {
        HalfVec3 {
            x: self.x * f16::from_f32(rhs.x),
            y: self.y * f16::from_f32(rhs.y),
            z: self.z * f16::from_f32(rhs.z),
        }
    }
}

impl std::ops::Mul<HalfVec3> for Vec3 {
    type Output = Vec3;

    fn mul(self, rhs: HalfVec3) -> Self::Output {
        Vec3 {
            x: self.x * rhs.x.to_f32(),
            y: self.y * rhs.y.to_f32(),
            z: self.z * rhs.z.to_f32(),
        }
    }
}

impl std::ops::Mul<HalfVec3> for HalfVec3 {
    type Output = HalfVec3;

    fn mul(self, rhs: HalfVec3) -> Self::Output {
        HalfVec3 {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
        }
    }
}

impl std::ops::Mul<f16> for HalfVec3 {
    type Output = HalfVec3;

    fn mul(self, rhs: f16) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl std::ops::Mul<HalfVec3> for f16 {
    type Output = HalfVec3;

    fn mul(self, rhs: HalfVec3) -> Self::Output {
        HalfVec3 {
            x: self * rhs.x,
            y: self * rhs.y,
            z: self * rhs.z,
        }
    }
}

impl Clone for HalfVec3 {
    fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

impl std::ops::Div<f32> for HalfVec3 {
    type Output = HalfVec3;

    fn div(self, rhs: f32) -> Self::Output {
        let rhs_f16 = f16::from_f32(rhs);
        Self {
            x: self.x / rhs_f16,
            y: self.y / rhs_f16,
            z: self.z / rhs_f16,
        }
    }
}

impl std::ops::Div<f16> for HalfVec3 {
    type Output = HalfVec3;

    fn div(self, rhs: f16) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl Copy for HalfVec3 {}

impl Debug for HalfVec3 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "HalfVec3({:?}, {:?}, {:?})",
            self.x, self.y, self.z
        ))
    }
}

pub struct HalfVec4 {
    pub x: f16,
    pub y: f16,
    pub z: f16,
    pub w: f16,
}

impl HalfVec4 {
    pub fn new(x: f16, y: f16, z: f16, w: f16) -> Self {
        Self { x, y, z, w }
    }
    pub fn new_from_f32(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            x: f16::from_f32(x),
            y: f16::from_f32(y),
            z: f16::from_f32(z),
            w: f16::from_f32(w),
        }
    }
}

impl std::ops::Add<HalfVec4> for HalfVec4 {
    type Output = HalfVec4;

    fn add(self, rhs: HalfVec4) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        }
    }
}

impl std::ops::Sub<HalfVec4> for HalfVec4 {
    type Output = HalfVec4;

    fn sub(self, rhs: HalfVec4) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
            w: self.w - rhs.w,
        }
    }
}

impl std::ops::Mul<f32> for HalfVec4 {
    type Output = HalfVec4;

    fn mul(self, rhs: f32) -> Self::Output {
        let rhs_f16 = f16::from_f32(rhs);
        Self {
            x: self.x * rhs_f16,
            y: self.y * rhs_f16,
            z: self.z * rhs_f16,
            w: self.w * rhs_f16,
        }
    }
}

impl std::ops::Mul<HalfVec4> for f32 {
    type Output = HalfVec4;

    fn mul(self, rhs: HalfVec4) -> Self::Output {
        let self_f16 = f16::from_f32(self);
        HalfVec4 {
            x: self_f16 * rhs.x,
            y: self_f16 * rhs.y,
            z: self_f16 * rhs.z,
            w: self_f16 * rhs.w,
        }
    }
}

impl std::ops::Mul<Vec4> for HalfVec4 {
    type Output = HalfVec4;

    fn mul(self, rhs: Vec4) -> Self::Output {
        HalfVec4 {
            x: self.x * f16::from_f32(rhs.x),
            y: self.y * f16::from_f32(rhs.y),
            z: self.z * f16::from_f32(rhs.z),
            w: self.w * f16::from_f32(rhs.w),
        }
    }
}

impl std::ops::Mul<HalfVec4> for Vec4 {
    type Output = Vec4;

    fn mul(self, rhs: HalfVec4) -> Self::Output {
        vec4(
            self.x * rhs.x.to_f32(),
            self.y * rhs.y.to_f32(),
            self.z * rhs.z.to_f32(),
            self.w * rhs.w.to_f32(),
        )
    }
}

impl std::ops::Mul<HalfVec4> for HalfVec4 {
    type Output = HalfVec4;

    fn mul(self, rhs: HalfVec4) -> Self::Output {
        HalfVec4 {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
            w: self.w * rhs.w,
        }
    }
}

impl std::ops::Mul<f16> for HalfVec4 {
    type Output = HalfVec4;

    fn mul(self, rhs: f16) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
            w: self.w * rhs,
        }
    }
}

impl std::ops::Mul<HalfVec4> for f16 {
    type Output = HalfVec4;

    fn mul(self, rhs: HalfVec4) -> Self::Output {
        HalfVec4 {
            x: self * rhs.x,
            y: self * rhs.y,
            z: self * rhs.z,
            w: self * rhs.w,
        }
    }
}

impl Clone for HalfVec4 {
    fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            z: self.z,
            w: self.w,
        }
    }
}

impl std::ops::Div<f32> for HalfVec4 {
    type Output = HalfVec4;

    fn div(self, rhs: f32) -> Self::Output {
        let rhs_f16 = f16::from_f32(rhs);
        Self {
            x: self.x / rhs_f16,
            y: self.y / rhs_f16,
            z: self.z / rhs_f16,
            w: self.w / rhs_f16,
        }
    }
}

impl std::ops::Div<f16> for HalfVec4 {
    type Output = HalfVec4;

    fn div(self, rhs: f16) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
            w: self.w / rhs,
        }
    }
}

impl Copy for HalfVec4 {}

impl Debug for HalfVec4 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "HalfVec4({:?}, {:?}, {:?}, {:?})",
            self.x, self.y, self.z, self.w
        ))
    }
}
