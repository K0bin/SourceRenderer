use std::marker::PhantomData;

use sourcerenderer_core::{
    Matrix4,
    Vec3,
    Vec4,
};

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
    shadow: PhantomData<u32>,
}

impl BoundingBox {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        assert!(min.x <= max.x);
        assert!(min.y <= max.y);
        assert!(min.z <= max.z);
        Self {
            min,
            max,
            shadow: PhantomData,
        }
    }

    pub fn transform(&self, matrix: &Matrix4) -> BoundingBox {
        let a = matrix * Vec4::new(self.min.x, self.min.y, self.min.z, 1f32);
        let b = matrix * Vec4::new(self.max.x, self.max.y, self.max.z, 1f32);
        BoundingBox::new(
            Vec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            Vec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        )
    }

    pub fn contains(&self, point: &Vec3) -> bool {
        self.min.x <= point.x
            && point.x < self.max.x
            && self.min.y <= point.y
            && point.y < self.max.y
            && self.min.z <= point.z
            && point.z < self.max.z
    }

    pub fn enlarge(&self, additional_size: &Vec3) -> BoundingBox {
        let mut bb = self.clone();
        bb.min.x -= additional_size.x * 0.5f32;
        bb.max.x += additional_size.x * 0.5f32;
        bb.min.y -= additional_size.y * 0.5f32;
        bb.max.y += additional_size.y * 0.5f32;
        bb.min.z -= additional_size.z * 0.5f32;
        bb.max.z += additional_size.z * 0.5f32;
        bb
    }
}
