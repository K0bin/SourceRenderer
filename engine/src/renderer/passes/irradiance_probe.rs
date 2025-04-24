use nalgebra::ComplexField;
use sourcerenderer_core::{
    Matrix4,
    Vec3,
};

// https://cseweb.ucsd.edu/~ravir/papers/envmap/
// https://handmade.network/p/75/monter/blog/p/7288-engine_work__global_illumination_with_irradiance_probes
// https://andrew-pham.blog/2019/08/26/spherical-harmonics/
fn prefilter_env_map(width: u32, hdr: &[Vec3]) -> [[f32; 9]; 3] {
    use core::f32::consts::PI;

    fn update_coeffs(coeffs: &mut [[f32; 9]; 3], hdr: Vec3, domega: f32, normal: Vec3) {
        /******************************************************************
         Update the coefficients (i.e. compute the next term in the
        integral) based on the lighting value hdr[3], the differential
        solid angle domega and cartesian components of surface normal x,y,z

        Inputs:  hdr = L(x,y,z) [note that x^2+y^2+z^2 = 1]
                  i.e. the illumination at position (x,y,z)

                  domega = The solid angle at the pixel corresponding to
            (x,y,z).  For these light probes, this is given by

            x,y,z  = Cartesian components of surface normal

        Notes:   Of course, there are better numerical methods to do
                  integration, but this naive approach is sufficient for our
            purpose.

        *********************************************************************/

        for col in 0..hdr.len() {
            let mut c: f32;

            /* L_{00}.  Note that Y_{00} = 0.282095 */
            c = 0.282095f32;
            coeffs[col][0] += hdr[col] * c * domega;

            /* L_{1m}. -1 <= m <= 1.  The linear terms */
            c = 0.488603f32;
            coeffs[col][1] += hdr[col] * (c * normal.y) * domega; /* Y_{1-1} = 0.488603 y  */
            coeffs[col][2] += hdr[col] * (c * normal.z) * domega; /* Y_{10}  = 0.488603 z  */
            coeffs[col][3] += hdr[col] * (c * normal.x) * domega; /* Y_{11}  = 0.488603 x  */

            /* The Quadratic terms, L_{2m} -2 <= m <= 2 */

            /* First, L_{2-2}, L_{2-1}, L_{21} corresponding to xy,yz,xz */
            c = 1.092548;
            coeffs[col][4] += hdr[col] * (c * normal.x * normal.y) * domega; /* Y_{2-2} = 1.092548 xy */
            coeffs[col][5] += hdr[col] * (c * normal.y * normal.z) * domega; /* Y_{2-1} = 1.092548 yz */
            coeffs[col][7] += hdr[col] * (c * normal.x * normal.z) * domega; /* Y_{21}  = 1.092548 xz */

            /* L_{20}.  Note that Y_{20} = 0.315392 (3z^2 - 1) */
            c = 0.315392;
            coeffs[col][6] += hdr[col] * (c * (3.0f32 * normal.z * normal.z - 1.0f32)) * domega;

            /* L_{22}.  Note that Y_{22} = 0.546274 (x^2 - y^2) */
            c = 0.546274;
            coeffs[col][8] += hdr[col] * (c * (normal.x * normal.x - normal.y * normal.y)) * domega;
        }
    }

    let mut coeffs = [[0f32; 9]; 3];
    let f_width = width as f32;
    for i in 0..width {
        let i_f = i as f32;
        for j in 0..width {
            let j_f = j as f32;
            /* We now find the cartesian components for the point (i,j) */
            let u: f32 = (j_f - f_width / 2.0f32) / (f_width / 2.0f32);
            let v: f32 = (f_width / 2.0f32 - i_f) / (f_width / 2.0f32);
            let r: f32 = (u * u + v * v).sqrt();

            if r > 1.0 {
                continue;
            }

            let theta: f32 = PI * r;
            let phi: f32 = v.atan2(u);
            let normal: Vec3 = Vec3::new(
                theta.sin() * phi.cos(),
                theta.sin() * phi.sin(),
                theta.cos(),
            );

            /* Computation of the solid angle.  This follows from some
            elementary calculus converting sin(theta) d theta d phi into
            coordinates in terms of r.  This calculation should be redone
            if the form of the input changes */

            let domega: f32 = (2.0f32 * PI / f_width) * (2.0f32 * PI / f_width) * theta.sinc();
            update_coeffs(&mut coeffs, hdr[(j * width + i) as usize], domega, normal);
        }
    }

    coeffs
}
