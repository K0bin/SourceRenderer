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

impl Copy for HalfVec3 {}

impl Debug for HalfVec3 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "HalfVec3({:?}, {:?}, {:?})",
            self.x, self.y, self.z
        ))
    }
}
