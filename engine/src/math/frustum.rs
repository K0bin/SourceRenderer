use bevy_math::Vec4Swizzles;
use sourcerenderer_core::{
    Matrix4,
    Vec3,
    Vec4,
};

use super::BoundingBox;

struct OrientedBoundingBox {
    center: Vec3,
    extents: Vec3,
    axes: [Vec3; 3],
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Frustum {
    pub near_half_width: f32,
    pub near_half_height: f32,
    z_near: f32,
    z_far: f32,
}

impl Frustum {
    pub fn new(z_near: f32, z_far: f32, fov: f32, aspect_ratio: f32) -> Self {
        let near_half_width = (fov / 2f32).tan() * z_near;
        let near_half_height = near_half_width / aspect_ratio;
        Self {
            near_half_width,
            near_half_height,
            z_near,
            z_far,
        }
    }

    pub fn intersects(&self, bounding_box: &BoundingBox, model_view: &Matrix4) -> bool {
        let model_space_center = (bounding_box.min + bounding_box.max) * 0.5;
        let model_space_extents = (bounding_box.max - bounding_box.min) * 0.5;

        // Transform center to view space
        let center = (model_view * Vec4::new(model_space_center.x, model_space_center.y, model_space_center.z, 1.0)).xyz();

        // Directly extract axes from the view matrix
        let axis_x = model_view.col(0).xyz();
        let axis_y = model_view.col(1).xyz();
        let axis_z = model_view.col(2).xyz();

        let len_x = axis_x.length();
        let len_y = axis_y.length();
        let len_z = axis_z.length();

        // Normalize per axis without NaN
        const EPSILON: f32 = 1e-6;
        let normalized_axes = [
            if len_x > EPSILON { axis_x / len_x } else { Vec3::new(1.0, 0.0, 0.0) },
            if len_y > EPSILON { axis_y / len_y } else { Vec3::new(0.0, 1.0, 0.0) },
            if len_z > EPSILON { axis_z / len_z } else { Vec3::new(0.0, 0.0, 1.0) },
        ];

        let obb = OrientedBoundingBox {
            axes: normalized_axes,
            center,
            extents: Vec3::new(
                model_space_extents.x * len_x,
                model_space_extents.y * len_y,
                model_space_extents.z * len_z,
            ),
        };

        // frustum near and far planes
        {
            let mo_c = obb.center.z;
            let mut radius = 0f32;
            for i in 0..3 {
                radius += obb.axes[i].z.abs() * obb.extents[i];
            }
            let obb_min = mo_c - radius;
            let obb_max = mo_c + radius;
            let tau_0 = self.z_near;
            let tau_1 = self.z_far;

            if obb_min > tau_1 || obb_max < tau_0 {
                return false;
            }
        }

        // remaining frustum planes
        {
            let frustum_normals = [
                Vec3::new(0f32, -self.z_near, self.near_half_height),
                Vec3::new(0f32, self.z_near, self.near_half_height),
                Vec3::new(-self.z_near, 0f32, self.near_half_width),
                Vec3::new(self.z_near, 0f32, self.near_half_width),
            ];
            for m in frustum_normals.iter() {
                let mo_x = m.x.abs();
                let mo_y = m.y.abs();
                let mo_z = m.z;
                let mo_c = m.dot(obb.center);
                let mut obb_radius = 0f32;
                for i in 0..3 {
                    obb_radius += (m.dot(obb.axes[i])).abs() * obb.extents[i];
                }
                let obb_min = mo_c - obb_radius;
                let obb_max = mo_c + obb_radius;
                let p = self.near_half_width * mo_x + self.near_half_height * mo_y;

                let mut tau_0 = self.z_near * mo_z - p;
                let mut tau_1 = self.z_near * mo_z + p;

                if tau_0 < 0f32 {
                    tau_0 *= self.z_far / self.z_near;
                }
                if tau_1 > 0f32 {
                    tau_1 *= self.z_far / self.z_near;
                }

                if obb_min > tau_1 || obb_max < tau_0 {
                    return false;
                }
            }
        }

        // OBB axes
        {
            for i in 0..3 {
                let m = &obb.axes[i];
                let mo_x = m.x.abs();
                let mo_y = m.y.abs();
                let mo_z = m.z;
                let mo_c = m.dot(obb.center);
                let obb_radius = obb.extents[i];
                let obb_min = mo_c - obb_radius;
                let obb_max = mo_c + obb_radius;
                let p = self.near_half_width * mo_x + self.near_half_height * mo_y;

                let mut tau_0 = self.z_near * mo_z - p;
                let mut tau_1 = self.z_near * mo_z + p;

                if tau_0 < 0f32 {
                    tau_0 *= self.z_far / self.z_near;
                }
                if tau_1 > 0f32 {
                    tau_1 *= self.z_far / self.z_near;
                }

                if obb_min > tau_1 || obb_max < tau_0 {
                    return false;
                }
            }
        }

        // cross products between the edges
        // R x A_i
        {
            for i in 0..3 {
                let m = Vec3::new(0f32, -obb.axes[i].z, obb.axes[i].y);
                let mo_x = 0f32;
                let mo_y = m.y.abs();
                let mo_z = m.z;
                let mo_c = m.y * obb.center.y + m.z * obb.center.z;
                let mut obb_radius = 0f32;
                for i in 0..3 {
                    obb_radius += (m.dot(obb.axes[i])).abs() * obb.extents[i];
                }
                let obb_min = mo_c - obb_radius;
                let obb_max = mo_c + obb_radius;
                let p = self.near_half_width * mo_x + self.near_half_height * mo_y;

                let mut tau_0 = self.z_near * mo_z - p;
                let mut tau_1 = self.z_near * mo_z + p;

                if tau_0 < 0f32 {
                    tau_0 *= self.z_far / self.z_near;
                }
                if tau_1 > 0f32 {
                    tau_1 *= self.z_far / self.z_near;
                }

                if obb_min > tau_1 || obb_max < tau_0 {
                    return false;
                }
            }
        }

        // U x A_i
        {
            for i in 0..3 {
                let m = Vec3::new(obb.axes[i].z, 0f32, -obb.axes[i].x);
                let mo_x = m.x.abs();
                let mo_y = 0f32;
                let mo_z = m.z;
                let mo_c = m.x * obb.center.x + m.z * obb.center.z;
                let mut obb_radius = 0f32;
                for i in 0..3 {
                    obb_radius += (m.dot(obb.axes[i])).abs() * obb.extents[i];
                }
                let obb_min = mo_c - obb_radius;
                let obb_max = mo_c + obb_radius;
                let p = self.near_half_width * mo_x + self.near_half_height * mo_y;

                let mut tau_0 = self.z_near * mo_z - p;
                let mut tau_1 = self.z_near * mo_z + p;

                if tau_0 < 0f32 {
                    tau_0 *= self.z_far / self.z_near;
                }
                if tau_1 > 0f32 {
                    tau_1 *= self.z_far / self.z_near;
                }

                if obb_min > tau_1 || obb_max < tau_0 {
                    return false;
                }
            }
        }

        // Frustum edge x A_i
        {
            for axis in &obb.axes {
                let m = [
                    Vec3::new(-self.near_half_width,  self.near_half_height, self.z_near).cross(*axis), // Top-Left
                    Vec3::new( self.near_half_width,  self.near_half_height, self.z_near).cross(*axis), // Top-Right
                    Vec3::new(-self.near_half_width, -self.near_half_height, self.z_near).cross(*axis), // Bottom-Left
                    Vec3::new( self.near_half_width, -self.near_half_height, self.z_near).cross(*axis), // Bottom-Right
                ];
                for m in m.iter() {
                    let mo_x = m.x.abs();
                    let mo_y = m.y.abs();
                    let mo_z = m.z;
                    const EPSILON: f32 = 0.0001f32;
                    if mo_x < EPSILON && mo_y < EPSILON && mo_z.abs() < EPSILON {
                        continue;
                    }
                    let mo_c = m.dot(obb.center);
                    let mut obb_radius = 0f32;
                    for i in 0..3 {
                        obb_radius += (m.dot(obb.axes[i])).abs() * obb.extents[i];
                    }
                    let obb_min = mo_c - obb_radius;
                    let obb_max = mo_c + obb_radius;
                    let p = self.near_half_width * mo_x + self.near_half_height * mo_y;

                    let mut tau_0 = self.z_near * mo_z - p;
                    let mut tau_1 = self.z_near * mo_z + p;

                    if tau_0 < 0f32 {
                        tau_0 *= self.z_far / self.z_near;
                    }
                    if tau_1 > 0f32 {
                        tau_1 *= self.z_far / self.z_near;
                    }

                    if obb_min > tau_1 || obb_max < tau_0 {
                        return false;
                    }
                }
            }
        }

        true
    }

    pub fn extract_planes(proj: &Matrix4) -> (Vec4, Vec4) {
        // http://www.cs.otago.ac.nz/postgrads/alexis/planeExtraction.pdf
        let transposed_proj = proj.transpose();
        let frustum_x = normalize_plane(transposed_proj.col(3) + transposed_proj.col(0)); // x + w < 0
        let frustum_y = normalize_plane(transposed_proj.col(3) + transposed_proj.col(1)); // y + w < 0
        (frustum_x, frustum_y)
    }
}

fn normalize_plane(p: Vec4) -> Vec4 {
    p / p.xyz().length()
}

// REF:
// https://bruop.github.io/improved_frustum_culling/
// http://davidlively.com/programming/graphics/frustum-calculation-and-culling-hopefully-demystified/
// https://gist.github.com/BruOp/60e862049ac6409d2fd4ec6fa5806b30
