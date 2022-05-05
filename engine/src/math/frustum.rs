use sourcerenderer_core::{Matrix4, Vec3, Vec4};

use super::BoundingBox;

struct OrientedBoundingBox {
  center: Vec3,
  extents: Vec3,
  axes: [Vec3; 3]
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Frustum {
  near_half_width: f32,
  near_half_height: f32,
  z_near: f32,
  z_far: f32
}

impl Frustum {
  pub fn new(z_near: f32, z_far: f32, fov: f32, aspect_ratio: f32) -> Self {
    let near_half_height = (fov / 2f32).tan() * z_near;
    let near_half_width = near_half_height * aspect_ratio;
    Self {
      near_half_width,
      near_half_height,
      z_near: -z_near,
      z_far: -z_far
    }
  }

  pub fn intersects(&self, bounding_box: &BoundingBox, model_view: &Matrix4) -> bool {
    let mut corners = [
      (model_view * Vec4::new(bounding_box.min.x, bounding_box.min.y, bounding_box.min.z, 1f32)).xyz(),
      (model_view * Vec4::new(bounding_box.max.x, bounding_box.min.y, bounding_box.min.z, 1f32)).xyz(),
      (model_view * Vec4::new(bounding_box.min.x, bounding_box.max.y, bounding_box.min.z, 1f32)).xyz(),
      (model_view * Vec4::new(bounding_box.min.x, bounding_box.min.y, bounding_box.max.z, 1f32)).xyz()
    ];

    // The algorithm assumes a right hand frustum, so just invert z in view space
    for corner in &mut corners {
      corner.z = -corner.z;
    }

    let axes = [
      corners[1] - corners[0],
      corners[2] - corners[0],
      corners[3] - corners[0]
    ];
    let center = corners[0] + 0.5f32 * (axes[0] + axes[1] + axes[2]);
    let extents = Vec3::new(axes[0].magnitude(), axes[1].magnitude(), axes[2].magnitude());
    let normalized_axes = [
      axes[0] / extents.x,
      axes[1] / extents.y,
      axes[2] / extents.z
    ];
    let half_extents = extents * 0.5f32;
    let obb = OrientedBoundingBox {
      axes: normalized_axes,
      center,
      extents: half_extents
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
      let tau_0 = self.z_far;
      let tau_1 = self.z_near;

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
        let mo_c = m.dot(&obb.center);
        let mut obb_radius = 0f32;
        for i in 0..3 {
          obb_radius += (m.dot(&obb.axes[i])).abs() * obb.extents[i];
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
      for i in 0..obb.extents.len() {
        let m = &obb.axes[i];
        let mo_x = m.x.abs();
        let mo_y = m.y.abs();
        let mo_z = m.z;
        let mo_c = m.dot(&obb.center);
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
      for i in 0..obb.extents.len() {
        let m = Vec3::new(0f32, -obb.axes[i].z, obb.axes[i].y);
        let mo_x = 0f32;
        let mo_y = m.y.abs();
        let mo_z = m.z;
        let mo_c = m.y * obb.center.y + m.z * obb.center.z;
        let mut obb_radius = 0f32;
        for i in 0..3 {
          obb_radius += (m.dot(&obb.axes[i])).abs() * obb.extents[i];
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
      for i in 0..obb.extents.len() {
        let m = Vec3::new(obb.axes[i].z, 0f32, -obb.axes[i].y);
        let mo_x = m.x.abs();
        let mo_y = 0f32;
        let mo_z = m.z;
        let mo_c = m.x * obb.center.x + m.z * obb.center.z;
        let mut obb_radius = 0f32;
        for i in 0..3 {
          obb_radius += (m.dot(&obb.axes[i])).abs() * obb.extents[i];
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
          Vec3::new(-self.near_half_width, 0.0f32, self.z_near).cross(axis),
          Vec3::new(self.near_half_width, 0.0f32, self.z_near).cross(axis),
          Vec3::new(0f32, self.near_half_height, self.z_near).cross(axis),
          Vec3::new(0f32, -self.near_half_height, self.z_near).cross(axis)
        ];
        for m in m.iter() {
          let mo_x = m.x.abs();
          let mo_y = m.y.abs();
          let mo_z = m.z;
          const EPSILON: f32 = 0.0001f32;
          if mo_x < EPSILON && mo_y < EPSILON && mo_z.abs() < EPSILON {
            continue;
          }
          let mo_c = m.dot(&obb.center);
          let mut obb_radius = 0f32;
          for i in 0..3 {
            obb_radius += (m.dot(&obb.axes[i])).abs() * obb.extents[i];
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
}

// REF:
// https://bruop.github.io/improved_frustum_culling/
// http://davidlively.com/programming/graphics/frustum-calculation-and-culling-hopefully-demystified/
// https://gist.github.com/BruOp/60e862049ac6409d2fd4ec6fa5806b30
